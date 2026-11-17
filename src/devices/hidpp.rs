//! Protocolo Logitech HID++ 2.0 (receptores Unifying / Bolt / Lightspeed, USB directo
//! y Bluetooth). Implementa solo lo necesario para leer nombre, tipo y batería.
//!
//! Informe "long" (20 bytes): [0x11, índice_dispositivo, índice_feature, función<<4 | sw_id, parámetros…]

use super::{Charge, Device, Kind};
use crate::ffi::*;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::time::Instant;

const REPORT_LONG: u8 = 0x11;
const SW_ID: u8 = 0x0B;
const ROOT: u8 = 0x00;
const F_DEVICE_NAME: u16 = 0x0005;
const F_BATTERY_STATUS: u16 = 0x1000;
const F_BATTERY_VOLTAGE: u16 = 0x1001;
const F_UNIFIED_BATTERY: u16 = 0x1004;
/// Medida ADC de tensión: auriculares Logitech G (G533, G633, G733, G933, G935, G PRO…).
const F_ADC_MEASUREMENT: u16 = 0x1F20;

pub struct Channel {
    h: HANDLE,
    event: HANDLE,
    in_len: usize,
    out_len: usize,
}

impl Channel {
    /// `h` debe estar abierto con FILE_FLAG_OVERLAPPED; su cierre es responsabilidad del llamador.
    pub unsafe fn new(h: HANDLE, in_len: usize, out_len: usize) -> Option<Self> {
        if in_len < 20 || out_len < 20 {
            return None;
        }
        Self::any(h, in_len, out_len)
    }

    /// Igual que `new`, sin exigir el tamaño de informe de HID++ (otros protocolos).
    pub unsafe fn any(h: HANDLE, in_len: usize, out_len: usize) -> Option<Self> {
        if in_len == 0 {
            return None;
        }
        let event = CreateEventW(null(), 1, 0, null());
        (!event.is_null()).then_some(Self { h, event, in_len, out_len })
    }

    /// E/S con tiempo límite. Si vence, cancela y espera a que el kernel suelte el buffer.
    unsafe fn transfer(&self, write: bool, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
        let mut ov: OVERLAPPED = zeroed();
        ov.event = self.event;
        ResetEvent(self.event);
        let ok = if write {
            WriteFile(self.h, buf.as_ptr(), buf.len() as u32, null_mut(), &mut ov)
        } else {
            ReadFile(self.h, buf.as_mut_ptr(), buf.len() as u32, null_mut(), &mut ov)
        };
        let mut n = 0u32;
        if ok == 0 {
            if GetLastError() != ERROR_IO_PENDING {
                return None;
            }
            if WaitForSingleObject(self.event, timeout_ms) != WAIT_OBJECT_0 {
                CancelIo(self.h);
                GetOverlappedResult(self.h, &ov, &mut n, 1);
                return None;
            }
        }
        if GetOverlappedResult(self.h, &ov, &mut n, 0) == 0 {
            return None;
        }
        Some(n as usize)
    }

    /// Lectura de un informe Input con tiempo límite (0 = solo lo ya encolado).
    pub fn read(&self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
        unsafe { self.transfer(false, buf, timeout_ms) }
    }

    /// Escritura de un informe Output con tiempo límite.
    pub fn write(&self, buf: &mut [u8], timeout_ms: u32) -> Option<usize> {
        unsafe { self.transfer(true, buf, timeout_ms) }
    }

    fn request(&self, dev: u8, feature: u8, function: u8, params: &[u8], timeout_ms: u32) -> Option<[u8; 16]> {
        let fn_sw = (function << 4) | SW_ID;
        let mut out = vec![0u8; self.out_len];
        out[0] = REPORT_LONG;
        out[1] = dev;
        out[2] = feature;
        out[3] = fn_sw;
        for (i, p) in params.iter().take(16).enumerate() {
            out[4 + i] = *p;
        }
        unsafe { self.transfer(true, &mut out, 500)? };

        let start = Instant::now();
        let mut inbuf = vec![0u8; self.in_len];
        loop {
            let elapsed = start.elapsed().as_millis() as u32;
            if elapsed >= timeout_ms {
                return None;
            }
            let n = unsafe { self.transfer(false, &mut inbuf, timeout_ms - elapsed)? };
            if n < 20 || inbuf[0] != REPORT_LONG || inbuf[1] != dev {
                continue; // notificación no relacionada (movimiento, otro dispositivo…)
            }
            if inbuf[2] == 0xFF && inbuf[3] == feature && inbuf[4] == fn_sw {
                return None; // error HID++ 2.0
            }
            if inbuf[2] == feature && inbuf[3] == fn_sw {
                let mut r = [0u8; 16];
                r.copy_from_slice(&inbuf[4..20]);
                return Some(r);
            }
        }
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.event) };
    }
}

/// Dispositivo directo (índice 0xFF) o hasta 6 emparejados a un receptor (1..=6).
pub fn query(ch: &Channel, out: &mut Vec<Device>) {
    if let Some(d) = query_index(ch, 0xFF) {
        out.push(d);
        return;
    }
    for idx in 1..=6u8 {
        if let Some(d) = query_index(ch, idx) {
            out.push(d);
        }
    }
}

fn query_index(ch: &Channel, idx: u8) -> Option<Device> {
    // Ping (Root.GetProtocolVersion). Dispositivo apagado/ausente ⇒ sin respuesta o error.
    let ping = ch.request(idx, ROOT, 1, &[0, 0, 0xAA], 300)?;
    if ping[0] < 2 {
        return None; // HID++ 1.0: sin features de batería estándar
    }
    let feature = |id: u16| -> Option<u8> {
        let r = ch.request(idx, ROOT, 0, &[(id >> 8) as u8, id as u8], 1000)?;
        (r[0] != 0).then_some(r[0])
    };
    let (level, charge) = read_battery(ch, idx, &feature)?;
    let (name, kind) = read_identity(ch, idx, &feature);
    Some(Device { name, kind, level: Some(level), charge, priority: 40, asleep: None })
}

fn read_battery(ch: &Channel, idx: u8, feature: &dyn Fn(u16) -> Option<u8>) -> Option<(u8, Charge)> {
    // 0x1004 UNIFIED_BATTERY.get_status: [%, nivel, estado_carga, alimentación_externa]
    if let Some(fi) = feature(F_UNIFIED_BATTERY) {
        if let Some(r) = ch.request(idx, fi, 1, &[], 1000) {
            let level = if r[0] == 0 && r[1] != 0 {
                // Dispositivos que solo informan niveles discretos.
                match r[1] {
                    l if l & 8 != 0 => 100,
                    l if l & 4 != 0 => 60,
                    l if l & 2 != 0 => 20,
                    _ => 5,
                }
            } else {
                r[0].min(100)
            };
            let charge = match r[2] {
                0 => Charge::Discharging,
                1 | 2 => Charge::Charging,
                3 => Charge::Full,
                _ => Charge::Unknown,
            };
            return Some((level, charge));
        }
    }
    // 0x1000 BATTERY_STATUS.GetBatteryLevelStatus: [%, siguiente_nivel, estado]
    if let Some(fi) = feature(F_BATTERY_STATUS) {
        if let Some(r) = ch.request(idx, fi, 0, &[], 1000) {
            if r[0] <= 100 {
                let charge = match r[2] {
                    0 => Charge::Discharging,
                    1 | 2 | 4 => Charge::Charging,
                    3 => Charge::Full,
                    _ => Charge::Unknown,
                };
                return Some((r[0], charge));
            }
        }
    }
    // 0x1001 BATTERY_VOLTAGE: [mV alto, mV bajo, flags(bit7 = cargando)]
    if let Some(fi) = feature(F_BATTERY_VOLTAGE) {
        if let Some(r) = ch.request(idx, fi, 0, &[], 1000) {
            let mv = u16::from_be_bytes([r[0], r[1]]);
            if mv > 0 {
                let charge = if r[2] & 0x80 != 0 { Charge::Charging } else { Charge::Discharging };
                return Some((voltage_to_percent(mv), charge));
            }
        }
    }
    // 0x1F20 ADC_MEASUREMENT: [mV alto, mV bajo, estado(3 = cargando)]
    if let Some(fi) = feature(F_ADC_MEASUREMENT) {
        if let Some(r) = ch.request(idx, fi, 0, &[], 1000) {
            let mv = u16::from_be_bytes([r[0], r[1]]);
            if mv >= HEADSET_CURVE[HEADSET_CURVE.len() - 1].0 {
                let charge = if r[2] == 3 { Charge::Charging } else { Charge::Discharging };
                return Some((interpolate(mv, &HEADSET_CURVE), charge));
            }
        }
    }
    None
}

/// Calibración de HeadsetControl para los auriculares Logitech G633/G733/G933/G935.
const HEADSET_CURVE: [(u16, u8); 8] =
    [(4100, 100), (3950, 80), (3850, 60), (3750, 40), (3650, 20), (3500, 10), (3300, 5), (3150, 0)];

fn read_identity(ch: &Channel, idx: u8, feature: &dyn Fn(u16) -> Option<u8>) -> (String, Kind) {
    let fallback = || ("Dispositivo Logitech".to_string(), Kind::Other);
    let Some(fi) = feature(F_DEVICE_NAME) else { return fallback() };

    let mut bytes: Vec<u8> = Vec::new();
    if let Some(r) = ch.request(idx, fi, 0, &[], 1000) {
        let count = (r[0] as usize).min(64);
        while bytes.len() < count {
            let Some(chunk) = ch.request(idx, fi, 1, &[bytes.len() as u8], 1000) else { break };
            let want = (count - bytes.len()).min(16);
            let part: Vec<u8> = chunk[..want].iter().copied().take_while(|&b| b != 0).collect();
            let complete = part.len() == want;
            bytes.extend(part);
            if !complete {
                break;
            }
        }
    }
    let name = String::from_utf8_lossy(&bytes).trim().to_string();
    if name.is_empty() {
        return fallback();
    }
    let kind = match ch.request(idx, fi, 2, &[], 1000).map(|r| r[0]) {
        Some(0) | Some(2) => Kind::Keyboard,
        Some(3) | Some(4) | Some(5) => Kind::Mouse,
        Some(8) => Kind::Headset,
        Some(11) | Some(12) => Kind::Gamepad,
        _ => Kind::from_name(&name),
    };
    (name, kind)
}

/// Curva de descarga típica de una celda Li-Po de 1S.
fn voltage_to_percent(mv: u16) -> u8 {
    interpolate(mv, &CURVE)
}

const CURVE: [(u16, u8); 11] = [
    (4186, 100),
    (4067, 90),
    (3989, 80),
    (3922, 70),
    (3859, 60),
    (3811, 50),
    (3778, 40),
    (3751, 30),
    (3717, 20),
    (3671, 10),
    (3500, 0),
    ];

/// Interpolación lineal en una curva (mV, %) ordenada de mayor a menor tensión.
fn interpolate(mv: u16, curve: &[(u16, u8)]) -> u8 {
    if mv >= curve[0].0 {
        return 100;
    }
    for w in curve.windows(2) {
        let ((v_hi, p_hi), (v_lo, p_lo)) = (w[0], w[1]);
        if mv >= v_lo {
            let span = (v_hi - v_lo) as u32;
            let pos = (mv - v_lo) as u32;
            return p_lo + ((pos * (p_hi - p_lo) as u32) / span) as u8;
        }
    }
    0
}
