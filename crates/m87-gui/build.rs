//! Embed the application manifest (common controls 6, per-monitor DPI).
//! No resource compiler is needed: the MSVC linker embeds the file.

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest = std::path::Path::new(&dir).join("m87-gui.manifest");
    println!("cargo:rerun-if-changed=m87-gui.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
