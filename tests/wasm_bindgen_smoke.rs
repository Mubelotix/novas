#![cfg(target_arch = "wasm32")]

#[path = "common/parity_expected.rs"]
mod parity_expected;

use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn novas_ffi_matches_expected_baseline() {
    let expected = parity_expected::expected_map();

    let era = unsafe { novas::era(2451545.0, 0.0) };
    parity_expected::assert_close("era", era, expected["era"], 1e-13);

    let ee_ct = unsafe { novas::ee_ct(2451545.0, 0.0, 0) };
    parity_expected::assert_close("ee_ct", ee_ct, expected["ee_ct"], 1e-13);

    let mut mobl = 0.0;
    let mut tobl = 0.0;
    let mut ee = 0.0;
    let mut dpsi = 0.0;
    let mut deps = 0.0;

    unsafe { novas::e_tilt(2451545.0, 0, &mut mobl, &mut tobl, &mut ee, &mut dpsi, &mut deps) };

    parity_expected::assert_close("mobl", mobl, expected["mobl"], 1e-13);
    parity_expected::assert_close("tobl", tobl, expected["tobl"], 1e-13);
    parity_expected::assert_close("ee", ee, expected["ee"], 1e-13);
    parity_expected::assert_close("dpsi", dpsi, expected["dpsi"], 1e-13);
    parity_expected::assert_close("deps", deps, expected["deps"], 1e-13);

    let mut gst = 0.0;
    let status = unsafe { novas::sidereal_time(2451545.0, 0.0, 69.184, 0, 0, 0, &mut gst) };
    assert_eq!(status, expected["sidereal_status"] as i16);
    parity_expected::assert_close("sidereal_gst", gst, expected["sidereal_gst"], 1e-13);
}
