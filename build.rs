use rustc_version::{version_meta, Channel};

fn main() {
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
