//! Embed the application manifest (common controls 6, per-monitor DPI) and
//! the version resource. No resource compiler is needed: the MSVC linker
//! embeds both.

#[path = "../../tools/versioninfo.rs"]
mod versioninfo;

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest = std::path::Path::new(&dir).join("m87-gui.manifest");
    println!("cargo:rerun-if-changed=m87-gui.manifest");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
    versioninfo::embed("M87 Pro settings (window version)", "m87-gui.exe");
}
