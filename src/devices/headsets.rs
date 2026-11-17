//! Auriculares inalámbricos con protocolo propio por informes Output/Input:
//! SteelSeries Arctis (1, 7, 7X/7P, 9, Pro, Nova 3P/5/7/7+/Pro), Corsair Void y
//! HyperX Cloud Flight / Cloud Alpha Wireless / Cloud II Wireless.
//!
//! Protocolos documentados por HeadsetControl (github.com/Sapd/HeadsetControl).
//! Reimplementados aquí a partir de esa documentación; no probados en hardware propio.
//!
//! Convención de índices (igual que hidapi): el byte 0 de la petición es el ID de informe;
//! en la respuesta, si el dispositivo no usa IDs de informe, Windows antepone un 0 que se quita.

use super::hidpp::Channel;
use super::Charge;
use std::time::Instant;

type Reading = Option<(u8, Charge)>;

pub struct Spec {
    pub vid: u16,
    pub pids: &'static [u16],
    pub name: &'static str,
    /// Colección (usage page, usage) que atiende el protocolo; `None` = cualquiera con informes Output.
    collection: Option<(u16, u16)>,
    /// Interfaz USB (`&mi_XX` en la ruta), si el protocolo la fija.
    iface: Option<u8>,
    read: fn(&Io, u16) -> Reading,
}

const STEELSERIES: u16 = 0x1038;
const CORSAIR: u16 = 0x1B1C;
const KINGSTON: u16 = 0x0951;
const HP: u16 = 0x03F0;

/// Arctis Nova 7 de primera generación: nivel en pasos 0-4 en vez de porcentaje.
const NOVA7_DISCRETE: [u16; 6] = [0x2202, 0x2206, 0x220A, 0x223A, 0x227A, 0x22A4];

static SPECS: &[Spec] = &[
    Spec {
        vid: STEELSERIES,
        pids: &[0x1260, 0x12AD, 0x1252, 0x1280],
        name: "SteelSeries Arctis 7",
        collection: Some((0xFF43, 0x0202)),
        iface: None,
        read: arctis7,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x12B3, 0x12B6, 0x12D7, 0x12D5],
        name: "SteelSeries Arctis 1 Wireless",
        collection: Some((0xFF43, 0x0202)),
        iface: None,
        read: arctis1,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x12C2, 0x12C4],
        name: "SteelSeries Arctis 9",
        collection: None,
        iface: Some(0),
        read: arctis9,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x2202, 0x22A1, 0x227E, 0x2206, 0x2258, 0x229E, 0x22AD, 0x223A, 0x22A9, 0x227A, 0x22A4, 0x22A5],
        name: "SteelSeries Arctis Nova 7",
        collection: Some((0xFFC0, 0x0001)),
        iface: None,
        read: nova7,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x2232, 0x2253],
        name: "SteelSeries Arctis Nova 5",
        collection: Some((0xFFC0, 0x0001)),
        iface: None,
        read: nova5,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x2269, 0x226D],
        name: "SteelSeries Arctis Nova 3P Wireless",
        collection: Some((0xFFC0, 0x0001)),
        iface: None,
        read: nova5,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x220E, 0x2212, 0x2216, 0x2236],
        name: "SteelSeries Arctis 7+",
        collection: Some((0xFFC0, 0x0001)),
        iface: None,
        read: arctis7_plus,
    },
    Spec {
        vid: STEELSERIES,
        pids: &[0x12E0, 0x12E5],
        name: "SteelSeries Arctis Nova Pro Wireless",
        collection: None,
        iface: Some(4),
        read: nova_pro,
    },
    Spec {
        vid: CORSAIR,
        pids: &[
            0x1B27, 0x0A14, 0x0A16, 0x0A17, 0x0A1D, 0x0A1A, 0x1B2A, 0x1B23, 0x1B29, 0x0A55, 0x0A51, 0x0A52,
            0x0A38, 0x0A4F, 0x0A2B, 0x0A75, 0x0A56,
        ],
        name: "Corsair Void Wireless",
        collection: Some((0xFFC5, 0x0001)),
        iface: None,
        read: corsair_void,
    },
    Spec {
        vid: KINGSTON,
        pids: &[0x16C4, 0x1723],
        name: "HyperX Cloud Flight Wireless",
        collection: None,
        iface: None,
        read: cloud_flight,
    },
    Spec {
        vid: HP,
        pids: &[0x098D],
        name: "HyperX Cloud Alpha Wireless",
        collection: None,
        iface: None,
        read: cloud_alpha_wireless,
    },
    Spec {
        vid: HP,
        pids: &[0x0696, 0x018B],
        name: "HyperX Cloud II Wireless",
        collection: Some((0xFF90, 0x0303)),
        iface: None,
        read: cloud2_wireless,
    },
];

pub fn find(vid: u16, pid: u16) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.vid == vid && s.pids.contains(&pid))
}

impl Spec {
    /// ¿Es esta colección (usage page/usage, ruta en minúsculas) la que habla el protocolo?
    pub fn wants(&self, page: u16, usage: u16, path_lower: &str) -> bool {
        if let Some(i) = self.iface {
            if !path_lower.contains(&format!("&mi_{i:02x}")) {
                return false;
            }
        }
        self.collection.map_or(true, |(p, u)| p == page && u == usage)
    }

    pub fn query(&self, ch: &Channel, out_len: usize, in_len: usize, pid: u16) -> Reading {
        (self.read)(&Io { ch, out_len, in_len }, pid)
    }
}

pub struct Io<'a> {
    ch: &'a Channel,
    out_len: usize,
    in_len: usize,
}

impl Io<'_> {
    /// Envía `cmd` (rellenado con ceros hasta el tamaño del informe Output) y devuelve el
    /// primer informe Input que cumpla `accept`. La respuesta se rellena con ceros hasta
    /// 160 bytes para que los índices fijos nunca se salgan.
    fn ask(&self, cmd: &[u8], accept: impl Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
        if cmd.len() > self.out_len || self.in_len == 0 {
            return None;
        }
        let mut buf = vec![0u8; self.in_len];
        // Descarta lo que ya estuviera encolado (informes espontáneos anteriores).
        for _ in 0..32 {
            if self.ch.read(&mut buf, 0).is_none() {
                break;
            }
        }
        let mut out = vec![0u8; self.out_len];
        out[..cmd.len()].copy_from_slice(cmd);
        self.ch.write(&mut out, 500)?;

        let start = Instant::now();
        loop {
            let elapsed = start.elapsed().as_millis() as u32;
            if elapsed >= 1500 {
                return None;
            }
            let n = self.ch.read(&mut buf, 1500 - elapsed)?;
            let r = if buf[0] == 0 { &buf[1..n.max(1)] } else { &buf[..n] };
            if accept(r) {
                let mut v = r.to_vec();
                v.resize(v.len().max(160), 0);
                return Some(v);
            }
        }
    }
}

fn any(_: &[u8]) -> bool {
    true
}

/// Escala lineal de `[lo, hi]` a 0-100.
fn scale(v: u8, lo: u8, hi: u8) -> u8 {
    if v <= lo {
        return 0;
    }
    (((v - lo) as u32 * 100) / (hi - lo) as u32).min(100) as u8
}

fn pct(v: u8) -> Option<u8> {
    (v <= 100).then_some(v)
}

fn arctis7(io: &Io, _: u16) -> Reading {
    let r = io.ask(&[0x06, 0x18], any)?;
    Some((pct(r[2])?, Charge::Unknown))
}

fn arctis1(io: &Io, _: u16) -> Reading {
    let r = io.ask(&[0x06, 0x12], any)?;
    if r[2] == 0x01 {
        return None; // auriculares apagados
    }
    Some((pct(r[3])?, Charge::Unknown))
}

fn arctis9(io: &Io, _: u16) -> Reading {
    let r = io.ask(&[0x00, 0x20], any)?;
    let charge = if r[4] == 0x01 { Charge::Charging } else { Charge::Discharging };
    Some((scale(r[3], 0x64, 0x9A), charge))
}

/// Estado de la familia Nova: → 00 B0, ← B0 …
fn nova_status(io: &Io) -> Option<Vec<u8>> {
    io.ask(&[0x00, 0xB0], |r| r.first() == Some(&0xB0))
}

fn nova7(io: &Io, pid: u16) -> Reading {
    let r = nova_status(io)?;
    if r[3] == 0x00 {
        return None;
    }
    let charge = if matches!(r[3], 0x01 | 0x02) { Charge::Charging } else { Charge::Discharging };
    let level = if NOVA7_DISCRETE.contains(&pid) { scale(r[2], 0, 4) } else { pct(r[2])? };
    Some((level, charge))
}

fn nova5(io: &Io, _: u16) -> Reading {
    let r = nova_status(io)?;
    if r[1] == 0x02 {
        return None;
    }
    let charge = if r[4] == 0x01 { Charge::Charging } else { Charge::Discharging };
    Some((pct(r[3])?, charge))
}

fn arctis7_plus(io: &Io, _: u16) -> Reading {
    let r = nova_status(io)?;
    if r[1] == 0x01 {
        return None;
    }
    let charge = if r[3] == 0x01 { Charge::Charging } else { Charge::Discharging };
    Some((scale(r[2], 0, 4), charge))
}

fn nova_pro(io: &Io, _: u16) -> Reading {
    let r = io.ask(&[0x06, 0xB0], |r| r.len() >= 16)?;
    let charge = match r[15] {
        0x01 => return None,
        0x02 => Charge::Charging,
        _ => Charge::Discharging,
    };
    Some((scale(r[6], 0, 8), charge))
}

fn corsair_void(io: &Io, _: u16) -> Reading {
    let r = io.ask(&[0xC9, 0x64], any)?;
    let charge = match r[4] {
        1 | 2 => Charge::Discharging,
        4 | 5 => Charge::Charging,
        _ => return None, // 0 = auriculares desconectados del receptor
    };
    Some((pct(r[2] & 0x7F)?, charge)) // bit 7 = micrófono levantado
}

fn cloud_flight(io: &Io, _: u16) -> Reading {
    let r = io.ask(&[0x21, 0xFF, 0x05], |r| matches!(r.len(), 15 | 20))?;
    let mv = u16::from_be_bytes([r[3], r[4]]);
    if mv == 0 {
        return None;
    }
    let charge = if mv > 0x100B { Charge::Charging } else { Charge::Discharging };
    Some((flight_percent(mv), charge))
}

/// Curva de HeadsetControl para la Cloud Flight (derivada de la de Logitech G633).
fn flight_percent(mv: u16) -> u8 {
    if mv <= 3648 {
        return (mv as f64 * 0.00125) as u8;
    }
    if mv > 3975 {
        return 100;
    }
    let v = mv as f64;
    let p = 0.00000002547505 * v.powi(4) - 0.0003900299 * v.powi(3) + 2.238321 * v.powi(2) - 5706.256 * v
        + 5452299.0;
    p.round().clamp(0.0, 100.0) as u8
}

fn cloud_alpha_wireless(io: &Io, _: u16) -> Reading {
    let conn = io.ask(&[0x21, 0xBB, 0x03], any)?;
    if conn[3] == 0x01 {
        return None;
    }
    let chg = io.ask(&[0x21, 0xBB, 0x0C], any)?;
    let charge = if chg[3] == 0x01 { Charge::Charging } else { Charge::Discharging };
    let bat = io.ask(&[0x21, 0xBB, 0x0B], any)?;
    Some((pct(bat[3])?, charge))
}

fn cloud2_wireless(io: &Io, _: u16) -> Reading {
    let cmd = |c: u8| io.ask(&[0x06, 0xFF, 0xBB, c], move |r| r.len() >= 8 && r[..4] == [0x06, 0xFF, 0xBB, c]);
    let level = pct(cmd(0x02)?[7])?;
    let charge = match cmd(0x03) {
        Some(r) if r[4] == 1 => Charge::Charging,
        Some(_) => Charge::Discharging,
        None => Charge::Unknown,
    };
    Some((level, charge))
}
