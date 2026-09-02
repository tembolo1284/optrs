// crates/optrs-cabi/build.rs
//! Generates include/optrs.h at build time. The header is checked into the repo
//! so consumers who only want the C interface do not need a Rust toolchain.

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = crate_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("include").join("optrs.h"))
        .expect("workspace layout: crates/optrs-cabi");

    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).ok();
    }

    // Header generation is a convenience, not a build requirement: if cbindgen
    // fails (e.g. offline vendored build) we warn rather than break the build.
    match cbindgen::generate(&crate_dir) {
        Ok(bindings) => {
            bindings.write_to_file(&out);
        }
        Err(e) => println!("cargo:warning=cbindgen skipped: {e}"),
    }

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}
