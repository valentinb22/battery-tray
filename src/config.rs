//! Preferencias en el registro (HKCU) y arranque automático, sin dependencias.

use crate::ffi::*;
use core::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

pub const APP_KEY: &str = "Software\\BatteryTray";
pub const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub const RUN_VALUE: &str = "BatteryTray";
const THEME_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

pub const DEFAULT_POLL_MINUTES: u32 = 5;
pub const DEFAULT_LOW_THRESHOLD: u8 = 15;

fn get_dword(subkey: &str, value: &str) -> Option<u32> {
    let (k, v) = (wide(subkey), wide(value));
    let mut data = 0u32;
    let mut len = 4u32;
    let r = unsafe {
        RegGetValueW(
            hkey_current_user(),
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut data as *mut u32 as *mut c_void,
            &mut len,
        )
    };
    (r == 0).then_some(data)
}

pub fn set_dword(subkey: &str, value: &str, data: u32) {
    let (k, v) = (wide(subkey), wide(value));
    unsafe {
        RegSetKeyValueW(
            hkey_current_user(),
            k.as_ptr(),
            v.as_ptr(),
            REG_DWORD,
            &data as *const u32 as *const c_void,
            4,
        );
    }
}

pub fn get_string(subkey: &str, value: &str) -> Option<String> {
    let (k, v) = (wide(subkey), wide(value));
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            hkey_current_user(),
            k.as_ptr(),
            v.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        )
    };
    (r == 0).then(|| from_wide(&buf))
}

pub fn set_string(subkey: &str, value: &str, data: &str) {
    let (k, v, d) = (wide(subkey), wide(value), wide(data));
    unsafe {
        RegSetKeyValueW(
            hkey_current_user(),
            k.as_ptr(),
            v.as_ptr(),
            REG_SZ,
            d.as_ptr() as *const c_void,
            (d.len() * 2) as u32,
        );
    }
}

/// Borra una clave de HKCU con todo su contenido.
pub fn delete_tree(subkey: &str) {
    let k = wide(subkey);
    unsafe { RegDeleteTreeW(hkey_current_user(), k.as_ptr()) };
}

const CACHE_KEY: &str = "Software\\BatteryTray\\Cache";

/// Última lectura guardada de un dispositivo (sobrevive a reinicios de la app).
pub fn cache_get(name: &str) -> Option<String> {
    get_string(CACHE_KEY, name)
}

pub fn cache_set(name: &str, data: &str) {
    let (k, v, d) = (wide(CACHE_KEY), wide(name), wide(data));
    unsafe {
        RegSetKeyValueW(
            hkey_current_user(),
            k.as_ptr(),
            v.as_ptr(),
            REG_SZ,
            d.as_ptr() as *const c_void,
            (d.len() * 2) as u32,
        );
    }
}

/// Intervalo de consulta configurable (HKCU\Software\BatteryTray\PollMinutes).
pub fn poll_minutes() -> u32 {
    get_dword(APP_KEY, "PollMinutes").unwrap_or(DEFAULT_POLL_MINUTES).clamp(1, 120)
}

pub fn set_poll_minutes(m: u32) {
    set_dword(APP_KEY, "PollMinutes", m);
}

/// Umbral de alerta (HKCU\Software\BatteryTray\LowThreshold), 15 % por defecto.
pub fn low_threshold() -> u8 {
    get_dword(APP_KEY, "LowThreshold")
        .map(|v| v.clamp(1, 99) as u8)
        .unwrap_or(DEFAULT_LOW_THRESHOLD)
}

/// Parpadeo del rayo mientras un dispositivo carga (HKCU\Software\BatteryTray\AnimateCharging).
pub fn animate_charging() -> bool {
    get_dword(APP_KEY, "AnimateCharging").unwrap_or(1) != 0
}

pub fn set_animate_charging(on: bool) {
    set_dword(APP_KEY, "AnimateCharging", on as u32);
}

/// La barra de tareas usa tema claro → dígitos oscuros.
pub fn light_taskbar() -> bool {
    get_dword(THEME_KEY, "SystemUsesLightTheme") == Some(1)
}

pub fn current_exe() -> PathBuf {
    let mut buf = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    PathBuf::from(String::from_utf16_lossy(&buf[..n]))
}

/// Ubicación fija de la copia instalada: %LOCALAPPDATA%\BatteryTray\battery-tray.exe.
pub fn installed_exe() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(base).join("BatteryTray").join("battery-tray.exe"))
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

pub fn quoted(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

/// Copia el ejecutable actual a la ubicación fija y devuelve su ruta.
fn install() -> Option<PathBuf> {
    let (cur, dst) = (current_exe(), installed_exe()?);
    if !same_path(&cur, &dst) {
        fs::create_dir_all(dst.parent()?).ok()?;
        fs::copy(&cur, &dst).ok()?;
    }
    Some(dst)
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}

pub fn autostart_enabled() -> bool {
    let Some(v) = get_string(RUN_KEY, RUN_VALUE) else { return false };
    let target = installed_exe().map(|p| quoted(&p));
    v.eq_ignore_ascii_case(&quoted(&current_exe())) || target.is_some_and(|t| v.eq_ignore_ascii_case(&t))
}

/// Activa o desactiva el inicio con Windows. Al activarlo se instala una copia en una
/// carpeta fija, así el arranque no depende de dónde se compiló o descargó el programa.
pub fn set_autostart(enable: bool) {
    let (k, v) = (wide(RUN_KEY), wide(RUN_VALUE));
    unsafe {
        if enable {
            let target = install().unwrap_or_else(current_exe);
            let cmd = wide(&quoted(&target));
            RegSetKeyValueW(
                hkey_current_user(),
                k.as_ptr(),
                v.as_ptr(),
                REG_SZ,
                cmd.as_ptr() as *const c_void,
                (cmd.len() * 2) as u32,
            );
        } else {
            RegDeleteKeyValueW(hkey_current_user(), k.as_ptr(), v.as_ptr());
        }
    }
}

/// Al arrancar: si el inicio automático está activo, (a) migra una entrada antigua que
/// apuntaba a otra carpeta hacia la copia instalada, y (b) si este ejecutable es más
/// nuevo que la copia instalada, la actualiza.
pub fn sync_install() {
    let Some(v) = get_string(RUN_KEY, RUN_VALUE) else { return };
    let (cur, Some(dst)) = (current_exe(), installed_exe()) else { return };
    if same_path(&cur, &dst) {
        return;
    }
    let points_here = v.eq_ignore_ascii_case(&quoted(&cur));
    let points_installed = v.eq_ignore_ascii_case(&quoted(&dst));
    if !(points_here || points_installed) {
        return; // la entrada Run es de otra cosa: no tocarla
    }
    let newer = match (modified(&cur), modified(&dst)) {
        (Some(a), Some(b)) => a > b,
        _ => true,
    };
    if newer || points_here {
        set_autostart(true);
    }
}
