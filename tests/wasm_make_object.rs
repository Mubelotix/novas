#![cfg(target_arch = "wasm32")]

use core::ffi::c_char;
use std::mem;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn make_object_uppercases_name_and_links_cleanly() {
    // Trigger NOVAS path that uses strcpy + toupper in C.
    let mut star: novas::cat_entry = unsafe { mem::zeroed() };
    let mut obj: novas::object = unsafe { mem::zeroed() };
    let mut name = *b"alpha\0";

    let status = unsafe {
        novas::make_object(
            2,
            0,
            name.as_mut_ptr() as *mut c_char,
            &mut star,
            &mut obj,
        )
    };

    assert_eq!(status, 0);
    assert_eq!(obj.name[0], b'A' as c_char);
    assert_eq!(obj.name[1], b'L' as c_char);
    assert_eq!(obj.name[2], b'P' as c_char);
    assert_eq!(obj.name[3], b'H' as c_char);
    assert_eq!(obj.name[4], b'A' as c_char);
    assert_eq!(obj.name[5], 0);
}
