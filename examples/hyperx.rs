//! Prueba del protocolo de batería HyperX 0951:170B (Feature 0xFF, MI_03 Col03).
#![allow(dead_code)]
#[path = "../src/ffi.rs"]
mod ffi;

use core::ffi::c_void;
use ffi::*;
use std::ptr::{null, null_mut};

const GUID_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);
const CONN: [u8; 18] = [0xFF, 0x07, 0x00, 0xFD, 0x04, 0x00, 0x00, 0x02, 0x82, 0x03, 0x05, 0x29, 0x0F, 0x00, 0x00, 0x03, 0x68, 0x10];
const BATT: [u8; 12] = [0xFF, 0x07, 0x00, 0xFD, 0x04, 0x0C, 0xF1, 0x02, 0x01, 0x04, 0xF0, 0x0C];

unsafe fn exchange(h: HANDLE, req: &[u8]) -> Option<(Vec<u8>, u128)> {
    let mut out = vec![0u8; 64];
    out[..req.len()].copy_from_slice(req);
    if HidD_SetFeature(h, out.as_ptr() as *const c_void, 64) == 0 {
        println!("  SetFeature err {}", GetLastError());
        return None;
    }
    let t = std::time::Instant::now();
    while t.elapsed().as_millis() < 1000 {
        let mut r = vec![0u8; 64];
        r[0] = 0xFF;
        if HidD_GetFeature(h, r.as_mut_ptr() as *mut c_void, 64) != 0
            && r[3] == 0xFE && r[2] != 0 && r[4] == req[5] && r[5] == req[4]
        {
            return Some((r, t.elapsed().as_millis()));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    None
}

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ") }

fn main() {
    unsafe {
        let mut len = 0u32;
        CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_HID, null(), 0);
        let mut buf = vec![0u16; len as usize];
        CM_Get_Device_Interface_ListW(&GUID_HID, null(), buf.as_mut_ptr(), len, 0);
        for p in buf.split(|&c| c == 0).filter(|s| !s.is_empty()) {
            let p: Vec<u16> = p.iter().copied().chain(Some(0)).collect();
            let s = String::from_utf16_lossy(&p).to_lowercase();
            if !(s.contains("vid_0951&pid_170b&mi_03&col03")) { continue; }
            let h = CreateFileW(p.as_ptr(), GENERIC_READ | GENERIC_WRITE, 3, null(), OPEN_EXISTING, 0, null_mut());
            if h == invalid_handle() { println!("no se pudo abrir: {}", GetLastError()); continue; }
            for i in 0..5 {
                match exchange(h, &CONN) {
                    Some((r, ms)) => println!("conn  ({ms} ms): {}  -> byte11={}", hex(&r[..20]), r[11]),
                    None => println!("conn: sin respuesta fresca"),
                }
                match exchange(h, &BATT) {
                    Some((r, ms)) => println!("batt  ({ms} ms): {}  -> estado={} mV={}", hex(&r[..20]), r[9], u16::from_le_bytes([r[11], r[12]])),
                    None => println!("batt: sin respuesta fresca"),
                }
                let _ = i;
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            CloseHandle(h);
        }
    }
}
