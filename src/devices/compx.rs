//! Receptores 2.4 GHz Compx (3554:FA09: AULA, Redragon, MCHOSE, LeoBog…), colección 0xFF02.
//!
//! Protocolo (proyectos open source volt / omarchy-gadget-batteries / BatteryTray,
//! verificado en hardware real): informe 0x13 de 20 bytes en ambos sentidos, el byte 19 es
//! la suma de los bytes 0..19.
//!   Variante A: → 13 4A 00…  ← 13 4A 01 00 02 <nivel> <estado> …
//!   Variante B: → 13 05 01…  ← 13 0A 01 00 04 05 <nivel> <estado> …
//!   Identidad:  → 13 05 01…  ← 13 05 01 00 0A <código de modelo, 6 bytes> …
//!   estado 0x10 = cable conectado, 0x01 = batería.
//!
//! El teclado solo responde despierto. La última lectura se guarda en el registro
//! (HKCU\Software\BatteryTray\Cache) y se muestra, marcada como "dormido", hasta 24 h.

use super::hidpp::Channel;
use super::{Charge, Device, Kind};
use crate::config;
use crate::ffi::{GetLocalTime, SYSTEMTIME};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const VID: u16 = 0x3554;
const REPORT: u8 = 0x13;
const KEEP_WHILE_ASLEEP_SECS: u64 = 24 * 60 * 60;
const GENERIC_NAME: &str = "Teclado inalámbrico 2.4G";

/// Códigos de modelo conocidos (los no listados se muestran con nombre genérico).
const MODELS: [([u8; 6], &str); 2] = [
    ([0x03, 0x00, 0x00, 0x00, 0x00, 0xCD], "AULA F75"),
    ([0x03, 0x00, 0x00, 0x00, 0x00, 0xBA], "Redragon UCAL Pro"),
];

static NEED_RETRY: AtomicBool = AtomicBool::new(false);

pub fn needs_retry() -> bool {
    NEED_RETRY.load(Ordering::Relaxed)
}

/// Lectura persistida: "nivel;estado;modelo_hex;unix;HH:MM".
struct Saved {
    level: u8,
    state: u8,
    model: Option<[u8; 6]>,
    unix: u64,
    hhmm: String,
}

impl Saved {
    fn encode(&self) -> String {
        let model = self.model.map_or(String::new(), |m| m.iter().map(|b| format!("{b:02X}")).collect());
        format!("{};{};{};{};{}", self.level, self.state, model, self.unix, self.hhmm)
    }

    fn decode(s: &str) -> Option<Self> {
        let p: Vec<&str> = s.split(';').collect();
        if p.len() != 5 {
            return None;
        }
        let model = if p[2].len() == 12 {
            let mut m = [0u8; 6];
            for (i, b) in m.iter_mut().enumerate() {
                *b = u8::from_str_radix(&p[2][i * 2..i * 2 + 2], 16).ok()?;
            }
            Some(m)
        } else {
            None
        };
        Some(Self { level: p[0].parse().ok()?, state: p[1].parse().ok()?, model, unix: p[3].parse().ok()?, hhmm: p[4].to_string() })
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn now_hhmm() -> String {
    let mut t = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut t) };
    format!("{:02}:{:02}", t.hour, t.minute)
}

fn frame(cmd: u8, b2: u8) -> [u8; 20] {
    let mut f = [0u8; 20];
    f[0] = REPORT;
    f[1] = cmd;
    f[2] = b2;
    f[19] = checksum(&f);
    f
}

fn checksum(f: &[u8]) -> u8 {
    f[..19].iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

/// (nivel, estado) si el paquete es una respuesta de batería válida.
fn parse(r: &[u8]) -> Option<(u8, u8)> {
    if r.len() < 20 || r[0] != REPORT || checksum(r) != r[19] {
        return None;
    }
    let (level, state) = match r[1] & 0x7F {
        0x4A if r[2] == 0x01 && r[4] == 0x02 => (r[5], r[6]),
        0x0A if r[2..6] == [0x01, 0x00, 0x04, 0x05] => (r[6], r[7]),
        _ => return None,
    };
    (level <= 100).then_some((level, state))
}

/// Código de modelo de la respuesta de identidad.
fn parse_model(r: &[u8]) -> Option<[u8; 6]> {
    if r.len() < 20 || r[0] != REPORT || checksum(r) != r[19] || r[1..5] != [0x05, 0x01, 0x00, 0x0A] {
        return None;
    }
    r[5..11].try_into().ok()
}

fn model_name(code: Option<[u8; 6]>) -> String {
    code.and_then(|c| MODELS.iter().find(|(m, _)| *m == c).map(|(_, n)| n.to_string()))
        .unwrap_or_else(|| GENERIC_NAME.to_string())
}

fn charge_of(state: u8) -> Charge {
    match state {
        // Con cable el firmware no informa un nivel fiable: no afirmamos "lleno".
        0x10 => Charge::Charging,
        0x01 => Charge::Discharging,
        _ => Charge::Unknown,
    }
}

fn ask<T>(ch: &Channel, request: &[u8; 20], parse: impl Fn(&[u8]) -> Option<T>) -> Option<T> {
    let mut buf = vec![0u8; 20];
    // Descarta informes antiguos acumulados en la cola del handle.
    while ch.read(&mut buf, 0).is_some() {}
    let mut req = request.to_vec();
    ch.write(&mut req, 500)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(1000) {
        let left = 1000u32.saturating_sub(start.elapsed().as_millis() as u32).max(1);
        let n = ch.read(&mut buf, left)?;
        if let Some(v) = parse(&buf[..n]) {
            return Some(v);
        }
    }
    None
}

/// `key` identifica el receptor (ruta de su interfaz HID).
pub fn query(ch: &Channel, key: &str, out: &mut Vec<Device>) {
    let saved = config::cache_get(key).and_then(|s| Saved::decode(&s));
    let reading = ask(ch, &frame(0x4A, 0x00), parse).or_else(|| ask(ch, &frame(0x05, 0x01), parse));

    match reading {
        Some((level, state)) => {
            // El modelo se pide una sola vez (luego queda guardado con la lectura).
            let model = saved.as_ref().and_then(|s| s.model).or_else(|| ask(ch, &frame(0x05, 0x01), parse_model));
            let entry = Saved { level, state, model, unix: now_unix(), hhmm: now_hhmm() };
            config::cache_set(key, &entry.encode());
            NEED_RETRY.store(false, Ordering::Relaxed);
            out.push(Device {
                name: model_name(model),
                kind: Kind::Keyboard,
                level: Some(level),
                charge: charge_of(state),
                priority: 40,
                asleep: None,
            });
        }
        None => match saved {
            // Dormido: última lectura conocida, si es de las últimas 24 h.
            Some(s) if now_unix().saturating_sub(s.unix) < KEEP_WHILE_ASLEEP_SECS => {
                NEED_RETRY.store(false, Ordering::Relaxed);
                out.push(Device {
                    name: model_name(s.model),
                    kind: Kind::Keyboard,
                    level: Some(s.level),
                    charge: charge_of(s.state),
                    priority: 40,
                    asleep: Some(s.hhmm),
                });
            }
            _ => NEED_RETRY.store(true, Ordering::Relaxed),
        },
    }
}
