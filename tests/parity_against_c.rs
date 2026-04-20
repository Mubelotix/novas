#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use tempfile::tempdir;

static C_BASELINE: OnceLock<BTreeMap<String, f64>> = OnceLock::new();

#[test]
fn ffi_matches_c_reference_outputs() {
    let c = c_baseline();

    let era_rust = unsafe { novas::era(2451545.0, 0.0) };
    assert_close("era", era_rust, c["era"], 1e-13);

    let ee_ct_rust = unsafe { novas::ee_ct(2451545.0, 0.0, 0) };
    assert_close("ee_ct", ee_ct_rust, c["ee_ct"], 1e-13);

    let mut mobl = 0.0;
    let mut tobl = 0.0;
    let mut ee = 0.0;
    let mut dpsi = 0.0;
    let mut deps = 0.0;
    unsafe { novas::e_tilt(2451545.0, 0, &mut mobl, &mut tobl, &mut ee, &mut dpsi, &mut deps) };

    assert_close("mobl", mobl, c["mobl"], 1e-13);
    assert_close("tobl", tobl, c["tobl"], 1e-13);
    assert_close("ee", ee, c["ee"], 1e-13);
    assert_close("dpsi", dpsi, c["dpsi"], 1e-13);
    assert_close("deps", deps, c["deps"], 1e-13);

    let mut gst = 0.0;
    let status = unsafe { novas::sidereal_time(2451545.0, 0.0, 69.184, 0, 0, 0, &mut gst) };
    assert_eq!(status, c["sidereal_status"] as i16);
    assert_close("sidereal_gst", gst, c["sidereal_gst"], 1e-13);
}

fn c_baseline() -> &'static BTreeMap<String, f64> {
    C_BASELINE.get_or_init(|| run_c_reference().expect("failed to build/run C baseline"))
}

fn run_c_reference() -> Result<BTreeMap<String, f64>, Box<dyn std::error::Error>> {
    let archive = Path::new(novas::NOVAS_ARCHIVE_PATH);
    if !archive.exists() {
        return Err(format!("NOVAS archive path does not exist: {}", archive.display()).into());
    }

    let td = tempdir()?;
    let src_root = td.path().join("upstream");
    fs::create_dir_all(&src_root)?;
    extract_archive(archive, &src_root)?;

    let source_dir = src_root.join("novasc3.1");
    let c_file = td.path().join("baseline.c");
    let exe_file = td.path().join("baseline");

    fs::write(&c_file, C_REFERENCE_PROGRAM)?;

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let output = Command::new(&cc)
        .arg("-std=c99")
        .arg("-I")
        .arg(&source_dir)
        .arg(&c_file)
        .arg(source_dir.join("novas.c"))
        .arg(source_dir.join("novascon.c"))
        .arg(source_dir.join("nutation.c"))
        .arg(source_dir.join("solsys3.c"))
        .arg(source_dir.join("readeph0.c"))
        .arg("-lm")
        .arg("-o")
        .arg(&exe_file)
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "C reference compile failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let run = Command::new(&exe_file).output()?;
    if !run.status.success() {
        return Err(format!(
            "C reference execution failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
        .into());
    }

    parse_key_values(&String::from_utf8(run.stdout)?)
}

fn parse_key_values(s: &str) -> Result<BTreeMap<String, f64>, Box<dyn std::error::Error>> {
    let mut map = BTreeMap::new();

    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Some((k, v)) = line.split_once('=') else {
            return Err(format!("invalid output line: {line}").into());
        };

        map.insert(k.trim().to_string(), v.trim().parse::<f64>()?);
    }

    Ok(map)
}

fn extract_archive(archive_path: &Path, extraction_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

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

        let mut out = fs::File::create(out_path)?;
        io::copy(&mut entry, &mut out)?;
    }

    Ok(())
}

fn assert_close(name: &str, lhs: f64, rhs: f64, tol: f64) {
    let diff = (lhs - rhs).abs();
    assert!(
        diff <= tol,
        "{name} mismatch: rust={lhs:.17e}, c={rhs:.17e}, diff={diff:.3e}, tol={tol:.3e}"
    );
}

const C_REFERENCE_PROGRAM: &str = r#"
#include <stdio.h>
#include "novas.h"

int main(void) {
    double mobl = 0.0;
    double tobl = 0.0;
    double ee = 0.0;
    double dpsi = 0.0;
    double deps = 0.0;
    double gst = 0.0;

    double era_val = era(2451545.0, 0.0);
    double ee_ct_val = ee_ct(2451545.0, 0.0, 0);
    e_tilt(2451545.0, 0, &mobl, &tobl, &ee, &dpsi, &deps);
    short int sid_status = sidereal_time(2451545.0, 0.0, 69.184, 0, 0, 0, &gst);

    printf("era=%.17e\n", era_val);
    printf("ee_ct=%.17e\n", ee_ct_val);
    printf("mobl=%.17e\n", mobl);
    printf("tobl=%.17e\n", tobl);
    printf("ee=%.17e\n", ee);
    printf("dpsi=%.17e\n", dpsi);
    printf("deps=%.17e\n", deps);
    printf("sidereal_status=%d\n", (int)sid_status);
    printf("sidereal_gst=%.17e\n", gst);

    return 0;
}
"#;
