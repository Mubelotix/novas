#![cfg(target_arch = "wasm32")]

use std::os::raw::c_long;
use wasm_bindgen_test::wasm_bindgen_test;

fn push_c_long_le(bytes: &mut Vec<u8>, value: c_long) {
	if std::mem::size_of::<c_long>() == 4 {
		bytes.extend_from_slice(&(value as i32).to_le_bytes());
	} else {
		bytes.extend_from_slice(&(value as i64).to_le_bytes());
	}
}

fn make_cio_ra_bin() -> Vec<u8> {
	// Header format used by NOVAS cio_array/cio_ra file reader:
	// jd_beg (f64), jd_end (f64), t_int (f64), n_recs (c_long), then
	// n_recs records of (jd_tdb: f64, ra_cio: f64).
	let jd_beg = 1000.0_f64;
	let jd_end = 1002.0_f64;
	let t_int = 1.0_f64;
	let n_recs: c_long = 3;
	let rows = [(1000.0_f64, 10.0_f64), (1001.0_f64, 20.0_f64), (1002.0_f64, 30.0_f64)];

	let mut bytes = Vec::new();
	bytes.extend_from_slice(&jd_beg.to_le_bytes());
	bytes.extend_from_slice(&jd_end.to_le_bytes());
	bytes.extend_from_slice(&t_int.to_le_bytes());
	push_c_long_le(&mut bytes, n_recs);

	for (jd, ra) in rows {
		bytes.extend_from_slice(&jd.to_le_bytes());
		bytes.extend_from_slice(&ra.to_le_bytes());
	}

	bytes
}

#[wasm_bindgen_test]
fn registered_virtual_cio_file_is_used() {
	let custom = make_cio_ra_bin();
	novas::register_virtual_file("cio_ra.bin", &custom);

	let mut points = [
		novas::ra_of_cio {
			jd_tdb: 0.0,
			ra_cio: 0.0,
		},
		novas::ra_of_cio {
			jd_tdb: 0.0,
			ra_cio: 0.0,
		},
	];

	let status = unsafe { novas::cio_array(1001.0, 2 as c_long, points.as_mut_ptr()) };
	assert_eq!(status, 0, "cio_array should open/read registered virtual file");

	assert!((points[0].jd_tdb - 1001.0).abs() < 1e-12);
	assert!((points[0].ra_cio - 20.0).abs() < 1e-12);
	assert!((points[1].jd_tdb - 1002.0).abs() < 1e-12);
	assert!((points[1].ra_cio - 30.0).abs() < 1e-12);
}