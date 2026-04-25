#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_FILE="${1:-$ROOT_DIR/tests/data/parity_expected.txt}"
NOVAS_DIR="$ROOT_DIR/novasc3.1"

if [[ ! -d "$NOVAS_DIR" ]]; then
  echo "expected vendored NOVAS sources at $NOVAS_DIR" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cat > "$tmp_dir/parity.c" <<'EOF'
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
EOF

cc -std=c99 \
  -I "$NOVAS_DIR" \
  "$tmp_dir/parity.c" \
  "$NOVAS_DIR/novas.c" \
  "$NOVAS_DIR/novascon.c" \
  "$NOVAS_DIR/nutation.c" \
  "$NOVAS_DIR/solsys3.c" \
  "$NOVAS_DIR/readeph0.c" \
  -lm \
  -o "$tmp_dir/parity"

mkdir -p "$(dirname "$OUT_FILE")"
"$tmp_dir/parity" > "$OUT_FILE"

echo "Wrote parity baseline to $OUT_FILE"
