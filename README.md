# novas

**High-precision positional astronomy for Rust and WebAssembly.**

Rust bindings for the U.S. Naval Observatory (USNO) [NOVAS C3.1 library](https://aa.usno.navy.mil/software/novasc_intro). Whether you are building native desktop apps or web-based planetariums, `novas` provides the gold-standard algorithms for celestial mechanics with Rust.

## API shape

This crate provides two layers of abstraction:

- `novas::sys`: Raw FFI bindings. Use this if you need 1:1 parity with legacy C codebases.
- An idiomatic Rust wrapper designed for better type discovery and cleaner signatures.

> [!NOTE]
> The Path to Safety: While these calls currently require unsafe blocks because they interface directly with C memory, we are aiming toward a fully safe, "Rusty" API. Transitioning the root to safe Rust is a primary goal of the upcoming releases.

## 🔬 Precision & Trust

We treat astronomical accuracy as a non-negotiable requirement.

- **Upstream Logic**: All scientific algorithms remain in the original, battle-tested C source.
- **Continuous Validation**: Every commit is tested against USNO reference outputs on both x86_64 and WASM targets.
- **No Hidden Magic**: Final binaries contain zero AI-generated logic; AI was used solely to map the FFI surface area.

## 🌍 See it in Action: Solar Time in Paris

Want to see these algorithms handle real-world calculations? Check out [examples/paris_solar_time.rs](examples/paris_solar_time.rs). This example demonstrates how to bridge the gap between standard `SystemTime` and high-precision astrometry. It calculates the *Equation of Time* (the difference between the time on your watch and the actual position of the sun) by computing the Sun's apparent right ascension and the local sidereal time for Paris. It’s a perfect starting point for understanding how to manage Julian dates, `ΔT` corrections, and coordinate transformations in your own projects.

## 📜 Attribution

The [Naval Observatory Vector Astrometry Software](https://aa.usno.navy.mil/software/novas_info) (NOVAS) is developed and maintained by the Astronomical Applications Department of the U.S. Naval Observatory (USNO). This crate serves as a bridge to their foundational work in high-precision astrometry.

If you use this library in research or software that requires formal attribution, please cite the official [User's Guide](https://aa.usno.navy.mil/downloads/novas/NOVAS_C3.1_Guide.pdf):

> Bangert, J., Puatua, W., Kaplan, G., Bartlett, J., Harris, W., Fredericks, A., & Monet, A. 2011, User's Guide to NOVAS Version C3.1 (Washington, DC: USNO).

## 💻 Platform Support

This crate is designed to run everywhere, from high-performance native servers to browser-based applications.

- **Native Targets**: Full support for desktop and server environments.
- **WebAssembly**: Fully compatible with `wasm32-unknown-unknown` and wasm-bindgen workflows.

### WASM-specific Virtual File System:

Certain NOVAS functions require binary data (e.g., `cio_ra.bin`). On WASM targets, this crate provides a virtual file layer to keep these code paths functional. By default, the `cio_ra.bin` file is bundled into your binary for a "plug-and-play" experience. To reduce your WASM payload size, you can disable default features and provide the necessary files manually.

```toml
[dependencies]
# Standard installation with embedded data
novas = "0.1"

# Slim WASM build without embedded files
novas = { version = "0.1", default-features = false }
```

## 🤖 AI policy

Transparency is a priority for this project. To ensure high-fidelity bindings across a large API surface, this crate was developed using AI-assisted code generation under the following strict constraints:

- Scaffolding Only: AI was used exclusively to generate the FFI boilerplate and Rust-facing wrapper signatures.
- No "Black Box" Logic: **Final binaries contain zero AI-generated algorithms**. All numerical results are produced by the original NOVAS C source and the Rust toolchain.
- Verified Outputs: Every binding is validated through a comprehensive test suite that compares Rust outputs against the official USNO reference data.

License: GPLv3-only
