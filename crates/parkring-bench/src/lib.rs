//! Benchmarks and chart generation for `parkring`. Not published.
//!
//! The benches live in `benches/`, the chart generator in `examples/plot.rs`.

use std::path::{Path, PathBuf};

/// The workspace root, where `target/` and `assets/` live.
#[must_use]
pub fn workspace_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// The cargo target directory, honouring `CARGO_TARGET_DIR`.
pub fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root().join("target"), PathBuf::from)
}
