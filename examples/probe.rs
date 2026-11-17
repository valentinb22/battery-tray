//! Sonda HID de SOLO LECTURA para investigar protocolos propietarios.
//! Uso: cargo run --release --example probe -- <VID hex> [segundos de escucha]
//! Lista colecciones, capacidades, lee informes Feature por ID y escucha informes Input.
#![allow(dead_code)]
#[path = "../src/ffi.rs"]
mod ffi;

use core::ffi::c_void;
use ffi::*;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

const GUID_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

fn paths() -> Vec<Vec<u16>> {
    unsafe {
        let mut len = 0u32;
        CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_HID, null(), 0);
        let mut buf = vec![0u16; len as usize];
        CM_Get_Device_Interface_ListW(&GUID_HID, null(), buf.as_mut_ptr(), len, 0);
        buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(|s| s.iter().copied().chain(Some(0)).collect()).collect()
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let vid = u16::from_str_radix(args.get(1).map(String::as_str).unwrap_or("0951"), 16).unwrap();
    let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    unsafe {
        for p in paths() {
            let h0 = CreateFileW(p.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, 0, null_mut());
            if h0 == invalid_handle() {
                continue;
            }
            let _g0 = HandleGuard(h0);
            let mut a: HIDD_ATTRIBUTES = zeroed();
            a.size = 12;
            if HidD_GetAttributes(h0, &mut a) == 0 || a.vendor_id != vid {
                continue;
            }
            let mut pp: PREPARSED = 0;
            HidD_GetPreparsedData(h0, &mut pp);
            let mut c: HIDP_CAPS = zeroed();
            HidP_GetCaps(pp, &mut c);
            println!("\n=== {:04X}:{:04X} page {:04X} usage {:04X} in {} out {} feat {} ===", a.vendor_id, a.product_id, c.usage_page, c.usage, c.input_len, c.output_len, c.feature_len);
            println!("    {}", String::from_utf16_lossy(&p[..p.len() - 1]));
            for (name, ty, n) in [("IN", HIDP_INPUT, c.n_in_values), ("OUT", 1, c.n_out_values), ("FEAT", HIDP_FEATURE, c.n_feat_values)] {
                if n == 0 { continue; }
                let mut v: Vec<HIDP_VALUE_CAPS> = vec![zeroed(); n as usize];
                let mut m = n;
                HidP_GetValueCaps(ty, v.as_mut_ptr(), &mut m, pp);
                for vc in &v[..m as usize] {
                    println!("    {name} value: report {:02X} page {:04X} usage {:04X}..{:04X} bits {}x{} logical {}..{}", vc.report_id, vc.usage_page, vc.u[0], if vc.is_range != 0 { vc.u[1] } else { vc.u[0] }, vc.bit_size, vc.report_count, vc.logical_min, vc.logical_max);
                }
            }
            HidD_FreePreparsedData(pp);

            // Feature reports: solo lectura (GET_REPORT).
            let rw = CreateFileW(p.as_ptr(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED, null_mut());
            if rw == invalid_handle() {
                println!("    (no se puede abrir R/W: colección en uso exclusivo del sistema)");
                continue;
            }
            let _g = HandleGuard(rw);
            if c.feature_len > 0 {
                for id in 0..=255u8 {
                    let mut b = vec![0u8; c.feature_len as usize];
                    b[0] = id;
                    if HidD_GetFeature(h0, b.as_mut_ptr() as *mut c_void, b.len() as u32) != 0 {
                        println!("    FEATURE {id:02X}: {}", hex(&b));
                    }
                }
            }
            // GET_REPORT de informes Input (lectura, sin efectos).
            if c.input_len > 0 {
                for id in 0..=255u8 {
                    let mut b = vec![0u8; c.input_len as usize];
                    b[0] = id;
                    if HidD_GetInputReport(h0, b.as_mut_ptr() as *mut c_void, b.len() as u32) != 0 {
                        println!("    GET_INPUT {id:02X}: {}", hex(&b));
                    }
                }
            }
            // Escucha pasiva de informes Input.
            if c.input_len > 0 && secs > 0 {
                let ev = CreateEventW(null(), 1, 0, null());
                let t = std::time::Instant::now();
                let mut b = vec![0u8; c.input_len as usize];
                while t.elapsed().as_secs() < secs {
                    let mut ov: OVERLAPPED = zeroed();
                    ov.event = ev;
                    ResetEvent(ev);
                    ReadFile(rw, b.as_mut_ptr(), b.len() as u32, null_mut(), &mut ov);
                    let mut n = 0u32;
                    if WaitForSingleObject(ev, 500) != WAIT_OBJECT_0 {
                        CancelIo(rw);
                        GetOverlappedResult(rw, &ov, &mut n, 1);
                        continue;
                    }
                    GetOverlappedResult(rw, &ov, &mut n, 0);
                    println!("    INPUT +{:>5}ms: {}", t.elapsed().as_millis(), hex(&b[..n as usize]));
                }
                CloseHandle(ev);
            }
        }
    }
}
