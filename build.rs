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

    let ac = autocfg::new();
    let is_nightly = version_meta().unwrap().channel == Channel::Nightly;

    println!("cargo::rerun-if-env-changed=LITHIUM_THREAD_LOCAL");
    if let Ok(thread_local) = std::env::var("LITHIUM_THREAD_LOCAL") {
        println!("cargo::rustc-cfg=thread_local=\"{thread_local}\"");
    } else if is_nightly && has_cfg("target_thread_local") {
        println!("cargo::rustc-cfg=thread_local=\"attribute\"");
    } else if ac
        .probe_raw(
            r"
        #![no_std]
        extern crate std;
        std::thread_local! {
            static FOO: () = ();
        }
    ",
        )
        .is_ok()
    {
        println!("cargo::rustc-cfg=thread_local=\"std\"");
    } else {
        println!("cargo::rustc-cfg=thread_local=\"unimplemented\"");
    }

    println!("cargo::rerun-if-env-changed=LITHIUM_BACKEND");
    if let Ok(backend) = std::env::var("LITHIUM_BACKEND") {
        println!("cargo::rustc-cfg=backend=\"{backend}\"");
    } else if is_nightly && cfg("target_os") == "emscripten" {
        println!("cargo::rustc-cfg=backend=\"emscripten\"");
    } else if is_nightly
        && (has_cfg("unix")
            || (has_cfg("windows") && cfg("target_env") == "gnu")
            || cfg("target_arch") == "wasm32")
    {
        println!("cargo::rustc-cfg=backend=\"itanium\"");
    } else if is_nightly && (has_cfg("windows") && cfg("target_env") == "msvc") && !is_miri {
        println!("cargo::rustc-cfg=backend=\"seh\"");
    } else if ac
        .probe_raw(
            r"
        #![no_std]
        extern crate std;
        use std::panic::{catch_unwind, resume_unwind};
        ",
        )
        .is_ok()
    {
        println!("cargo::rustc-cfg=backend=\"panic\"");
    } else {
        println!("cargo::rustc-cfg=backend=\"unimplemented\"");
    }

    if ac
        .probe_raw(
            r#"
        #![no_std]
        extern crate std;
        use std::io::Write;
        fn main() {
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
