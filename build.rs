use rustc_version::{version_meta, Channel};

fn has_cfg(name: &str) -> bool {
    std::env::var_os(format!("CARGO_CFG_{}", name.to_uppercase())).is_some()
}

fn cfg(name: &str) -> String {
    std::env::var(format!("CARGO_CFG_{}", name.to_uppercase())).unwrap_or_default()
}

fn main() {
    println!("cargo::rerun-if-env-changed=MIRIFLAGS");
    let is_miri = has_cfg("miri");
    let is_tree_borrows =
        std::env::var("MIRIFLAGS").is_ok_and(|flags| flags.contains("-Zmiri-tree-borrows"));
    if is_miri && !is_tree_borrows {
        println!("cargo::rustc-cfg=feature=\"sound-under-stacked-borrows\"");
    }

    // RUSTC_BOOTSTRAP is just for testing. Promise.
    // XXX: https://github.com/rust-lang/rust/issues/135608
    let has_features = version_meta().unwrap().channel == Channel::Nightly
        || std::env::var("RUSTC_BOOTSTRAP").is_ok_and(|bootstrap| bootstrap == "1");

    println!("cargo::rerun-if-env-changed=LITHIUM_BACKEND");
    if let Ok(backend) = std::env::var("LITHIUM_BACKEND") {
        println!("cargo::rustc-cfg=backend=\"{backend}\"");
    } else if has_features && cfg("target_os") == "emscripten" && !has_cfg("emscripten_wasm_eh") {
        println!("cargo::rustc-cfg=backend=\"emscripten\"");
    } else if has_features
        && (has_cfg("unix")
            || (has_cfg("windows") && cfg("target_env") == "gnu")
            || cfg("target_arch") == "wasm32")
    {
        println!("cargo::rustc-cfg=backend=\"itanium\"");
    } else if has_features && (has_cfg("windows") && cfg("target_env") == "msvc") && !is_miri {
        println!("cargo::rustc-cfg=backend=\"seh\"");
    } else {
        #[cfg(feature = "std")]
        println!("cargo::rustc-cfg=backend=\"panic\"");
        #[cfg(not(feature = "std"))]
        println!("cargo::rustc-cfg=backend=\"unimplemented\"");
    }
}
