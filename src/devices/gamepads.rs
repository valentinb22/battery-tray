//! Mandos: Xbox (XInput, cualquier conexión) y PlayStation (DualShock 4 / DualSense,
//! nivel dentro del informe Input que el mando envía continuamente).

use super::hidpp::Channel;
use super::{Charge, Device, Kind};
use crate::ffi::*;
use std::time::Instant;

pub const VID_SONY: u16 = 0x054C;

/// Mandos Xbox conectados por XInput (receptor, Bluetooth o cable). Los de cable no tienen
/// batería y se omiten. XInput solo informa 4 niveles (vacía, baja, media, llena).
pub fn collect_xinput(out: &mut Vec<Device>) {
    let mut found = Vec::new();
    for user in 0..4u32 {
        let mut info = XINPUT_BATTERY_INFORMATION { battery_type: 0, battery_level: 0 };
        if unsafe { XInputGetBatteryInformation(user, BATTERY_DEVTYPE_GAMEPAD, &mut info) } != 0 {
            continue;
        }
        if !matches!(info.battery_type, BATTERY_TYPE_ALKALINE | BATTERY_TYPE_NIMH) {
            continue; // desconectado, con cable o desconocido
        }
        let level = match info.battery_level {
            0 => 5,
            1 => 25,
            2 => 60,
            _ => 100,
        };
        found.push(level);
    }
    let many = found.len() > 1;
    for (i, level) in found.into_iter().enumerate() {
        let name = if many { format!("Xbox Wireless Controller {}", i + 1) } else { "Xbox Wireless Controller".into() };
        out.push(Device { name, kind: Kind::Gamepad, level: Some(level), charge: Charge::Unknown, priority: 30, asleep: None });
    }
}

/// DualShock 4 y DualSense. `ch` debe tener acceso de lectura a la colección Gamepad.
pub fn query_playstation(ch: &Channel, pid: u16, in_len: usize, out: &mut Vec<Device>) -> bool {
    let (name, dualsense) = match pid {
        0x05C4 | 0x09CC | 0x0BA0 => ("DualShock 4", false),
        0x0CE6 => ("DualSense", true),
        0x0DF2 => ("DualSense Edge", true),
        _ => return false,
    };
    let mut buf = vec![0u8; in_len.max(80)];
    let start = Instant::now();
    while start.elapsed().as_millis() < 400 {
        let Some(n) = ch.read(&mut buf[..in_len], 200) else { return false };
        // Posición del byte de estado según el informe (USB 0x01, Bluetooth 0x11/0x31).
        let offset = match (dualsense, buf[0], n) {
            (false, 0x01, n) if n >= 64 => 30,
            (false, 0x11, n) if n >= 78 => 32,
            (true, 0x01, n) if n >= 64 => 53,
            (true, 0x31, n) if n >= 78 => 54,
            _ => continue, // informe reducido (Bluetooth sin modo completo) u otro
        };
        let status = buf[offset];
        let raw = status & 0x0F;
        let (level, charge) = if dualsense {
            match status >> 4 {
                0x0 => ((raw * 10 + 5).min(100), Charge::Discharging),
                0x1 => ((raw * 10 + 5).min(100), Charge::Charging),
                0x2 => (100, Charge::Full),
                _ => return false,
            }
        } else if status & 0x10 != 0 {
            // DS4 con cable: 0-10 cargando, 11 = completa.
            if raw < 10 { (raw * 10 + 5, Charge::Charging) } else { (100, Charge::Full) }
        } else {
            ((raw * 10 + 5).min(100), Charge::Discharging)
        };
        out.push(Device { name: name.into(), kind: Kind::Gamepad, level: Some(level), charge, priority: 40, asleep: None });
        return true;
    }
    false
}
