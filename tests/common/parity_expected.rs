use std::collections::BTreeMap;
use std::sync::OnceLock;

static EXPECTED: OnceLock<BTreeMap<String, f64>> = OnceLock::new();

pub fn expected_map() -> &'static BTreeMap<String, f64> {
    EXPECTED.get_or_init(|| {
        let mut out = BTreeMap::new();
        let raw = include_str!("../data/parity_expected.txt");

        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let (k, v) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("invalid parity baseline line: {line}"));

            let value = v
                .trim()
                .parse::<f64>()
                .unwrap_or_else(|e| panic!("invalid numeric baseline value for '{k}': {e}"));

            out.insert(k.trim().to_string(), value);
        }

        out
    })
}

pub fn assert_close(name: &str, lhs: f64, rhs: f64, tol: f64) {
    let diff = (lhs - rhs).abs();
    assert!(
        diff <= tol,
        "{name} mismatch: rust={lhs:.17e}, expected={rhs:.17e}, diff={diff:.3e}, tol={tol:.3e}"
    );
}
