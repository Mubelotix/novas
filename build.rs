use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const NOVAS_URL: &str = "https://ascl.net/assets/codes/NOVAS/novasc3.1.zip";
const NOVAS_ARCHIVE_NAME: &str = "novasc3.1.zip";
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

	let archive_path = cache_dir.join(NOVAS_ARCHIVE_NAME);
	if !archive_path.exists() {
		download_archive(&archive_path)?;
	}

	let extraction_root = out_dir.join("novas-upstream");
	if extraction_root.exists() {
		fs::remove_dir_all(&extraction_root)?;
	}
	fs::create_dir_all(&extraction_root)?;
	extract_archive(&archive_path, &extraction_root)?;

	let source_dir = extraction_root.join(NOVAS_EXTRACTED_DIR);
	if !source_dir.exists() {
		return Err(format!(
			"expected source directory '{}' after extraction",
			source_dir.display()
		)
		.into());
	}

	apply_compatibility_patches(&source_dir)?;
	let cio_ra_bin_path = generate_cio_ra_bin(&source_dir, &out_dir)?;
	generate_bindings(&source_dir, &out_dir, &host_target)?;
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
	println!("cargo:rustc-env=NOVAS_ARCHIVE_PATH={}", archive_path.display());
	println!("cargo:rustc-env=NOVAS_CIO_RA_BIN_PATH={}", cio_ra_bin_path.display());

	Ok(())
}

fn target_cache_dir(manifest_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
	if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
		return Ok(PathBuf::from(target_dir).join("novas-cache"));
	}

	Ok(manifest_dir.join("target").join("novas-cache"))
}

fn download_archive(destination: &Path) -> Result<(), Box<dyn Error>> {
	let response = reqwest::blocking::get(NOVAS_URL)?;
	let response = response.error_for_status()?;
	let content = response.bytes()?;
	fs::write(destination, content.as_ref())?;
	Ok(())
}

fn download_file(url: &str, destination: &Path) -> Result<(), Box<dyn Error>> {
	let response = reqwest::blocking::get(url)?;
	let response = response.error_for_status()?;
	let content = response.bytes()?;
	fs::write(destination, content.as_ref())?;
	Ok(())
}

fn extract_archive(archive_path: &Path, extraction_root: &Path) -> Result<(), Box<dyn Error>> {
	let archive_file = fs::File::open(archive_path)?;
	let mut archive = zip::ZipArchive::new(archive_file)?;

	for i in 0..archive.len() {
		let mut entry = archive.by_index(i)?;
		let Some(rel_path) = entry.enclosed_name().map(|p| p.to_owned()) else {
			continue;
		};

		let out_path = extraction_root.join(rel_path);

		if entry.is_dir() {
			fs::create_dir_all(&out_path)?;
			continue;
		}

		if let Some(parent) = out_path.parent() {
			fs::create_dir_all(parent)?;
		}

		let mut out_file = fs::File::create(&out_path)?;
		io::copy(&mut entry, &mut out_file)?;
	}

	Ok(())
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
		.allowlist_file(".*novas\\.h")
		.allowlist_file(".*novascon\\.h")
		.allowlist_file(".*solarsystem\\.h")
		.allowlist_file(".*nutation\\.h")
		.allowlist_file(".*eph_manager\\.h")
		.layout_tests(false)
		.parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
		.generate()
		.map_err(|_| "bindgen failed to generate NOVAS bindings")?;

	bindings.write_to_file(out_dir.join("bindings.rs"))?;
	Ok(())
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

	let emsdk_zip = cache_dir.join(emsdk_zip_name(&version));
	if !emsdk_zip.exists() {
		download_file(&emsdk_zip_url(&version), &emsdk_zip)?;
	}

	let staging = cache_dir.join("emsdk-extract-staging");
	if staging.exists() {
		fs::remove_dir_all(&staging)?;
	}
	fs::create_dir_all(&staging)?;
	extract_archive(&emsdk_zip, &staging)?;

	let extracted = staging.join(emsdk_extracted_dir(&version));
	if !extracted.exists() {
		return Err(format!("emsdk archive did not contain '{}'", emsdk_extracted_dir(&version)).into());
	}

	fs::rename(&extracted, &emsdk_root)?;
	fs::remove_dir_all(&staging)?;

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

fn emsdk_zip_url(version: &str) -> String {
	format!("https://github.com/emscripten-core/emsdk/archive/refs/tags/{version}.zip")
}

fn emsdk_zip_name(version: &str) -> String {
	format!("emsdk-{version}.zip")
}

fn emsdk_extracted_dir(version: &str) -> String {
	format!("emsdk-{version}")
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
