use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::{BTreeSet, HashMap};

const NOVAS_EXTRACTED_DIR: &str = "novasc3.1";
const NOVAS_UPSTREAM_VERSION: &str = "3.1";
const EMSDK_DEFAULT_VERSION: &str = "5.0.6";

struct EmscriptenToolchain {
	emsdk_root: Option<PathBuf>,
	emcc: PathBuf,
	emar: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-env-changed=NOVAS_EMSDK_VERSION");

	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
	let out_dir = PathBuf::from(env::var("OUT_DIR")?);
	let host_target = env::var("HOST")?;
	let target = env::var("TARGET")?;
	let target_arch = env::var("CARGO_CFG_TARGET_ARCH")?;
	let target_os = env::var("CARGO_CFG_TARGET_OS")?;

	let cache_dir = target_cache_dir(&manifest_dir)?;
	fs::create_dir_all(&cache_dir)?;

	let source_dir = manifest_dir.join(NOVAS_EXTRACTED_DIR);
	if !source_dir.exists() {
		return Err(format!(
			"expected source directory '{}' in workspace",
			source_dir.display()
		)
		.into());
	}

	apply_compatibility_patches(&source_dir)?;
	let cio_ra_bin_path = generate_cio_ra_bin(&source_dir, &out_dir)?;
	generate_bindings(&source_dir, &out_dir, &host_target)?;
	generate_root_reexports(&out_dir)?;
	generate_convenience_api(&source_dir, &out_dir)?;
	let mut emscripten_toolchain = None;

	let c_target = if target_arch == "wasm32" {
		let toolchain = resolve_emscripten_toolchain(&cache_dir)?;
		setup_emscripten_environment(&toolchain);
		emscripten_toolchain = Some(toolchain);
		let toolchain_ref = emscripten_toolchain.as_ref().expect("toolchain present");

		if target_os == "unknown" {
			setup_wasm_unknown_linking(&toolchain_ref.emcc)?;
			"wasm32-unknown-emscripten"
		} else if target_os == "emscripten" {
			"wasm32-unknown-emscripten"
		} else {
			return Err(
				format!(
						"unsupported wasm target '{target}': use wasm32-unknown-unknown or wasm32-unknown-emscripten"
				)
				.into(),
			);
		}
	} else {
		target.as_str()
	};

	compile_c_library(&source_dir, c_target, emscripten_toolchain.as_ref());

	println!("cargo:rustc-env=NOVAS_UPSTREAM_VERSION={NOVAS_UPSTREAM_VERSION}");
	println!("cargo:rustc-env=NOVAS_SOURCE_PATH={}", source_dir.display());
	println!("cargo:rustc-env=NOVAS_CIO_RA_BIN_PATH={}", cio_ra_bin_path.display());

	Ok(())
}

fn target_cache_dir(manifest_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
	if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
		return Ok(PathBuf::from(target_dir).join("novas-cache"));
	}

	Ok(manifest_dir.join("target").join("novas-cache"))
}

fn apply_compatibility_patches(source_dir: &Path) -> Result<(), Box<dyn Error>> {
	// The patch hook is intentionally explicit: compatibility edits can be
	// applied here when upstream C requires target-specific adjustments.
	let _ = source_dir;
	Ok(())
}

fn generate_cio_ra_bin(source_dir: &Path, out_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
	let txt_path = source_dir.join("CIO_RA.TXT");
	let txt = fs::read_to_string(&txt_path)?;

	let mut rows = Vec::new();
	for (line_number, line) in txt.lines().enumerate() {
		if line_number == 0 {
			continue;
		}

		let trimmed = line.trim();
		if trimmed.is_empty() {
			continue;
		}

		let mut parts = trimmed.split_whitespace();
		let Some(jd_str) = parts.next() else {
			continue;
		};
		let Some(ra_str) = parts.next() else {
			return Err(format!("missing CIO_RA.TXT RA value on line {}", line_number + 1).into());
		};

		let jd: f64 = jd_str
			.parse()
			.map_err(|_| format!("invalid CIO_RA.TXT Julian date on line {}", line_number + 1))?;
		let ra: f64 = ra_str
			.parse()
			.map_err(|_| format!("invalid CIO_RA.TXT RA value on line {}", line_number + 1))?;
		rows.push((jd, ra));
	}

	if rows.is_empty() {
		return Err("CIO_RA.TXT did not contain any data rows".into());
	}

	let jd_first = rows[0].0;
	let jd_last = rows[rows.len() - 1].0;
	let interval = if rows.len() > 1 { rows[1].0 - rows[0].0 } else { 0.0 };
	let n_recs = i32::try_from(rows.len())
		.map_err(|_| "CIO_RA.TXT row count does not fit into 32-bit long")?;

	let mut bytes = Vec::with_capacity(3 * std::mem::size_of::<f64>() + std::mem::size_of::<i32>() + rows.len() * 2 * std::mem::size_of::<f64>());
	bytes.extend_from_slice(&jd_first.to_le_bytes());
	bytes.extend_from_slice(&jd_last.to_le_bytes());
	bytes.extend_from_slice(&interval.to_le_bytes());
	bytes.extend_from_slice(&n_recs.to_le_bytes());

	for (jd, ra) in rows {
		bytes.extend_from_slice(&jd.to_le_bytes());
		bytes.extend_from_slice(&ra.to_le_bytes());
	}

	let out_path = out_dir.join("cio_ra.bin");
	fs::write(&out_path, bytes)?;
	Ok(out_path)
}

fn generate_bindings(source_dir: &Path, out_dir: &Path, host_target: &str) -> Result<(), Box<dyn Error>> {
	let header = source_dir.join("novas.h");
	let include_arg = format!("-I{}", source_dir.display());
	let target_arg = format!("--target={host_target}");

	let bindings = bindgen::Builder::default()
		.header(header.to_string_lossy())
		.clang_arg(include_arg)
		.clang_arg(target_arg)
		.clang_arg("-fparse-all-comments")
		.allowlist_file(".*novas\\.h")
		.allowlist_file(".*novascon\\.h")
		.allowlist_file(".*solarsystem\\.h")
		.allowlist_file(".*nutation\\.h")
		.allowlist_file(".*eph_manager\\.h")
		.generate_comments(true)
		.layout_tests(false)
		.parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
		.generate()
		.map_err(|_| "bindgen failed to generate NOVAS bindings")?;

	bindings.write_to_file(out_dir.join("bindings.rs"))?;
	Ok(())
}

#[derive(Debug)]
struct FunctionSig {
	name: String,
	params: Vec<(String, String)>,
	ret: Option<String>,
}

fn generate_convenience_api(source_dir: &Path, out_dir: &Path) -> Result<(), Box<dyn Error>> {
	let bindings_path = out_dir.join("bindings.rs");
	let bindings_source = fs::read_to_string(&bindings_path)?;
	let signatures = parse_function_signatures(&bindings_source);
	let docs = extract_function_docs(source_dir)?;

	let mut generated = String::new();
	generated.push_str("// Auto-generated convenience API.\n");
	generated.push_str("// This file is generated by build.rs.\n\n");

	let mut count = 0usize;
	for sig in &signatures {
		if let Some(wrapper) = generate_scalar_io_wrapper(sig) {
			generated.push_str(&with_doc_comment(&sig.name, docs.get(&sig.name), &wrapper));
			generated.push('\n');
			count += 1;
		} else if let Some(wrapper) = generate_non_scalar_ref_wrapper(sig) {
			generated.push_str(&with_doc_comment(&sig.name, docs.get(&sig.name), &wrapper));
			generated.push('\n');
			count += 1;
		} else {
			generated.push_str(&with_doc_comment(
				&sig.name,
				docs.get(&sig.name),
				&generate_passthrough_wrapper(sig),
			));
			generated.push('\n');
			count += 1;
		}
	}

	if count == 0 {
		generated.push_str("// No convenience wrappers were generated for this build.\n");
	}

	fs::write(out_dir.join("convenience.rs"), generated)?;
	Ok(())
}

fn with_doc_comment(function_name: &str, doc: Option<&String>, code: &str) -> String {
	let Some(doc) = doc else {
		return code.to_string();
	};

	let mut out = String::new();
	for line in doc.lines() {
		if line.trim().is_empty() {
			out.push_str("///\n");
		} else {
			out.push_str("/// ");
			out.push_str(line.trim());
			out.push('\n');
		}
	}
	out.push_str("///\n");
	out.push_str(&format!("/// See [`sys::{function_name}`].\n"));
	out.push_str(code);
	out
}

fn extract_function_docs(source_dir: &Path) -> Result<HashMap<String, String>, Box<dyn Error>> {
	let files = ["novas.c", "novascon.c", "nutation.c", "solsys3.c", "readeph0.c"];
	let mut docs = HashMap::new();

	for file in files {
		let path = source_dir.join(file);
		if !path.exists() {
			continue;
		}

		let content = fs::read_to_string(path)?;
		let lines: Vec<&str> = content.lines().collect();
		let mut i = 0usize;

		while i < lines.len() {
			let marker_line = lines[i].trim();
			if !marker_line.starts_with("/********") || !marker_line.ends_with("*/") {
				i += 1;
				continue;
			}

			let marker = marker_line
				.trim_start_matches('/')
				.trim_end_matches("*/")
				.trim();
			let function_name = marker.trim_start_matches('*').trim().to_string();
			if function_name.is_empty() {
				i += 1;
				continue;
			}

			let mut j = i + 1;
			while j < lines.len() && !lines[j].trim_start().starts_with("/*") {
				j += 1;
			}
			if j >= lines.len() {
				i += 1;
				continue;
			}

			let mut comment_lines = Vec::new();
			while j < lines.len() {
				let c = lines[j].trim_end();
				comment_lines.push(c);
				if c.trim_end().ends_with("*/") {
					break;
				}
				j += 1;
			}

			if let Some(purpose) = extract_purpose_from_comment(&comment_lines) {
				docs.entry(function_name).or_insert(purpose);
			}

			i = j + 1;
		}
	}

	Ok(docs)
}

fn extract_purpose_from_comment(comment_lines: &[&str]) -> Option<String> {
	let mut in_purpose = false;
	let mut collected = Vec::new();

	for raw in comment_lines {
		let line = raw.trim().trim_start_matches('*').trim();
		if !in_purpose {
			if line == "PURPOSE:" {
				in_purpose = true;
			}
			continue;
		}

		if line.is_empty() {
			if !collected.is_empty() {
				break;
			}
			continue;
		}

		if line.ends_with(':') {
			break;
		}

		let clean = line.trim_start_matches('-').trim();
		if !clean.is_empty() {
			collected.push(clean.to_string());
		}
	}

	if collected.is_empty() {
		return None;
	}

	Some(collected.join(" "))
}

fn generate_root_reexports(out_dir: &Path) -> Result<(), Box<dyn Error>> {
	let bindings_path = out_dir.join("bindings.rs");
	let source = fs::read_to_string(&bindings_path)?;

	let mut names = BTreeSet::new();
	for line in source.lines() {
		let trimmed = line.trim();

		if let Some(rest) = trimmed.strip_prefix("pub type ") {
			if let Some(name) = rest.split_whitespace().next() {
				names.insert(name.trim_end_matches(';').to_string());
			}
			continue;
		}

		if let Some(rest) = trimmed.strip_prefix("pub struct ") {
			if let Some(name) = rest.split_whitespace().next() {
				names.insert(name.trim_end_matches('{').to_string());
			}
			continue;
		}

		if let Some(rest) = trimmed.strip_prefix("pub union ") {
			if let Some(name) = rest.split_whitespace().next() {
				names.insert(name.trim_end_matches('{').to_string());
			}
			continue;
		}

		if let Some(rest) = trimmed.strip_prefix("pub const ") {
			if let Some(name) = rest.split(':').next() {
				names.insert(name.trim().to_string());
			}
		}
	}

	let mut out = String::new();
	out.push_str("// Auto-generated root re-exports (non-function bindings).\n");
	out.push_str("// This file is generated by build.rs.\n\n");

	for name in names {
		out.push_str(&format!("pub use sys::{name};\n"));
	}

	fs::write(out_dir.join("root_reexports.rs"), out)?;
	Ok(())
}

fn parse_function_signatures(source: &str) -> Vec<FunctionSig> {
	let mut signatures = Vec::new();
	let mut pending: Option<String> = None;

	for line in source.lines() {
		let trimmed = line.trim();

		if let Some(current) = pending.as_mut() {
			current.push(' ');
			current.push_str(trimmed);
			if trimmed.ends_with(';') {
				if let Some(sig) = parse_one_function_signature(current) {
					signatures.push(sig);
				}
				pending = None;
			}
			continue;
		}

		if !trimmed.starts_with("pub fn ") {
			continue;
		}

		if trimmed.ends_with(';') {
			if let Some(sig) = parse_one_function_signature(trimmed) {
				signatures.push(sig);
			}
		} else {
			pending = Some(trimmed.to_string());
		}
	}

	signatures
}

fn parse_one_function_signature(raw: &str) -> Option<FunctionSig> {
	let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");

	let after_fn = normalized.strip_prefix("pub fn ")?;
	let open_paren = after_fn.find('(')?;

	let name = after_fn[..open_paren].trim().to_string();
	let close_paren = after_fn.rfind(')')?;

	let params_block = &after_fn[open_paren + 1..close_paren];
	let tail = after_fn[close_paren + 1..].trim();

	let ret = if let Some(arrow) = tail.find("->") {
		let ret_part = tail[arrow + 2..].trim().trim_end_matches(';').trim();
		if ret_part.is_empty() {
			None
		} else {
			Some(ret_part.to_string())
		}
	} else {
		None
	};

	let mut params = Vec::new();
	for raw_param in params_block.split(',') {
		let param = raw_param.trim();
		if param.is_empty() {
			continue;
		}
		let (param_name, param_ty) = param.split_once(':')?;
		params.push((param_name.trim().to_string(), param_ty.trim().to_string()));
	}

	Some(FunctionSig { name, params, ret })
}

fn is_pointer(ty: &str) -> bool {
	ty.starts_with("*mut ") || ty.starts_with("*const ")
}

fn pointee_type(ty: &str) -> Option<&str> {
	if let Some(rest) = ty.strip_prefix("*mut ") {
		return Some(rest.trim());
	}
	if let Some(rest) = ty.strip_prefix("*const ") {
		return Some(rest.trim());
	}
	None
}

fn is_primitive_scalar(ty: &str) -> bool {
	matches!(
		ty,
		"f64"
			| "f32"
			| "i8"
			| "u8"
			| "i16"
			| "u16"
			| "i32"
			| "u32"
			| "i64"
			| "u64"
			| "isize"
			| "usize"
			| "::std::os::raw::c_char"
			| "::std::os::raw::c_uchar"
			| "::std::os::raw::c_short"
			| "::std::os::raw::c_ushort"
			| "::std::os::raw::c_int"
			| "::std::os::raw::c_uint"
			| "::std::os::raw::c_long"
			| "::std::os::raw::c_ulong"
			| "::std::os::raw::c_float"
			| "::std::os::raw::c_double"
	)
}

fn is_integer_scalar(ty: &str) -> bool {
	matches!(
		ty,
		"i8"
			| "u8"
			| "i16"
			| "u16"
			| "i32"
			| "u32"
			| "i64"
			| "u64"
			| "isize"
			| "usize"
			| "::std::os::raw::c_char"
			| "::std::os::raw::c_uchar"
			| "::std::os::raw::c_short"
			| "::std::os::raw::c_ushort"
			| "::std::os::raw::c_int"
			| "::std::os::raw::c_uint"
			| "::std::os::raw::c_long"
			| "::std::os::raw::c_ulong"
	)
}

fn name_looks_like_count(name: &str) -> bool {
	let lower = name.to_ascii_lowercase();
	lower == "n"
		|| lower.starts_with("n_")
		|| lower.starts_with("num")
		|| lower.starts_with("count")
		|| lower.starts_with("len")
		|| lower.ends_with("_count")
		|| lower.ends_with("_len")
		|| lower.ends_with("_n")
}

fn should_convert_non_scalar_ptr(sig: &FunctionSig, index: usize) -> bool {
	let (_, ty) = &sig.params[index];
	let Some(pointee) = pointee_type(ty) else {
		return false;
	};

	if is_primitive_scalar(pointee) || pointee == "::std::os::raw::c_void" {
		return false;
	}

	if index > 0 {
		let (prev_name, prev_ty) = &sig.params[index - 1];
		if is_integer_scalar(prev_ty) && name_looks_like_count(prev_name) {
			return false;
		}
	}

	true
}

fn generate_scalar_io_wrapper(sig: &FunctionSig) -> Option<String> {
	let first_ptr_index = sig.params.iter().position(|(_, ty)| ty.starts_with("*mut "))?;

	if sig.params[..first_ptr_index].iter().any(|(_, ty)| is_pointer(ty)) {
		return None;
	}

	if sig.params[first_ptr_index..]
		.iter()
		.any(|(_, ty)| !ty.starts_with("*mut "))
	{
		return None;
	}

	let out_params: Vec<(&str, &str)> = sig.params[first_ptr_index..]
		.iter()
		.filter_map(|(name, ty)| pointee_type(ty).map(|pointee| (name.as_str(), pointee)))
		.collect();

	if out_params.is_empty() || out_params.iter().any(|(_, ty)| !is_primitive_scalar(ty)) {
		return None;
	}

	let in_params = &sig.params[..first_ptr_index];
	let wrapper_name = sig.name.clone();

	let in_param_sig = in_params
		.iter()
		.map(|(name, ty)| format!("{name}: {ty}"))
		.collect::<Vec<_>>()
		.join(", ");

	let io_param_sig = out_params
		.iter()
		.map(|(name, ty)| format!("{name}: &mut {ty}"))
		.collect::<Vec<_>>()
		.join(", ");

	let wrapper_param_sig = if in_param_sig.is_empty() {
		io_param_sig.clone()
	} else if io_param_sig.is_empty() {
		in_param_sig.clone()
	} else {
		format!("{in_param_sig}, {io_param_sig}")
	};

	let return_ty = sig.ret.clone().unwrap_or_else(|| "()".to_string());

	let mut body = String::new();
	body.push_str("#[inline]\n");
	body.push_str(&format!("pub unsafe fn {wrapper_name}({wrapper_param_sig}) -> {return_ty} {{\n"));

	let mut call_args = in_params
		.iter()
		.map(|(name, _)| name.clone())
		.collect::<Vec<_>>();
	for (name, ty) in &out_params {
		call_args.push(format!("{name} as *mut {ty}"));
	}

	let call_expr = format!("sys::{}({})", sig.name, call_args.join(", "));
	if return_ty == "()" {
		body.push_str(&format!("    {call_expr};\n"));
	} else {
		body.push_str(&format!("    {call_expr}\n"));
	}

	body.push_str("}\n");
	Some(body)
}

fn generate_non_scalar_ref_wrapper(sig: &FunctionSig) -> Option<String> {
	let mut converted = false;

	let param_sig = sig
		.params
		.iter()
		.enumerate()
		.map(|(idx, (name, ty))| {
			if should_convert_non_scalar_ptr(sig, idx) {
				if let Some(pointee) = pointee_type(ty) {
					converted = true;
					if ty.starts_with("*const ") {
						return format!("{name}: &{pointee}");
					}
					if ty.starts_with("*mut ") {
						return format!("{name}: &mut {pointee}");
					}
				}
			}
			format!("{name}: {ty}")
		})
		.collect::<Vec<_>>()
		.join(", ");

	if !converted {
		return None;
	}

	let call_args = sig
		.params
		.iter()
		.enumerate()
		.map(|(idx, (name, ty))| {
			if should_convert_non_scalar_ptr(sig, idx) {
				if let Some(pointee) = pointee_type(ty) {
					if ty.starts_with("*const ") {
						return format!("{name} as *const {pointee}");
					}
					if ty.starts_with("*mut ") {
						return format!("{name} as *mut {pointee}");
					}
				}
			}
			name.clone()
		})
		.collect::<Vec<_>>()
		.join(", ");

	let ret_ty = sig.ret.clone().unwrap_or_else(|| "()".to_string());
	let mut body = String::new();
	body.push_str("#[inline]\n");
	body.push_str(&format!("pub unsafe fn {}({}) -> {} {{\n", sig.name, param_sig, ret_ty));
	if ret_ty == "()" {
		body.push_str(&format!("    sys::{}({call_args});\n", sig.name));
	} else {
		body.push_str(&format!("    sys::{}({call_args})\n", sig.name));
	}
	body.push_str("}\n");

	Some(body)
}

fn generate_passthrough_wrapper(sig: &FunctionSig) -> String {
	let param_sig = sig
		.params
		.iter()
		.map(|(name, ty)| format!("{name}: {ty}"))
		.collect::<Vec<_>>()
		.join(", ");

	let arg_list = sig
		.params
		.iter()
		.map(|(name, _)| name.clone())
		.collect::<Vec<_>>()
		.join(", ");

	let ret_ty = sig.ret.clone().unwrap_or_else(|| "()".to_string());
	let mut body = String::new();
	body.push_str("#[inline]\n");
	if ret_ty == "()" {
		body.push_str(&format!("pub unsafe fn {}({}) {{\n", sig.name, param_sig));
		body.push_str(&format!("    sys::{}({});\n", sig.name, arg_list));
	} else {
		body.push_str(&format!("pub unsafe fn {}({}) -> {} {{\n", sig.name, param_sig, ret_ty));
		body.push_str(&format!("    sys::{}({})\n", sig.name, arg_list));
	}
	body.push_str("}\n");
	body
}

fn compile_c_library(source_dir: &Path, target: &str, emscripten: Option<&EmscriptenToolchain>) {
	let c_files = ["novas.c", "novascon.c", "nutation.c", "solsys3.c", "readeph0.c"];

	let mut build = cc::Build::new();
	if target == "wasm32-unknown-emscripten" {
		if let Some(toolchain) = emscripten {
			build.compiler(&toolchain.emcc);
			if let Some(emar) = &toolchain.emar {
				build.archiver(emar);
			}
		} else {
			build.compiler("emcc");
		}
	}
	build.target(target);
	build.include(source_dir);
	build.warnings(false);
	build.flag_if_supported("-std=c99");

	for file in c_files {
		build.file(source_dir.join(file));
	}

	build.compile("novas_c31");
}

fn setup_wasm_unknown_linking(emcc: &Path) -> Result<(), Box<dyn Error>> {
	// For wasm32-unknown-unknown we deliberately avoid linking Emscripten runtime
	// archives here. Pulling libc/compiler_rt archives in this mode introduces
	// `env`/WASI host imports that are incompatible with wasm-bindgen test runner.
	let _ = emcc;

	Ok(())
}

fn resolve_emscripten_toolchain(cache_dir: &Path) -> Result<EmscriptenToolchain, Box<dyn Error>> {
	if let Some(emcc) = find_tool_in_path("emcc") {
		let emar = find_tool_in_path("emar");
		return Ok(EmscriptenToolchain {
			emsdk_root: None,
			emcc,
			emar,
		});
	}

	let emsdk_root = ensure_emsdk_root(cache_dir)?;
	install_and_activate_emsdk(&emsdk_root)?;

	let emcc = emsdk_root.join("upstream").join("emscripten").join("emcc");
	if !emcc.exists() {
		return Err(format!("emsdk bootstrap did not produce emcc at '{}'", emcc.display()).into());
	}

	let emar_candidate = emsdk_root.join("upstream").join("emscripten").join("emar");
	let emar = if emar_candidate.exists() {
		Some(emar_candidate)
	} else {
		None
	};

	Ok(EmscriptenToolchain {
		emsdk_root: Some(emsdk_root),
		emcc,
		emar,
	})
}

fn ensure_emsdk_root(cache_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
	let version = emsdk_version();
	let emsdk_root = cache_dir.join(format!("emsdk-{version}"));
	if emsdk_root.exists() {
		return Ok(emsdk_root);
	}

	let output = Command::new("git")
		.arg("clone")
		.arg("--depth")
		.arg("1")
		.arg("--branch")
		.arg(&version)
		.arg("https://github.com/emscripten-core/emsdk.git")
		.arg(&emsdk_root)
		.output()?;

	if !output.status.success() {
		return Err(format!(
			"failed to clone emsdk {version} into '{}':\nstdout:\n{}\nstderr:\n{}",
			emsdk_root.display(),
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}

	Ok(emsdk_root)
}

fn install_and_activate_emsdk(emsdk_root: &Path) -> Result<(), Box<dyn Error>> {
	let version = emsdk_version();
	run_emsdk_python(emsdk_root, &["install", &version])?;
	run_emsdk_python(emsdk_root, &["activate", "--embedded", &version])?;
	Ok(())
}

fn emsdk_version() -> String {
	env::var("NOVAS_EMSDK_VERSION").unwrap_or_else(|_| EMSDK_DEFAULT_VERSION.to_string())
}

fn run_emsdk_python(emsdk_root: &Path, args: &[&str]) -> Result<(), Box<dyn Error>> {
	let output = Command::new("python3")
		.arg("emsdk.py")
		.args(args)
		.current_dir(emsdk_root)
		.output()?;

	if !output.status.success() {
		return Err(format!(
			"emsdk command failed: python3 emsdk.py {}\nstdout:\n{}\nstderr:\n{}",
			args.join(" "),
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		)
		.into());
	}

	Ok(())
}

fn setup_emscripten_environment(toolchain: &EmscriptenToolchain) {
	if let Some(emsdk_root) = &toolchain.emsdk_root {
		let em_config = emsdk_root.join(".emscripten");
		if em_config.exists() {
			env::set_var("EM_CONFIG", &em_config);
		}
		env::set_var("EMSDK", emsdk_root);
		env::set_var("EM_CACHE", emsdk_root.join("upstream").join("emscripten").join("cache"));
	}
}

fn find_tool_in_path(name: &str) -> Option<PathBuf> {
	let paths = env::var_os("PATH")?;
	for base in env::split_paths(&paths) {
		let candidate = base.join(name);
		if candidate.is_file() {
			return Some(candidate);
		}
	}
	None
}
