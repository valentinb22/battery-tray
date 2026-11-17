//! Enumeración HID: detecta colecciones Logitech HID++ y colecciones estándar con
//! "Battery Strength" (Usage Page 0x06 Generic Device Controls, Usage 0x20).

use super::{compx, hidpp, hyperx, Charge, Device, Kind};
use crate::ffi::*;
use core::ffi::c_void;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);
const VID_LOGITECH: u16 = 0x046D;
const PAGE_GENERIC_DEVICE: u16 = 0x06;
const USAGE_BATTERY_STRENGTH: u16 = 0x20;

pub fn collect(out: &mut Vec<Device>) {
    for path in interface_paths(&GUID_DEVINTERFACE_HID) {
        unsafe { probe(&path, out) };
    }
}

fn interface_paths(class: &GUID) -> Vec<Vec<u16>> {
    unsafe {
        for _ in 0..3 {
            let mut len = 0u32;
            if CM_Get_Device_Interface_List_SizeW(&mut len, class, null(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
                != CR_SUCCESS
                || len <= 1
            {
                return Vec::new();
            }
            let mut buf = vec![0u16; len as usize];
            match CM_Get_Device_Interface_ListW(class, null(), buf.as_mut_ptr(), len, CM_GET_DEVICE_INTERFACE_LIST_PRESENT) {
                CR_SUCCESS => {
                    return buf
                        .split(|&c| c == 0)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.iter().copied().chain(Some(0)).collect())
                        .collect();
                }
                CR_BUFFER_SMALL => continue,
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }
}

unsafe fn open(path: &[u16], access: u32, overlapped: bool) -> Option<HandleGuard> {
    let flags = if overlapped { FILE_FLAG_OVERLAPPED } else { 0 };
    let h = CreateFileW(
        path.as_ptr(),
        access,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        null(),
        OPEN_EXISTING,
        flags,
        null_mut(),
    );
    (h != invalid_handle() && !h.is_null()).then(|| HandleGuard(h))
}

/// Datos preparsed liberados al salir de ámbito.
struct Preparsed(PREPARSED);
impl Drop for Preparsed {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { HidD_FreePreparsedData(self.0) };
        }
    }
}

unsafe fn probe(path: &[u16], out: &mut Vec<Device>) {
    // Acceso 0: siempre permitido (incluso para teclados/ratones abiertos en exclusiva
    // por el sistema) y suficiente para atributos y descriptores.
    let Some(h0) = open(path, 0, false) else { return };

    let mut attrs: HIDD_ATTRIBUTES = zeroed();
    attrs.size = std::mem::size_of::<HIDD_ATTRIBUTES>() as u32;
    if HidD_GetAttributes(h0.0, &mut attrs) == 0 {
        return;
    }
    let mut pp = Preparsed(0);
    if HidD_GetPreparsedData(h0.0, &mut pp.0) == 0 {
        return;
    }
    let mut caps: HIDP_CAPS = zeroed();
    if HidP_GetCaps(pp.0, &mut caps) != HIDP_STATUS_SUCCESS {
        return;
    }

    // Mouse con cable: colección Generic Desktop / Mouse de un dispositivo USB directo.
    if caps.usage_page == 0x01 && caps.usage == 0x02 {
        if let Some(d) = wired_mouse(h0.0, path, attrs.vendor_id) {
            out.push(d);
            return;
        }
    }

    // Logitech HID++: colección "long" (0xFF00/0x0002 en receptores y USB,
    // 0xFF43/0x0202 en dispositivos Bluetooth).
    let is_hidpp = attrs.vendor_id == VID_LOGITECH
        && ((caps.usage_page == 0xFF00 && caps.usage == 0x0002) || (caps.usage_page == 0xFF43 && caps.usage == 0x0202));
    if is_hidpp {
        if let Some(h) = open(path, GENERIC_READ | GENERIC_WRITE, true) {
            if let Some(ch) = hidpp::Channel::new(h.0, caps.input_len as usize, caps.output_len as usize) {
                hidpp::query(&ch, out);
            }
        }
        return;
    }
    if attrs.vendor_id == VID_LOGITECH && (caps.usage_page == 0xFF00 || caps.usage_page == 0xFF43) {
        return; // colección "short" del mismo receptor: ya cubierta
    }

    // HyperX Cloud Stinger Core Wireless: Feature 0xFF en la colección vendor 0xFF00.
    if attrs.vendor_id == hyperx::VID && attrs.product_id == hyperx::PID {
        if caps.usage_page == 0xFF00 && caps.output_len > 0 {
            if let Some(h) = open(path, GENERIC_READ | GENERIC_WRITE, false) {
                hyperx::query(h.0, caps.feature_len as usize, out);
            }
        }
        return;
    }

    // Receptores Compx: informe 0x13 en la colección vendor 0xFF02.
    if attrs.vendor_id == compx::VID {
        if caps.usage_page == 0xFF02 {
            if let Some(h) = open(path, GENERIC_READ | GENERIC_WRITE, true) {
                if let Some(ch) = hidpp::Channel::new(h.0, caps.input_len as usize, caps.output_len as usize) {
                    compx::query(&ch, &String::from_utf16_lossy(path), out);
                }
            }
        }
        return;
    }

    let Some((report_type, vc)) = find_battery_cap(pp.0, &caps) else { return };
    let len = if report_type == HIDP_FEATURE { caps.feature_len } else { caps.input_len } as usize;
    if len == 0 {
        return;
    }

    let rw = open(path, GENERIC_READ | GENERIC_WRITE, false);
    let h = rw.as_ref().map_or(h0.0, |g| g.0);
    let mut buf = vec![0u8; len];
    buf[0] = vc.report_id;
    let ok = if report_type == HIDP_FEATURE {
        HidD_GetFeature(h, buf.as_mut_ptr() as *mut c_void, len as u32)
    } else {
        HidD_GetInputReport(h, buf.as_mut_ptr() as *mut c_void, len as u32)
    };
    if ok == 0 {
        return;
    }
    let mut value = 0u32;
    if HidP_GetUsageValue(
        report_type,
        PAGE_GENERIC_DEVICE,
        vc.link_collection,
        USAGE_BATTERY_STRENGTH,
        &mut value,
        pp.0,
        buf.as_ptr(),
        len as u32,
    ) != HIDP_STATUS_SUCCESS
    {
        return;
    }

    let (min, max) = (vc.logical_min as i64, vc.logical_max as i64);
    let level = if max > min {
        ((value as i64 - min) * 100 / (max - min)).clamp(0, 100) as u8
    } else if value <= 100 {
        value as u8
    } else {
        return;
    };

    let mut name_buf = [0u16; 128];
    let name = if HidD_GetProductString(h, name_buf.as_mut_ptr() as *mut c_void, 254) != 0 {
        from_wide(&name_buf).trim().to_string()
    } else {
        String::new()
    };
    let name = if name.is_empty() { format!("HID {:04X}:{:04X}", attrs.vendor_id, attrs.product_id) } else { name };

    let kind = match (caps.usage_page, caps.usage) {
        (0x01, 0x02) => Kind::Mouse,
        (0x01, 0x06) => Kind::Keyboard,
        (0x01, 0x04) | (0x01, 0x05) => Kind::Gamepad,
        _ => Kind::from_name(&name),
    };
    out.push(Device { name, kind, level: Some(level), charge: Charge::Unknown, priority: 20, asleep: None });
}

/// Un mouse USB que no es un receptor inalámbrico. Los mouse Bluetooth tienen rutas
/// `hid#{0000…}` (no `hid#vid_`) y se descartan; los receptores 2.4 GHz se reconocen por
/// fabricante o por su nombre de producto.
unsafe fn wired_mouse(h: HANDLE, path: &[u16], vid: u16) -> Option<Device> {
    let p = String::from_utf16_lossy(path).to_lowercase();
    if !p.starts_with("\\\\?\\hid#vid_") || vid == compx::VID {
        return None;
    }
    let mut buf = [0u16; 128];
    if HidD_GetProductString(h, buf.as_mut_ptr() as *mut c_void, 254) == 0 {
        return None;
    }
    let name = from_wide(&buf).trim().to_string();
    let lower = name.to_lowercase();
    let wireless = ["receiver", "receptor", "wireless", "dongle", "2.4g", "lightspeed", "unifying", "bolt", "hyperspeed", "slipstream"];
    if name.is_empty() || wireless.iter().any(|w| lower.contains(w)) {
        return None;
    }
    Some(Device { name, kind: Kind::Mouse, level: None, charge: Charge::Wired, priority: 5, asleep: None })
}

unsafe fn find_battery_cap(pp: PREPARSED, caps: &HIDP_CAPS) -> Option<(i32, HIDP_VALUE_CAPS)> {
    for (report_type, count) in [(HIDP_FEATURE, caps.n_feat_values), (HIDP_INPUT, caps.n_in_values)] {
        if count == 0 {
            continue;
        }
        let mut list: Vec<HIDP_VALUE_CAPS> = vec![zeroed(); count as usize];
        let mut n = count;
        if HidP_GetValueCaps(report_type, list.as_mut_ptr(), &mut n, pp) != HIDP_STATUS_SUCCESS {
            continue;
        }
        for vc in &list[..n as usize] {
            if vc.usage_page != PAGE_GENERIC_DEVICE {
                continue;
            }
            let hit = if vc.is_range != 0 {
                vc.u[0] <= USAGE_BATTERY_STRENGTH && USAGE_BATTERY_STRENGTH <= vc.u[1]
            } else {
                vc.u[0] == USAGE_BATTERY_STRENGTH
            };
            if hit {
                return Some((report_type, *vc));
            }
        }
    }
    None
}
