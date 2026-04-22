fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(no_libghosty)");

    let libghosty_dir = std::path::Path::new("libghosty");

    if !libghosty_dir.exists() {
        println!(
            "cargo:warning=libghosty/ directory not found — building without libghosty support"
        );
        println!("cargo:rustc-cfg=no_libghosty");
    }
}
