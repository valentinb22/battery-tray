//! Prueba aislada del protocolo Razer: envía "batería" (07 80) con cada ID de transacción
//! a las colecciones 1532 con informe Feature de 91 bytes y muestra el estado de la respuesta.
//! Uso: cargo run --release --example razer
#[path = "../src/ffi.rs"]
#[allow(dead_code)]
mod ffi;

use core::ffi::c_void;
use ffi::*;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

const GUID_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

fn main() {
    unsafe {
        let mut len = 0u32;
        CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_HID, null(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT);
        let mut buf = vec![0u16; len as usize];
        CM_Get_Device_Interface_ListW(&GUID_HID, null(), buf.as_mut_ptr(), len, CM_GET_DEVICE_INTERFACE_LIST_PRESENT);
        for p in buf.split(|&c| c == 0).filter(|s| !s.is_empty()) {
            let path: Vec<u16> = p.iter().copied().chain(Some(0)).collect();
            let h = CreateFileW(path.as_ptr(), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, null(), OPEN_EXISTING, 0, null_mut());
            if h == invalid_handle() {
                continue;
            }
            let _g = HandleGuard(h);
            let mut a: HIDD_ATTRIBUTES = zeroed();
            a.size = std::mem::size_of::<HIDD_ATTRIBUTES>() as u32;
            HidD_GetAttributes(h, &mut a);
            if a.vendor_id != 0x1532 {
                continue;
            }
            let mut pp = 0;
            HidD_GetPreparsedData(h, &mut pp);
            let mut caps: HIDP_CAPS = zeroed();
            HidP_GetCaps(pp, &mut caps);
            HidD_FreePreparsedData(pp);
            if caps.feature_len != 91 {
                continue;
            }
            println!("{}", String::from_utf16_lossy(p));
            for tid in [0x1Fu8, 0x3F, 0xFF] {
                for cmd in [0x80u8, 0x84] {
                    let mut req = [0u8; 91];
                    req[2] = tid;
                    req[6] = 2;
                    req[7] = 7;
                    req[8] = cmd;
                    req[89] = req[3..89].iter().fold(0, |x, y| x ^ y);
                    let t = Instant::now();
                    let ok = HidD_SetFeature(h, req.as_ptr() as *const c_void, 91);
                    std::thread::sleep(Duration::from_millis(25));
                    let mut r = [0u8; 91];
                    let ok2 = HidD_GetFeature(h, r.as_mut_ptr() as *mut c_void, 91);
                    println!(
                        "  tid {tid:02X} cmd 07 {cmd:02X}: set={ok} get={ok2} {:?} ms  resp {:02X?}",
                        t.elapsed().as_millis(),
                        &r[1..12]
                    );
                }
            }
        }
    }
}
