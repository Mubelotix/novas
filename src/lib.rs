#![doc = include_str!("../README.md")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

/// Upstream NOVAS C version used by this crate.
pub const NOVAS_UPSTREAM_VERSION: &str = env!("NOVAS_UPSTREAM_VERSION");
