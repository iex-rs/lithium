use rustc_version::{version_meta, Channel};

fn main() {
    println!("cargo::rerun-if-env-changed=MIRIFLAGS");
    let is_miri = std::env::var_os("CARGO_CFG_MIRI").is_some();
    let is_tree_borrows =
        std::env::var("MIRIFLAGS").is_ok_and(|flags| flags.contains("-Zmiri-tree-borrows"));
    if is_miri && !is_tree_borrows {
        println!("cargo::rustc-cfg=feature=\"sound-under-stacked-borrows\"");
    }

    println!("cargo::rerun-if-env-changed=LITHIUM_BACKEND");
    if let Ok(backend) = std::env::var("LITHIUM_BACKEND") {
        println!("cargo::rustc-cfg=backend=\"{backend}\"");
    } else {
        if version_meta().unwrap().channel == Channel::Nightly
            && (std::env::var_os("CARGO_CFG_UNIX").is_some()
                || (std::env::var_os("CARGO_CFG_WINDOWS").is_some()
                    && std::env::var_os("CARGO_CFG_TARGET_ENV").is_some_and(|env| env == "gnu")))
        {
            println!("cargo::rustc-cfg=backend=\"itanium\"");
        } else {
            println!("cargo::rustc-cfg=backend=\"panic\"");
        }
    }
}
