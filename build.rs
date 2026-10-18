// Embed the application manifest (per-monitor DPI awareness, common controls
// v6, Windows 10+ compatibility) with the MSVC linker; no resource compiler needed.
fn main() {
    println!("cargo:rerun-if-changed=snipper.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let manifest = std::path::Path::new(&dir).join("snipper.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
