//! Batería que la pila Bluetooth de Windows publica como propiedad del nodo PnP
//! (la misma que muestra Configuración → Bluetooth). No despierta la radio.

use super::{Charge, Device, Kind};
use crate::ffi::*;
use std::ptr::{null, null_mut};

/// Nivel de batería Bluetooth (DEVPROP_TYPE_BYTE, 0-100).
const DEVPKEY_BLUETOOTH_BATTERY: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };
/// "Conectado" del dispositivo Bluetooth (DEVPROP_TYPE_BOOLEAN).
const DEVPKEY_DEVICE_CONNECTED: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0x83da6326_97a6_4088_9453_a1923f573b29), pid: 15 };
const DEVPKEY_DEVICE_FRIENDLY_NAME: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 14 };
const DEVPKEY_DEVICE_DESC: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 2 };
/// Class of Device Bluetooth clásico (DEVPROP_TYPE_UINT32).
const DEVPKEY_BLUETOOTH_COD: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0x2bd67d8b_8beb_48d5_87e0_6cda3428040a), pid: 10 };

/// Sufijos que Windows añade a los nodos de servicio de unos auriculares.
const SUFFIXES: [&str; 5] = [" Hands-Free AG", " Hands-Free", " Avrcp Transport", " Stereo", " LE"];

pub fn collect(out: &mut Vec<Device>) {
    unsafe {
        let Some(ids) = present_device_ids() else { return };
        for id in ids.split(|&c| c == 0).filter(|s| !s.is_empty()) {
            // Solo enumeradores Bluetooth (BTHENUM\, BTHLE\, BTHLEDEVICE\): evita consultar
            // cientos de nodos que nunca tendrán esta propiedad.
            if !(id.len() > 3 && id[0] == b'B' as u16 && id[1] == b'T' as u16 && id[2] == b'H' as u16) {
                continue;
            }
            let mut idz = id.to_vec();
            idz.push(0);
            let mut inst = 0u32;
            if CM_Locate_DevNodeW(&mut inst, idz.as_ptr(), 0) != CR_SUCCESS {
                continue;
            }
            let Some(level) = prop(inst, &DEVPKEY_BLUETOOTH_BATTERY, DEVPROP_TYPE_BYTE).and_then(|b| b.first().copied())
            else {
                continue;
            };
            if level > 100 {
                continue;
            }
            // Si Windows dice explícitamente que está desconectado, el valor es antiguo.
            if let Some(b) = prop(inst, &DEVPKEY_DEVICE_CONNECTED, DEVPROP_TYPE_BOOLEAN) {
                if b.first() == Some(&0) {
                    continue;
                }
            }
            let raw_name = prop_string(inst, &DEVPKEY_DEVICE_FRIENDLY_NAME)
                .or_else(|| prop_string(inst, &DEVPKEY_DEVICE_DESC))
                .unwrap_or_else(|| "Dispositivo Bluetooth".into());
            let name = clean_name(&raw_name);
            let kind = prop(inst, &DEVPKEY_BLUETOOTH_COD, DEVPROP_TYPE_UINT32)
                .filter(|b| b.len() >= 4)
                .map(|b| kind_from_cod(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
                .filter(|k| *k != Kind::Other)
                .unwrap_or_else(|| Kind::from_name(&name));
            out.push(Device { name, kind, level: Some(level), charge: Charge::Unknown, priority: 10, asleep: None });
        }
    }
}

unsafe fn present_device_ids() -> Option<Vec<u16>> {
    // La lista puede crecer entre las dos llamadas: reintenta.
    for _ in 0..3 {
        let mut len = 0u32;
        if CM_Get_Device_ID_List_SizeW(&mut len, null(), CM_GETIDLIST_FILTER_PRESENT) != CR_SUCCESS || len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        match CM_Get_Device_ID_ListW(null(), buf.as_mut_ptr(), len, CM_GETIDLIST_FILTER_PRESENT) {
            CR_SUCCESS => return Some(buf),
            CR_BUFFER_SMALL => continue,
            _ => return None,
        }
    }
    None
}

unsafe fn prop(inst: u32, key: &DEVPROPKEY, expected: u32) -> Option<Vec<u8>> {
    let mut ty = 0u32;
    let mut size = 0u32;
    let r = CM_Get_DevNode_PropertyW(inst, key, &mut ty, null_mut(), &mut size, 0);
    if r != CR_BUFFER_SMALL || size == 0 || ty != expected {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if CM_Get_DevNode_PropertyW(inst, key, &mut ty, buf.as_mut_ptr(), &mut size, 0) != CR_SUCCESS {
        return None;
    }
    buf.truncate(size as usize);
    Some(buf)
}

unsafe fn prop_string(inst: u32, key: &DEVPROPKEY) -> Option<String> {
    let b = prop(inst, key, DEVPROP_TYPE_STRING)?;
    let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let s = from_wide(&w);
    (!s.trim().is_empty()).then(|| s.trim().to_string())
}

fn clean_name(raw: &str) -> String {
    let mut s = raw.trim();
    for suf in SUFFIXES {
        if let Some(stripped) = s.strip_suffix(suf) {
            s = stripped.trim_end();
        }
    }
    s.to_string()
}

/// Bluetooth Class of Device → tipo. Mayor 0x04 = audio, 0x05 = periférico.
fn kind_from_cod(cod: u32) -> Kind {
    let major = (cod >> 8) & 0x1F;
    let minor = (cod >> 2) & 0x3F;
    match major {
        0x04 => Kind::Headset,
        0x05 => match (minor >> 4) & 0x3 {
            1 | 3 => Kind::Keyboard,
            2 => Kind::Mouse,
            _ if matches!(minor & 0xF, 1 | 2) => Kind::Gamepad,
            _ => Kind::Other,
        },
        _ => Kind::Other,
    }
}
