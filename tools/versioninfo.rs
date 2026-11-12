//! Shared by the build scripts of `m87` and `m87-gui`: write a Windows
//! version resource (`VS_VERSIONINFO`) as a `.res` file and hand it to the
//! MSVC linker. Code signing requires the product name and version to be
//! set in every signed file. No resource compiler is needed.

use std::path::PathBuf;

const PRODUCT: &str = "m87pro-tool";
const COMPANY: &str = "rity73";
const COPYRIGHT: &str = "Copyright (c) 2026 rity73. Licensed under MIT or Apache-2.0.";

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(std::iter::once(0)).flat_map(|c| c.to_le_bytes()).collect()
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

/// One version-info block: header, key, value, then child blocks.
/// `text` marks a string value (its length is counted in characters).
fn block(key: &str, value: &[u8], text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0u8; 6];
    b.extend(utf16z(key));
    pad4(&mut b);
    b.extend_from_slice(value);
    for c in children {
        pad4(&mut b);
        b.extend_from_slice(c);
    }
    let value_len = if text { value.len() / 2 } else { value.len() };
    let total = b.len() as u16;
    b[0..2].copy_from_slice(&total.to_le_bytes());
    b[2..4].copy_from_slice(&(value_len as u16).to_le_bytes());
    b[4..6].copy_from_slice(&(text as u16).to_le_bytes());
    b
}

fn version_info(description: &str, file_name: &str, version: &str) -> Vec<u8> {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, minor, patch) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let ms = (major << 16) | minor;
    let ls = patch << 16;
    let mut fixed = Vec::new();
    for v in [
        0xFEEF_04BDu32, // signature
        0x0001_0000,    // structure version
        ms,
        ls, // file version
        ms,
        ls,          // product version
        0x3F,        // flags mask
        0,           // flags
        0x0004_0004, // VOS_NT_WINDOWS32
        1,           // VFT_APP
        0,
        0,
        0,
    ] {
        fixed.extend_from_slice(&v.to_le_bytes());
    }
    let strings: Vec<Vec<u8>> = [
        ("CompanyName", COMPANY),
        ("FileDescription", description),
        ("FileVersion", version),
        ("InternalName", file_name.trim_end_matches(".exe")),
        ("LegalCopyright", COPYRIGHT),
        ("OriginalFilename", file_name),
        ("ProductName", PRODUCT),
        ("ProductVersion", version),
    ]
    .iter()
    .map(|(k, v)| block(k, &utf16z(v), true, &[]))
    .collect();
    let table = block("040904B0", &[], true, &strings);
    let string_info = block("StringFileInfo", &[], true, &[table]);
    // English (US), Unicode code page.
    let translation = block("Translation", &[0x09, 0x04, 0xB0, 0x04], false, &[]);
    let var_info = block("VarFileInfo", &[], true, &[translation]);
    block("VS_VERSION_INFO", &fixed, false, &[string_info, var_info])
}

/// Write the resource file and link it into the crate's programs.
pub fn embed(description: &str, file_name: &str) {
    println!("cargo:rerun-if-changed=../../tools/versioninfo.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let data = version_info(description, file_name, &version);

    // .res file: an empty first entry, then the RT_VERSION entry.
    let mut res: Vec<u8> = vec![0, 0, 0, 0, 0x20, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0xFF, 0xFF, 0, 0];
    res.extend_from_slice(&[0u8; 16]);
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes()); // header size
    res.extend_from_slice(&[0xFF, 0xFF, 16, 0]); // type RT_VERSION
    res.extend_from_slice(&[0xFF, 0xFF, 1, 0]); // name 1
    res.extend_from_slice(&0u32.to_le_bytes()); // data version
    res.extend_from_slice(&0x0030u16.to_le_bytes()); // moveable, pure
    res.extend_from_slice(&0x0409u16.to_le_bytes()); // language
    res.extend_from_slice(&0u32.to_le_bytes()); // version
    res.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    res.extend_from_slice(&data);
    pad4(&mut res);

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("version.res");
    std::fs::write(&out, res).unwrap();
    println!("cargo:rustc-link-arg-bins={}", out.display());
}
