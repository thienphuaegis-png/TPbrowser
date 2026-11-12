use std::env;
use std::fs;
use std::path::PathBuf;

fn push_res_entry(out: &mut Vec<u8>, res_type: u16, res_id: u16, data: &[u8]) {
    let data_size = data.len() as u32;
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_type.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&0x1030u16.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&0x0409u16.to_le_bytes()); // LanguageId (en-US)
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn build_windows_icon_res(ico: &[u8]) -> Option<Vec<u8>> {
    if ico.len() < 6 {
        return None;
    }
    let res_type = u16::from_le_bytes([ico[2], ico[3]]);
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    if res_type != 1 || count == 0 || ico.len() < 6 + count * 16 {
        return None;
    }

    let mut res = Vec::with_capacity(ico.len() + 512);
    // Mandatory 32-byte null header at start of .res file
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&0u16.to_le_bytes());
    res.extend_from_slice(&[0u8; 16]);

    let mut group = Vec::with_capacity(6 + count * 14);
    group.extend_from_slice(&0u16.to_le_bytes());
    group.extend_from_slice(&1u16.to_le_bytes());
    group.extend_from_slice(&(count as u16).to_le_bytes());

    for i in 0..count {
        let base = 6 + i * 16;
        let w = ico[base];
        let h = ico[base + 1];
        let color_count = ico[base + 2];
        let reserved = ico[base + 3];
        let planes = u16::from_le_bytes([ico[base + 4], ico[base + 5]]);
        let bpp = u16::from_le_bytes([ico[base + 6], ico[base + 7]]);
        let size = u32::from_le_bytes([ico[base + 8], ico[base + 9], ico[base + 10], ico[base + 11]]) as usize;
        let offset = u32::from_le_bytes([ico[base + 12], ico[base + 13], ico[base + 14], ico[base + 15]]) as usize;
        if offset + size > ico.len() {
            return None;
        }
        let icon_id = (i + 1) as u16;
        push_res_entry(&mut res, 3, icon_id, &ico[offset..offset + size]);

        group.push(w);
        group.push(h);
        group.push(color_count);
        group.push(reserved);
        group.extend_from_slice(&planes.to_le_bytes());
        group.extend_from_slice(&bpp.to_le_bytes());
        group.extend_from_slice(&(size as u32).to_le_bytes());
        group.extend_from_slice(&icon_id.to_le_bytes());
    }

    push_res_entry(&mut res, 14, 1, &group);
    Some(res)
}

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/tpbrowser.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico_path = PathBuf::from("assets/icons/tpbrowser.ico");
    let Ok(ico_bytes) = fs::read(&ico_path) else {
        return;
    };
    let Some(res_bytes) = build_windows_icon_res(&ico_bytes) else {
        return;
    };
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let res_path = out_dir.join("tpbrowser_icon.res");
    fs::write(&res_path, res_bytes).expect("write tpbrowser_icon.res");
    println!("cargo:rustc-link-arg={}", res_path.display());
}
