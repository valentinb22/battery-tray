//! Añade al ejecutable compilado el icono de la aplicación y la información de versión
//! (nombre "Battery Tray" en el Explorador, el menú Inicio y las notificaciones).
//! El toolchain GNU no trae `windres`, así que los recursos se escriben con la API
//! UpdateResource de Windows sobre una copia del .exe.
//! Uso: cargo run --release --example stamp -- <entrada.exe> <salida.exe> [vista.bmp]
#![allow(dead_code)]
#[path = "../src/ffi.rs"]
mod ffi;
#[path = "../src/icon.rs"]
mod icon;

use core::ffi::c_void;
use ffi::{wide, BOOL, HANDLE};

windows_targets::link!("kernel32.dll" "system" fn BeginUpdateResourceW(file: *const u16, delete_existing: BOOL) -> HANDLE);
windows_targets::link!("kernel32.dll" "system" fn UpdateResourceW(h: HANDLE, ty: *const u16, name: *const u16, lang: u16, data: *const c_void, len: u32) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn EndUpdateResourceW(h: HANDLE, discard: BOOL) -> BOOL);

const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;
const RT_VERSION: usize = 16;
const LANG_NEUTRAL: u16 = 0;
const SIZES: [usize; 7] = [16, 20, 24, 32, 48, 64, 128];

fn res_id(n: usize) -> *const u16 {
    n as *const u16
}

fn le32(v: &mut Vec<u8>, xs: &[u32]) {
    for x in xs {
        v.extend_from_slice(&x.to_le_bytes());
    }
}

fn le16(v: &mut Vec<u8>, xs: &[u16]) {
    for x in xs {
        v.extend_from_slice(&x.to_le_bytes());
    }
}

/// Imagen de icono en formato DIB: cabecera con alto doble, píxeles BGRA de abajo arriba
/// y máscara AND vacía (la transparencia la da el alfa).
fn dib(size: usize, px: &[u32]) -> Vec<u8> {
    let mut v = Vec::new();
    let mask_stride = ((size + 31) / 32) * 4;
    let image = size * size * 4 + mask_stride * size;
    le32(&mut v, &[40, size as u32, (size * 2) as u32]);
    le16(&mut v, &[1, 32]); // planos, bits por píxel
    le32(&mut v, &[0, image as u32, 0, 0, 0, 0]);
    for y in (0..size).rev() {
        for x in 0..size {
            v.extend_from_slice(&px[y * size + x].to_le_bytes()); // ARGB → B G R A en memoria
        }
    }
    v.resize(v.len() + mask_stride * size, 0);
    v
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(|c| c.to_le_bytes()).collect()
}

fn pad4(v: &mut Vec<u8>) {
    while v.len() % 4 != 0 {
        v.push(0);
    }
}

/// Nodo de VS_VERSIONINFO: wLength, wValueLength, wType, clave, valor, hijos.
fn node(key: &str, value: &[u8], text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut v = vec![0u8; 6];
    v.extend(utf16z(key));
    pad4(&mut v);
    v.extend_from_slice(value);
    for c in children {
        pad4(&mut v);
        v.extend_from_slice(c);
    }
    let value_len = if text { value.len() / 2 } else { value.len() };
    let total = v.len() as u16;
    v[0..2].copy_from_slice(&total.to_le_bytes());
    v[2..4].copy_from_slice(&(value_len as u16).to_le_bytes());
    v[4..6].copy_from_slice(&(text as u16).to_le_bytes());
    v
}

fn version_info() -> Vec<u8> {
    let ver = env!("CARGO_PKG_VERSION");
    let mut parts = ver.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (a, b, c) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let (ms, ls) = ((a << 16) | b, c << 16);
    let mut fixed = Vec::new();
    // Firma, versión de estructura, archivo, producto, máscara, flags, VOS_NT_WINDOWS32, VFT_APP…
    le32(&mut fixed, &[0xFEEF04BD, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 1, 0, 0, 0]);
    let strings: Vec<Vec<u8>> = [
        ("FileDescription", "Battery Tray"),
        ("ProductName", "Battery Tray"),
        ("FileVersion", ver),
        ("ProductVersion", ver),
        ("InternalName", "battery-tray"),
        ("OriginalFilename", "battery-tray.exe"),
        ("Comments", "Batería de periféricos inalámbricos en la bandeja de Windows"),
    ]
    .iter()
    .map(|(k, v)| node(k, &utf16z(v), true, &[]))
    .collect();
    // Español (0x0C0A), Unicode (1200 = 0x04B0).
    let table = node("0C0A04B0", &[], true, &strings);
    let sfi = node("StringFileInfo", &[], true, &[table]);
    let mut translation = Vec::new();
    le16(&mut translation, &[0x0C0A, 1200]);
    let var = node("Translation", &translation, false, &[]);
    let vfi = node("VarFileInfo", &[], true, &[var]);
    node("VS_VERSION_INFO", &fixed, false, &[sfi, vfi])
}

fn preview_bmp(path: &str) {
    let s = 128usize;
    let px = icon::app_icon(s);
    let row = s * 4;
    let mut f = Vec::new();
    f.extend_from_slice(b"BM");
    le32(&mut f, &[(54 + row * s) as u32, 0, 54, 40, s as u32, s as u32]);
    le16(&mut f, &[1, 32]);
    le32(&mut f, &[0, (row * s) as u32, 0, 0, 0, 0]);
    for y in (0..s).rev() {
        for x in 0..s {
            // Compuesto sobre blanco.
            let c = px[y * s + x];
            let a = c >> 24;
            let blend = |sh: u32| ((((c >> sh) & 0xFF) * a + 255 * (255 - a)) / 255) as u8;
            f.extend_from_slice(&[blend(0), blend(8), blend(16), 255]);
        }
    }
    std::fs::write(path, f).expect("no se pudo escribir la vista previa");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("uso: stamp <entrada.exe> <salida.exe> [vista.bmp]");
        std::process::exit(2);
    };
    if let Some(parent) = std::path::Path::new(output).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::copy(input, output).expect("no se pudo copiar el ejecutable");

    let images: Vec<(usize, Vec<u8>)> = SIZES.iter().map(|&s| (s, dib(s, &icon::app_icon(s)))).collect();
    let mut group = Vec::new();
    le16(&mut group, &[0, 1, images.len() as u16]);
    for (i, (s, data)) in images.iter().enumerate() {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        group.extend_from_slice(&[dim, dim, 0, 0]);
        le16(&mut group, &[1, 32]);
        le32(&mut group, &[data.len() as u32]);
        le16(&mut group, &[(i + 1) as u16]);
    }
    let version = version_info();

    unsafe {
        let path = wide(output);
        let h = BeginUpdateResourceW(path.as_ptr(), 0);
        assert!(!h.is_null(), "BeginUpdateResource falló");
        let mut ok = true;
        for (i, (_, data)) in images.iter().enumerate() {
            ok &= UpdateResourceW(h, res_id(RT_ICON), res_id(i + 1), LANG_NEUTRAL, data.as_ptr() as *const c_void, data.len() as u32) != 0;
        }
        ok &= UpdateResourceW(h, res_id(RT_GROUP_ICON), res_id(1), LANG_NEUTRAL, group.as_ptr() as *const c_void, group.len() as u32) != 0;
        ok &= UpdateResourceW(h, res_id(RT_VERSION), res_id(1), LANG_NEUTRAL, version.as_ptr() as *const c_void, version.len() as u32) != 0;
        assert!(ok, "UpdateResource falló");
        assert!(EndUpdateResourceW(h, 0) != 0, "EndUpdateResource falló");
    }
    println!("{output}: icono ({} tamaños) y versión {} añadidos", SIZES.len(), env!("CARGO_PKG_VERSION"));

    if let Some(bmp) = args.get(3) {
        preview_bmp(bmp);
    }
}
