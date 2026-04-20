#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn novas_ffi_smoke_runs() {
    let era = unsafe { novas::era(2451545.0, 0.0) };
    assert!(era.is_finite());

    let mut mobl = 0.0;
    let mut tobl = 0.0;
    let mut ee = 0.0;
    let mut dpsi = 0.0;
    let mut deps = 0.0;

    unsafe { novas::e_tilt(2451545.0, 0, &mut mobl, &mut tobl, &mut ee, &mut dpsi, &mut deps) };

    assert!(mobl.is_finite());
    assert!(tobl.is_finite());
    assert!(ee.is_finite());
    assert!(dpsi.is_finite());
    assert!(deps.is_finite());
}
