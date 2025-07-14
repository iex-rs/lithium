use rustc_version::{Channel, version_meta};

fn has_cfg(name: &str) -> bool {
    std::env::var_os(format!("CARGO_CFG_{}", name.to_uppercase())).is_some()
}

fn cfg(name: &str) -> String {
    std::env::var(format!("CARGO_CFG_{}", name.to_uppercase())).unwrap_or_default()
}

fn make_overridable_cfg(name: &str, logic: impl FnOnce() -> &'static str) -> String {
    let env_name = format!("LITHIUM_{}", name.to_uppercase());
    println!("cargo::rerun-if-env-changed={env_name}");
    let value = std::env::var(env_name).unwrap_or_else(|_| logic().to_string());
    println!("cargo::rustc-cfg={name}=\"{value}\"");
    value
}

fn main() {
    println!("cargo::rerun-if-env-changed=MIRIFLAGS");
    let is_miri = has_cfg("miri");
    let is_tree_borrows =
        std::env::var("MIRIFLAGS").is_ok_and(|flags| flags.contains("-Zmiri-tree-borrows"));
    if is_miri && !is_tree_borrows {
        println!("cargo::rustc-cfg=feature=\"sound-under-stacked-borrows\"");
    }

    let is_nightly = version_meta().unwrap().channel == Channel::Nightly;

    // We've previously used `autocfg` to check if `std` is available, and emitted errors when
    // compiling without `std` on stable. But that didn't work well: when std is available due to
    // `-Z build-std`, autocfg doesn't notice it [1], so the tests within this build script would
    // fail even though using `std` from within the crate would work. So instead, we just assume
    // that `std` is present if nothing else works -- that leads to worse diagnostics in the failure
    // case, but makes the common one actually work.
    //
    // [1]: https://github.com/cuviper/autocfg/issues/34

    make_overridable_cfg("thread_local", || {
        if is_nightly && has_cfg("target_thread_local") {
            "attribute"
        } else {
            "std"
        }
    });

    let backend = make_overridable_cfg("backend", || {
        if is_nightly && cfg("target_os") == "emscripten" && !has_cfg("emscripten_wasm_eh") {
            "emscripten"
        } else if is_nightly && cfg("target_arch") == "wasm32" {
            // Catching a foreign Itanium exception from within Rust is (currently) guaranteed to
            // abort, but the optimizations we use for Wasm cause std to read uninitialized memory
            // in this case. Make missing `catch` or misplaced `catch_unwind` calls easier to debug
            // by switching to the more robust, but slower mechanism in debug mode. We can't use
            // `has_cfg("debug_assertions")` due to https://github.com/rust-lang/cargo/issues/7634.
            if std::env::var("PROFILE").unwrap_or_default() == "debug" {
                "itanium"
            } else {
                "wasm"
            }
        } else if is_nightly
            && (has_cfg("unix") || (has_cfg("windows") && cfg("target_env") == "gnu"))
        {
            "itanium"
        } else if is_nightly && has_cfg("windows") && cfg("target_env") == "msvc" && !is_miri {
            "seh"
        } else {
            "panic"
        }
    });

    // Since the panic backend can use `abort` and is available on stable, we need to set
    // `abort = "std"` whenever the panic backend is used, even if we don't readily know if `std` is
    // available. But that's fine, since the panic backend requires `std` anyway.
    let ac = autocfg::new();
    if backend == "panic"
        || ac
            .probe_raw(
                r#"
        #![no_std]
        extern crate std;
        use std::io::Write;
        pub fn main() {
            let _ = std::io::stderr().write_all(b"hello");
            std::process::abort();
        }
    "#,
            )
            .is_ok()
    {
        println!("cargo::rustc-cfg=abort=\"std\"");
    } else {
        println!("cargo::rustc-cfg=abort=\"core\"");
    }
}
