//! HyperX Cloud Stinger Core Wireless (+ 7.1), receptor 0951:170B, colección MI_03 Col03.
//!
//! Protocolo (documentado por los proyectos open source nogenuity y HyperBatteryMonitor,
//! verificado en hardware real): SET_FEATURE 0xFF con un comando y GET_FEATURE 0xFF en
//! sondeo hasta recibir una respuesta "fresca" (byte 3 = 0xFE, byte 2 ≠ 0, eco del comando).
//!   Conexión → byte 11: 1 = auriculares encendidos.
//!   Batería  → byte 9: 3 descargando, 5 cargando, 6 lleno; bytes 11-12: mV (little-endian).

use super::{Charge, Device, Kind};
use crate::ffi::*;
use core::ffi::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const VID: u16 = 0x0951;
pub const PID: u16 = 0x170B;

const CMD_CONNECTION: [u8; 18] =
    [0xFF, 0x07, 0x00, 0xFD, 0x04, 0x00, 0x00, 0x02, 0x82, 0x03, 0x05, 0x29, 0x0F, 0x00, 0x00, 0x03, 0x68, 0x10];
const CMD_BATTERY: [u8; 12] = [0xFF, 0x07, 0x00, 0xFD, 0x04, 0x0C, 0xF1, 0x02, 0x01, 0x04, 0xF0, 0x0C];

/// `h`: handle R/W síncrono a la colección vendor 0xFF00; `feature_len` de HidP_GetCaps.
pub unsafe fn query(h: HANDLE, feature_len: usize, out: &mut Vec<Device>) {
    if feature_len < 20 {
        return;
    }
    let Some(conn) = exchange(h, feature_len, &CMD_CONNECTION) else { return };
    if conn[11] != 1 {
        return; // auriculares apagados o fuera de alcance
    }
    // Varias muestras (~30 ms cada una): mediana de la tensión y estado mayoritario,
    // para que el ruido de la medida no haga saltar el porcentaje.
    let mut mvs = Vec::with_capacity(SAMPLES);
    let mut states = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        if let Some(r) = exchange(h, feature_len, &CMD_BATTERY) {
            if matches!(r[9], 3 | 5 | 6) {
                mvs.push(u16::from_le_bytes([r[11], r[12]]));
                states.push(r[9]);
            }
        }
    }
    if mvs.is_empty() {
        return;
    }
    mvs.sort_unstable();
    let mv = mvs[mvs.len() / 2];
    let state = [3u8, 5, 6].into_iter().max_by_key(|s| states.iter().filter(|x| *x == s).count()).unwrap_or(3);
    let (raw, charge) = match state {
        5 => (percent(mv, &CHARGING), Charge::Charging),
        6 => (100, Charge::Full),
        _ => (percent(mv, &DISCHARGING), Charge::Discharging),
    };
    let level = stabilize(raw, charge);
    out.push(Device {
        name: "HyperX Cloud Stinger Core Wireless".into(),
        kind: Kind::Headset,
        level: Some(level),
        charge,
        priority: 40,
        asleep: None,
    });
}

const SAMPLES: usize = 5;
/// Salto máximo "contra corriente" antes de aceptar el valor nuevo (p. ej. tras un error).
const MAX_BACKTRACK: u8 = 15;

static PREVIOUS: Mutex<Option<(Charge, u8)>> = Mutex::new(None);

/// Mientras el estado no cambia, el porcentaje no sube descargando ni baja cargando.
fn stabilize(raw: u8, charge: Charge) -> u8 {
    let mut prev = PREVIOUS.lock().unwrap_or_else(|e| e.into_inner());
    let level = match *prev {
        Some((c, p)) if c == charge && charge == Charge::Discharging && raw > p && raw - p <= MAX_BACKTRACK => p,
        Some((c, p)) if c == charge && charge == Charge::Charging && raw < p && p - raw <= MAX_BACKTRACK => p,
        _ => raw,
    };
    *prev = Some((charge, level));
    level
}

unsafe fn exchange(h: HANDLE, len: usize, cmd: &[u8]) -> Option<Vec<u8>> {
    let mut req = vec![0u8; len];
    req[..cmd.len()].copy_from_slice(cmd);
    if HidD_SetFeature(h, req.as_ptr() as *const c_void, len as u32) == 0 {
        return None;
    }
    // La respuesta llega en ~3-40 ms. Un GET demasiado pronto devuelve la anterior.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(1000) {
        std::thread::sleep(Duration::from_millis(5));
        let mut r = vec![0u8; len];
        r[0] = 0xFF;
        if HidD_GetFeature(h, r.as_mut_ptr() as *mut c_void, len as u32) != 0
            && r[3] == 0xFE
            && r[2] != 0
            && r[4] == cmd[5]
            && r[5] == cmd[4]
        {
            return Some(r);
        }
    }
    None
}

/// Curvas Li-Po 1S (mV → %). Con carga, la tensión medida sube ~0,1 V.
const DISCHARGING: [(u16, u8); 11] = [
    (4120, 100), (4050, 90), (3980, 80), (3920, 70), (3870, 60), (3830, 50),
    (3800, 40), (3770, 30), (3740, 20), (3690, 10), (3500, 0),
];
const CHARGING: [(u16, u8); 11] = [
    (4130, 100), (4080, 90), (4030, 80), (3990, 70), (3950, 60), (3910, 50),
    (3870, 40), (3820, 30), (3760, 20), (3650, 10), (3400, 0),
];

pub fn percent(mv: u16, curve: &[(u16, u8)]) -> u8 {
    if mv >= curve[0].0 {
        return 100;
    }
    for w in curve.windows(2) {
        let ((v_hi, p_hi), (v_lo, p_lo)) = (w[0], w[1]);
        if mv >= v_lo {
            let pos = (mv - v_lo) as u32 * (p_hi - p_lo) as u32;
            return p_lo + (pos / (v_hi - v_lo) as u32) as u8;
        }
    }
    0
}
