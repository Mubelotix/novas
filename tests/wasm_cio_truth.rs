#![cfg(all(target_arch = "wasm32", feature = "embedded-cio-ra"))]

use std::os::raw::c_long;
use wasm_bindgen_test::wasm_bindgen_test;

fn assert_close(label: &str, actual: f64, expected: f64, tol: f64) {
    let delta = (actual - expected).abs();
    assert!(
        delta <= tol,
        "{label}: expected {expected:.15}, got {actual:.15}, |delta|={delta:.3e}"
    );
}

#[wasm_bindgen_test]
fn embedded_cio_file_matches_upstream_truth_points() {
    // These values come directly from upstream novasc3.1/CIO_RA.TXT and verify
    // that wasm C stdio reads the embedded cio_ra.bin with exact layout/offsets.

    // Beginning of file: first two records.
    let mut begin_points = [
        novas::ra_of_cio {
            jd_tdb: 0.0,
            ra_cio: 0.0,
        },
        novas::ra_of_cio {
            jd_tdb: 0.0,
            ra_cio: 0.0,
        },
    ];
    let begin_status = unsafe { novas::cio_array(2341951.5, 2 as c_long, begin_points.as_mut_ptr()) };
    assert_eq!(begin_status, 0);
    assert_close("begin jd[0]", begin_points[0].jd_tdb, 2341951.4, 1e-12);
    assert_close("begin ra[0]", begin_points[0].ra_cio, -1.94832757875929, 1e-12);
    assert_close("begin jd[1]", begin_points[1].jd_tdb, 2341952.6, 1e-12);
    assert_close("begin ra[1]", begin_points[1].ra_cio, -1.94825199908877, 1e-12);

    // Middle of file around J2000: exact row + next row.
    let mut mid_points = [
        novas::ra_of_cio {
            jd_tdb: 0.0,
            ra_cio: 0.0,
        },
        novas::ra_of_cio {
            jd_tdb: 0.0,
            ra_cio: 0.0,
        },
    ];
    let mid_status = unsafe { novas::cio_array(2451545.1, 2 as c_long, mid_points.as_mut_ptr()) };
    assert_eq!(mid_status, 0);
    assert_close("mid jd[0]", mid_points[0].jd_tdb, 2451545.0, 1e-12);
    assert_close("mid ra[0]", mid_points[0].ra_cio, 0.00201246480529, 1e-12);
    assert_close("mid jd[1]", mid_points[1].jd_tdb, 2451546.2, 1e-12);
    assert_close("mid ra[1]", mid_points[1].ra_cio, 0.00201467274626, 1e-12);

    // End of file: last two records (anchor on second-to-last index).
    let mut end_points = [
        novas::ra_of_cio {
            jd_tdb: 0.0,
            ra_cio: 0.0,
        },
        novas::ra_of_cio {
            jd_tdb: 0.0,
            ra_cio: 0.0,
        },
    ];
    let end_status = unsafe { novas::cio_array(2561137.5, 2 as c_long, end_points.as_mut_ptr()) };
    assert_eq!(end_status, 0);
    assert_close("end jd[0]", end_points[0].jd_tdb, 2561137.4, 1e-12);
    assert_close("end ra[0]", end_points[0].ra_cio, 1.94201099482202, 1e-12);
    assert_close("end jd[1]", end_points[1].jd_tdb, 2561138.6, 1e-12);
    assert_close("end ra[1]", end_points[1].ra_cio, 1.94212513149679, 1e-12);
}
