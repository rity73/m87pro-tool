//! Embed the version resource (product name and version) into `m87.exe`.

#[path = "../../tools/versioninfo.rs"]
mod versioninfo;

fn main() {
    versioninfo::embed("M87 Pro settings (command line and terminal)", "m87.exe");
}
