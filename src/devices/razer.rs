//! Periféricos inalámbricos Razer (VID 1532): mouse, teclados y sus dongles HyperSpeed.
//!
//! Protocolo (documentado por OpenRazer): informe Feature 0 de 90 bytes en ambos sentidos.
//!   [estado, id_transacción, paquetes_restantes(2), protocolo, tamaño, clase, comando,
//!    argumentos(80), crc = XOR de los bytes 2..88, reservado]
//!   Batería: clase 0x07, comando 0x80 → argumento 1 = nivel 0-255.
//!   Carga:   clase 0x07, comando 0x84 → argumento 1 = 1 si está cargando.
//! El ID de transacción depende del modelo (0x1F en los recientes, 0x3F, 0xFF en los antiguos):
//! se prueban por orden y se recuerda el que respondió.

use super::{Charge, Device, Kind};
use crate::ffi::*;
use core::ffi::c_void;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

pub const VID: u16 = 0x1532;
/// Tamaño del informe Feature (ID de informe 0 + 90 bytes).
pub const FEATURE_LEN: usize = 91;

const TRANSACTION_IDS: [u8; 3] = [0x1F, 0x3F, 0xFF];
const STATUS_BUSY: u8 = 0x01;
const STATUS_OK: u8 = 0x02;

/// Por ruta de colección: ID de transacción que funcionó, o `None` si el dispositivo
/// contestó que no tiene batería (con cable): no se le vuelve a preguntar.
static KNOWN: Mutex<Option<HashMap<String, Option<u8>>>> = Mutex::new(None);

enum Answer {
    Value(u8),
    /// El dispositivo respondió, pero no admite el comando (p. ej. mouse con cable).
    Unsupported,
    /// Sin respuesta útil (ID de transacción incorrecto, dispositivo apagado…).
    Silent,
}

unsafe fn command(h: HANDLE, tid: u8, cmd: u8) -> Answer {
    let mut req = [0u8; FEATURE_LEN];
    {
        let r = &mut req[1..];
        r[1] = tid;
        r[5] = 0x02; // tamaño de datos
        r[6] = 0x07; // clase: misc / energía
        r[7] = cmd;
        r[88] = r[2..88].iter().fold(0, |a, b| a ^ b);
    }
    if HidD_SetFeature(h, req.as_ptr() as *const c_void, FEATURE_LEN as u32) == 0 {
        return Answer::Silent;
    }
    // Los dispositivos inalámbricos tardan unos ms en reenviar la petición por radio.
    for _ in 0..12 {
        std::thread::sleep(Duration::from_millis(25));
        let mut resp = [0u8; FEATURE_LEN];
        if HidD_GetFeature(h, resp.as_mut_ptr() as *mut c_void, FEATURE_LEN as u32) == 0 {
            return Answer::Silent;
        }
        let r = &resp[1..];
        if r[0] == STATUS_BUSY {
            continue;
        }
        if r[1] != tid || r[6] != 0x07 || r[7] != cmd {
            return Answer::Silent;
        }
        return match r[0] {
            STATUS_OK => Answer::Value(r[9]),
            0x05 => Answer::Unsupported,
            _ => Answer::Silent, // 0x03 error, 0x04 sin respuesta del mouse (apagado)
        };
    }
    Answer::Silent
}

/// `h`: handle (acceso 0 basta) a una colección Razer con informe Feature de 91 bytes.
pub unsafe fn query(h: HANDLE, path: &str, usage: (u16, u16), out: &mut Vec<Device>) -> bool {
    let mut known = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
    let known = known.get_or_insert_with(HashMap::new);
    let tids: Vec<u8> = match known.get(path) {
        Some(None) => return false,
        Some(Some(t)) => vec![*t],
        None => TRANSACTION_IDS.to_vec(),
    };

    for tid in tids {
        let raw = match command(h, tid, 0x80) {
            Answer::Value(v) => v,
            Answer::Unsupported => {
                known.insert(path.to_string(), None);
                return false;
            }
            Answer::Silent => continue,
        };
        let charging = matches!(command(h, tid, 0x84), Answer::Value(1));
        if raw == 0 && !charging {
            // Nivel 0 sin cargar: o no tiene batería, o el mouse está apagado. No se muestra.
            return false;
        }
        known.insert(path.to_string(), Some(tid));
        let level = ((raw as u32 * 100 + 127) / 255) as u8;
        let name = product_name(h);
        let kind = match usage {
            (0x01, 0x02) => Kind::Mouse,
            (0x01, 0x06) => Kind::Keyboard,
            _ => Kind::from_name(&name),
        };
        let charge = if charging { Charge::Charging } else { Charge::Discharging };
        out.push(Device { name, kind, level: Some(level), charge, priority: 40, asleep: None });
        return true;
    }
    false
}

unsafe fn product_name(h: HANDLE) -> String {
    let mut buf = [0u16; 128];
    let mut name = if HidD_GetProductString(h, buf.as_mut_ptr() as *mut c_void, 254) != 0 {
        from_wide(&buf).trim().to_string()
    } else {
        String::new()
    };
    for suffix in [" Dongle", " Receiver", " (Wireless)", " (Wired)", " Wireless Receiver"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            name = stripped.trim().to_string();
        }
    }
    if name.is_empty() {
        "Razer inalámbrico".into()
    } else if name.to_lowercase().starts_with("razer") {
        name
    } else {
        format!("Razer {name}")
    }
}
