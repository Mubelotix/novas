#![cfg(not(target_arch = "wasm32"))]

use core::ffi::{c_char, c_short};

#[test]
fn transform_cat_uses_reference_params() {
    let _f: unsafe fn(
        option: c_short,
        date_incat: f64,
        incat: &mut novas::cat_entry,
        date_newcat: f64,
        newcat_id: *mut c_char,
        newcat: &mut novas::cat_entry,
    ) -> c_short = novas::transform_cat;
}
