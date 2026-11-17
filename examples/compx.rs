//! Prueba de la consulta de batería Compx (3554:FA09): 13 4A … 5D por la colección FF02.
#![allow(dead_code)]
#[path = "../src/ffi.rs"]
mod ffi;

use ffi::*;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

const GUID_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

unsafe fn io(h: HANDLE, ev: HANDLE, write: bool, b: &mut [u8], ms: u32) -> Option<usize> {
    let mut ov: OVERLAPPED = zeroed();
    ov.event = ev;
    ResetEvent(ev);
    let ok = if write { WriteFile(h, b.as_ptr(), b.len() as u32, null_mut(), &mut ov) } else { ReadFile(h, b.as_mut_ptr(), b.len() as u32, null_mut(), &mut ov) };
    let mut n = 0;
    if ok == 0 {
        if GetLastError() != ERROR_IO_PENDING { println!("  err {}", GetLastError()); return None; }
        if WaitForSingleObject(ev, ms) != WAIT_OBJECT_0 { CancelIo(h); GetOverlappedResult(h, &ov, &mut n, 1); return None; }
    }
    if GetOverlappedResult(h, &ov, &mut n, 0) == 0 { println!("  err {}", GetLastError()); return None; }
    Some(n as usize)
}

fn main() {
    unsafe {
        let mut len = 0u32;
        CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_HID, null(), 0);
        let mut buf = vec![0u16; len as usize];
        CM_Get_Device_Interface_ListW(&GUID_HID, null(), buf.as_mut_ptr(), len, 0);
        for p in buf.split(|&c| c == 0).filter(|s| !s.is_empty()) {
            let p: Vec<u16> = p.iter().copied().chain(Some(0)).collect();
            let s = String::from_utf16_lossy(&p).to_lowercase();
            if !(s.contains("vid_3554&pid_fa09") && s.contains("col01")) { continue; }
            let h = CreateFileW(p.as_ptr(), GENERIC_READ | GENERIC_WRITE, 3, null(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED, null_mut());
            if h == invalid_handle() { println!("no se pudo abrir"); continue; }
            let ev = CreateEventW(null(), 1, 0, null());
            let variants = [(0x4Au8, 0x00u8), (0x4A, 0x01), (0x05, 0x01)];
            let start = std::time::Instant::now();
            for attempt in 0..usize::MAX {
                if start.elapsed().as_secs() > 45 { break; }
                let (cmd, b2) = variants[attempt % 3];
                let mut req = [0u8; 20];
                req[0] = 0x13; req[1] = cmd; req[2] = b2;
                req[19] = req[..19].iter().fold(0u8, |a, b| a.wrapping_add(*b));
                println!("intento {attempt}: write {:?}", io(h, ev, true, &mut req, 1000));
                let t = std::time::Instant::now();
                while t.elapsed().as_millis() < 1500 {
                    let mut r = [0u8; 20];
                    if let Some(n) = io(h, ev, false, &mut r, 500) {
                        println!("  resp: {}", r[..n].iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" "));
                    }
                }
            }
            CloseHandle(ev);
            CloseHandle(h);
        }
    }
}
