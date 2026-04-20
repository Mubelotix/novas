use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const NOVAS_URL: &str = "https://ascl.net/assets/codes/NOVAS/novasc3.1.zip";
const NOVAS_ARCHIVE_NAME: &str = "novasc3.1.zip";
const NOVAS_EXTRACTED_DIR: &str = "novasc3.1";
const NOVAS_UPSTREAM_VERSION: &str = "3.1";

fn main() -> Result<(), Box<dyn Error>> {
	println!("cargo:rerun-if-changed=build.rs");

	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
	let out_dir = PathBuf::from(env::var("OUT_DIR")?);
	let host_target = env::var("HOST")?;
	let target_arch = env::var("CARGO_CFG_TARGET_ARCH")?;

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
	generate_bindings(&source_dir, &out_dir, &host_target)?;

	if target_arch != "wasm32" {
		compile_native_c(&source_dir);
	}

	println!("cargo:rustc-env=NOVAS_UPSTREAM_VERSION={NOVAS_UPSTREAM_VERSION}");

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

fn compile_native_c(source_dir: &Path) {
	let c_files = ["novas.c", "novascon.c", "nutation.c", "solsys3.c", "readeph0.c"];

	let mut build = cc::Build::new();
	build.include(source_dir);
	build.warnings(false);
	build.flag_if_supported("-std=c99");

	for file in c_files {
		build.file(source_dir.join(file));
	}

	build.compile("novas_c31");
}
