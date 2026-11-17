//! Muestrea los informes Feature candidatos (solo lectura) cada 2 s.
#![allow(dead_code)]
#[path = "../src/ffi.rs"]
mod ffi;

use core::ffi::c_void;
use ffi::*;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

const GUID_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(60);
    unsafe {
        let mut len = 0u32;
        CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_HID, null(), 0);
        let mut buf = vec![0u16; len as usize];
        CM_Get_Device_Interface_ListW(&GUID_HID, null(), buf.as_mut_ptr(), len, 0);
        let mut targets = Vec::new();
        for p in buf.split(|&c| c == 0).filter(|s| !s.is_empty()) {
            let p: Vec<u16> = p.iter().copied().chain(Some(0)).collect();
            let h = CreateFileW(p.as_ptr(), 0, 3, null(), OPEN_EXISTING, 0, null_mut());
            if h == invalid_handle() { continue; }
            let mut a: HIDD_ATTRIBUTES = zeroed();
            a.size = 12;
            HidD_GetAttributes(h, &mut a);
            let mut pp: PREPARSED = 0;
            HidD_GetPreparsedData(h, &mut pp);
            let mut c: HIDP_CAPS = zeroed();
            HidP_GetCaps(pp, &mut c);
            HidD_FreePreparsedData(pp);
            let id = match (a.vendor_id, a.product_id, c.usage_page) {
                (0x0951, 0x170B, 0xFF00) => 0xFF,
                (0x3554, 0xFA09, 0xFF04) => 0x06,
                _ => { CloseHandle(h); continue; }
            };
            targets.push((format!("{:04X}:{:04X}", a.vendor_id, a.product_id), h, id, c.feature_len as usize));
        }
        let t = std::time::Instant::now();
        while t.elapsed().as_secs() < secs {
            for (name, h, id, flen) in &targets {
                let mut b = vec![0u8; *flen];
                b[0] = *id;
                let ok = HidD_GetFeature(*h, b.as_mut_ptr() as *mut c_void, b.len() as u32);
                let n = b.len().min(18);
                println!("+{:>3}s {name} ok={ok} {}", t.elapsed().as_secs(), b[..n].iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" "));
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }
}
