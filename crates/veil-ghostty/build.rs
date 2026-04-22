fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(no_libghosty)");

    // Look for the pre-built static library
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap(); // workspace root

    let lib_dir = workspace_root.join("vendor/ghostty-lib/lib");
    let lib_path = lib_dir.join("libghostty-vt.a");

    if !lib_path.exists() {
        println!(
            "cargo:warning=vendor/ghostty-lib/lib/libghostty-vt.a not found \
             -- building without libghosty support"
        );
        println!("cargo:rustc-cfg=no_libghosty");
        return;
    }

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");

    // libghostty-vt may depend on system libraries on macOS
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=System");
    }
}
