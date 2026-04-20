#![doc = include_str!("../README.md")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(clippy::all)]

pub mod sys {
	#![allow(non_camel_case_types)]
	#![allow(non_snake_case)]
	#![allow(non_upper_case_globals)]
	#![allow(clippy::all)]

	include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

include!(concat!(env!("OUT_DIR"), "/root_reexports.rs"));
include!(concat!(env!("OUT_DIR"), "/convenience.rs"));

/// Upstream NOVAS C version used by this crate.
pub const NOVAS_UPSTREAM_VERSION: &str = env!("NOVAS_UPSTREAM_VERSION");

/// Absolute path to the cached upstream NOVAS archive used for this build.
pub const NOVAS_ARCHIVE_PATH: &str = env!("NOVAS_ARCHIVE_PATH");

/// Registers a virtual file for the wasm32-unknown-unknown runtime.
///
/// When NOVAS C code calls `fopen` on this target, lookups are resolved from
/// the registered virtual file table. On non-wasm targets this function is a
/// no-op because NOVAS uses the host file system directly.
pub fn register_virtual_file(name: &str, bytes: &[u8]) {
	#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
	wasm_c_runtime_shims::register_virtual_file(name, bytes);

	#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
	let _ = (name, bytes);
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod wasm_c_runtime_shims {
	use core::ffi::{c_char, c_int, c_long, c_void};
	use std::borrow::Cow;
	use std::collections::HashMap;
	use std::ffi::CStr;
	use std::ptr;
	use std::sync::{Mutex, OnceLock};

	struct VirtualFile {
		data: Vec<u8>,
		position: usize,
	}

	fn allocations() -> &'static Mutex<HashMap<usize, Vec<u8>>> {
		static ALLOCATIONS: OnceLock<Mutex<HashMap<usize, Vec<u8>>>> = OnceLock::new();
		ALLOCATIONS.get_or_init(|| Mutex::new(HashMap::new()))
	}

	fn virtual_files() -> &'static Mutex<HashMap<String, Vec<u8>>> {
		static FILES: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
		FILES.get_or_init(|| {
			let files = {
				#[cfg(feature = "embedded-cio-ra")]
				{
					let mut files = HashMap::new();
					let cio_bin = include_bytes!(env!("NOVAS_CIO_RA_BIN_PATH")).to_vec();
					files.insert("cio_ra.bin".to_string(), cio_bin.clone());
					files.insert("./cio_ra.bin".to_string(), cio_bin);
					files
				}

				#[cfg(not(feature = "embedded-cio-ra"))]
				{
					HashMap::new()
				}
			};
			Mutex::new(files)
		})
	}

	pub(super) fn register_virtual_file(name: &str, bytes: &[u8]) {
		let key = normalize_path(name).into_owned();
		virtual_files()
			.lock()
			.expect("virtual file registry poisoned")
			.insert(key, bytes.to_vec());
	}

	fn normalize_path(path: &str) -> Cow<'_, str> {
		if path.contains('\\') {
			Cow::Owned(path.replace('\\', "/"))
		} else {
			Cow::Borrowed(path)
		}
	}

	fn lookup_virtual_file(path: &str) -> Option<Vec<u8>> {
		let normalized = normalize_path(path);
		let filename = normalized.rsplit('/').next().unwrap_or(normalized.as_ref());

		let files = virtual_files()
			.lock()
			.expect("virtual file registry poisoned");

		files
			.get(normalized.as_ref())
			.or_else(|| files.get(filename))
			.cloned()
	}

	fn c_string(ptr: *const c_char) -> Option<String> {
		if ptr.is_null() {
			return None;
		}

		// SAFETY: The NOVAS C calls are expected to pass valid null-terminated strings.
		let string = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
		Some(string)
	}

	#[no_mangle]
	pub extern "C" fn malloc(size: usize) -> *mut c_void {
		if size == 0 {
			return ptr::null_mut();
		}

		let mut bytes = vec![0u8; size];
		let ptr = bytes.as_mut_ptr();
		allocations()
			.lock()
			.expect("malloc allocation registry poisoned")
			.insert(ptr as usize, bytes);
		ptr as *mut c_void
	}

	#[no_mangle]
	pub extern "C" fn calloc(nmemb: usize, size: usize) -> *mut c_void {
		let Some(total) = nmemb.checked_mul(size) else {
			return ptr::null_mut();
		};
		malloc(total)
	}

	#[no_mangle]
	pub extern "C" fn free(ptr: *mut c_void) {
		if ptr.is_null() {
			return;
		}

		let _ = allocations()
			.lock()
			.expect("free allocation registry poisoned")
			.remove(&(ptr as usize));
	}

	#[no_mangle]
	pub extern "C" fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void {
		let Some(path) = c_string(path) else {
			return ptr::null_mut();
		};
		let Some(mode) = c_string(mode) else {
			return ptr::null_mut();
		};

		if !mode.contains('r') {
			return ptr::null_mut();
		}

		let Some(data) = lookup_virtual_file(&path) else {
			return ptr::null_mut();
		};

		let file = Box::new(VirtualFile { data, position: 0 });
		Box::into_raw(file) as *mut c_void
	}

	#[no_mangle]
	pub extern "C" fn fclose(stream: *mut c_void) -> c_int {
		if stream.is_null() {
			return -1;
		}

		// SAFETY: `stream` was produced by `Box::into_raw` in `fopen`.
		unsafe {
			drop(Box::from_raw(stream as *mut VirtualFile));
		}

		0
	}

	#[no_mangle]
	pub extern "C" fn fread(
		ptr: *mut c_void,
		size: usize,
		nmemb: usize,
		stream: *mut c_void,
	) -> usize {
		if ptr.is_null() || stream.is_null() || size == 0 || nmemb == 0 {
			return 0;
		}

		let Some(total_bytes) = size.checked_mul(nmemb) else {
			return 0;
		};

		// SAFETY: `stream` is expected to be a valid pointer returned by `fopen`.
		let file = unsafe { &mut *(stream as *mut VirtualFile) };

		let available = file.data.len().saturating_sub(file.position);
		let to_read = total_bytes.min(available);

		if to_read == 0 {
			return 0;
		}

		// SAFETY: destination pointer is provided by C caller and `to_read` bytes
		// are copied from in-bounds source slice.
		unsafe {
			ptr::copy_nonoverlapping(file.data.as_ptr().add(file.position), ptr as *mut u8, to_read);
		}

		file.position += to_read;
		to_read / size
	}

	#[no_mangle]
	pub extern "C" fn fseek(stream: *mut c_void, offset: c_long, whence: c_int) -> c_int {
		if stream.is_null() {
			return -1;
		}

		const SEEK_SET: c_int = 0;
		const SEEK_CUR: c_int = 1;
		const SEEK_END: c_int = 2;

		// SAFETY: `stream` is expected to be a valid pointer returned by `fopen`.
		let file = unsafe { &mut *(stream as *mut VirtualFile) };

		let base = match whence {
			SEEK_SET => 0_i128,
			SEEK_CUR => file.position as i128,
			SEEK_END => file.data.len() as i128,
			_ => return -1,
		};

		let target = base + offset as i128;
		if target < 0 || target as usize > file.data.len() {
			return -1;
		}

		file.position = target as usize;
		0
	}
}
