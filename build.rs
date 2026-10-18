// Embed the application manifest (per-monitor DPI awareness, common controls
// v6, Windows 10+ compatibility) and icon with MSVC; no resource compiler needed.
use std::{env, fs, path::Path};

// A .res entry has a DWORD-aligned header followed by DWORD-aligned data.
// Ordinal types 3 and 14 are RT_ICON and RT_GROUP_ICON, respectively.
fn resource(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for word in [0xffff, kind, 0xffff, id] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&0x1030u16.to_le_bytes()); // MOVEABLE | PURE | DISCARDABLE
    out.extend_from_slice(&0u16.to_le_bytes()); // language neutral
    out.extend_from_slice(&[0; 8]); // Version, Characteristics
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn embed_icon() {
    let ico = fs::read("assets/snipper.ico").expect("read application icon");
    assert_eq!(&ico[..4], &[0, 0, 1, 0], "expected an ICO file");
    let count = u16::from_le_bytes(ico[4..6].try_into().unwrap());
    let mut res = Vec::new();
    resource(&mut res, 0, 0, &[]); // required null resource header
    let mut group = ico[..6].to_vec();
    for i in 0..count {
        let entry = &ico[6 + i as usize * 16..6 + (i as usize + 1) * 16];
        let size = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        resource(&mut res, 3, i + 1, &ico[offset..offset + size]);
        group.extend_from_slice(&entry[..12]);
        group.extend_from_slice(&(i + 1).to_le_bytes());
    }
    resource(&mut res, 14, 1, &group);
    let path = Path::new(&env::var_os("OUT_DIR").unwrap()).join("snipper.res");
    fs::write(&path, res).expect("write icon resources");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}

fn main() {
    println!("cargo:rerun-if-changed=snipper.manifest");
    println!("cargo:rerun-if-changed=assets/snipper.ico");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let manifest = std::path::Path::new(&dir).join("snipper.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
        embed_icon();
    }
}
