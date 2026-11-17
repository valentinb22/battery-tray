//! Instalador integrado, por usuario y sin permisos de administrador.
//!
//! Al abrir el `.exe` desde cualquier carpeta (p. ej. Descargas):
//!   - si no está instalado, pregunta y, si se acepta, lo copia a
//!     %LOCALAPPDATA%\BatteryTray, crea el acceso directo del menú Inicio, lo registra en
//!     Configuración → Aplicaciones (con desinstalador) y activa el inicio con Windows;
//!   - si ya está instalado, actualiza la copia instalada (cierra la que está corriendo).
//! `battery-tray.exe --uninstall` (lo que ejecuta Windows al desinstalar) deshace todo.

use crate::config::{self, current_exe, installed_exe, quoted, same_path};
use crate::ffi::*;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

const TITLE: &str = "Battery Tray";
const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\BatteryTray";
pub const WINDOW_CLASS: &str = "BatteryTrayHiddenWindow";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn ask(text: &str) -> bool {
    let (t, c) = (wide(text), wide(TITLE));
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND) == IDYES }
}

fn tell(text: &str, flags: u32) {
    let (t, c) = (wide(text), wide(TITLE));
    unsafe { MessageBoxW(null_mut(), t.as_ptr(), c.as_ptr(), MB_OK | flags | MB_SETFOREGROUND) };
}

fn start_menu_link() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("Microsoft\\Windows\\Start Menu\\Programs\\Battery Tray.lnk"))
}

/// Se llama al arrancar, antes de crear la ventana. Devuelve `true` si el proceso debe
/// terminar (se instaló o actualizó y ya se lanzó la copia instalada).
pub fn on_launch() -> bool {
    let (cur, Some(dst)) = (current_exe(), installed_exe()) else { return false };
    if same_path(&cur, &dst) {
        // Copia instalada. Las instalaciones anteriores (solo "Iniciar con Windows") no
        // tenían entrada en Aplicaciones ni acceso directo: se completan aquí.
        if config::get_string(UNINSTALL_KEY, "UninstallString").is_none() {
            register(&dst);
            let _ = create_shortcut(&dst);
        }
        return false;
    }
    if dst.exists() {
        // Ya instalado: esta es una versión nueva (o la misma) y se actualiza sin preguntar.
        return install(&cur, &dst, false);
    }
    let msg = "¿Instalar Battery Tray en este equipo?\n\n\
               • Se copia a tu carpeta de usuario (no necesita permisos de administrador).\n\
               • Se agrega al menú Inicio y arranca con Windows.\n\
               • Se desinstala desde Configuración → Aplicaciones.\n\n\
               Si elegís «No», se ejecuta sin instalar.";
    if !ask(msg) {
        return false;
    }
    install(&cur, &dst, true)
}

fn install(cur: &Path, dst: &Path, first_time: bool) -> bool {
    close_running();
    let Some(dir) = dst.parent() else { return false };
    let copied = fs::create_dir_all(dir).is_ok() && copy_with_retry(cur, dst);
    if !copied {
        tell("No se pudo copiar el programa a su carpeta de instalación.", MB_ICONWARNING);
        return false;
    }
    register(dst);
    let _ = create_shortcut(dst);
    config::set_autostart(true);
    if Command::new(dst).spawn().is_err() {
        return false;
    }
    if first_time {
        tell(
            "Battery Tray quedó instalado.\n\n\
             Sus iconos aparecen en la bandeja, junto al reloj. Si no los ves, abrí la flecha ˄ \
             y arrastralos a la barra de tareas.",
            MB_ICONINFORMATION,
        );
    }
    true
}

fn copy_with_retry(from: &Path, to: &Path) -> bool {
    // La instancia anterior puede tardar un momento en soltar el archivo.
    let start = Instant::now();
    loop {
        if fs::copy(from, to).is_ok() {
            return true;
        }
        if start.elapsed() > Duration::from_secs(5) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Cierra la instancia en ejecución (si la hay) y espera a que termine.
fn close_running() {
    let class = wide(WINDOW_CLASS);
    let start = Instant::now();
    unsafe {
        loop {
            let hwnd = FindWindowW(class.as_ptr(), null());
            if hwnd.is_null() {
                break;
            }
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            if start.elapsed() > Duration::from_secs(5) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    std::thread::sleep(Duration::from_millis(300)); // margen para que el proceso salga
}

/// Entrada de Configuración → Aplicaciones (HKCU: solo para este usuario).
fn register(exe: &Path) {
    let k = UNINSTALL_KEY;
    let dir = exe.parent().map(|d| d.display().to_string()).unwrap_or_default();
    config::set_string(k, "DisplayName", TITLE);
    config::set_string(k, "DisplayVersion", env!("CARGO_PKG_VERSION"));
    config::set_string(k, "DisplayIcon", &format!("{},0", exe.display()));
    config::set_string(k, "InstallLocation", &dir);
    config::set_string(k, "UninstallString", &format!("{} --uninstall", quoted(exe)));
    config::set_string(k, "URLInfoAbout", "https://github.com/valentinb22/battery-tray");
    config::set_dword(k, "NoModify", 1);
    config::set_dword(k, "NoRepair", 1);
    let kb = fs::metadata(exe).map(|m| (m.len() / 1024) as u32).unwrap_or(0);
    config::set_dword(k, "EstimatedSize", kb);
}

fn create_shortcut(target: &Path) -> windows::core::Result<()> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let Some(lnk) = start_menu_link() else { return Ok(()) };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(target.as_os_str()))?;
        if let Some(dir) = target.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        link.SetDescription(&HSTRING::from("Batería de mouse, teclado, auriculares y mandos inalámbricos"))?;
        link.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk.as_os_str()), true)?;
    }
    Ok(())
}

pub fn uninstall() {
    if !ask("¿Desinstalar Battery Tray?\n\nSe quitan el programa, el acceso directo, el inicio con Windows y sus ajustes.") {
        return;
    }
    close_running();
    let cur = current_exe();
    let dst = installed_exe();
    // Inicio con Windows: solo si la entrada es nuestra.
    if let Some(v) = config::get_string(config::RUN_KEY, config::RUN_VALUE) {
        let ours = v.eq_ignore_ascii_case(&quoted(&cur))
            || dst.as_ref().is_some_and(|d| v.eq_ignore_ascii_case(&quoted(d)));
        if ours {
            config::set_autostart(false);
        }
    }
    config::delete_tree(UNINSTALL_KEY);
    config::delete_tree(config::APP_KEY);
    if let Some(lnk) = start_menu_link() {
        let _ = fs::remove_file(lnk);
    }
    if let Some(dst) = dst {
        if same_path(&cur, &dst) {
            // Un ejecutable no puede borrarse a sí mismo: lo hace cmd cuando este proceso sale.
            if let Some(dir) = dst.parent() {
                let script = format!(
                    "/C ping -n 3 127.0.0.1 >nul & del /f /q {} & rmdir /q {}",
                    quoted(&dst),
                    quoted(dir)
                );
                let _ = Command::new("cmd.exe").raw_arg(script).creation_flags(CREATE_NO_WINDOW).spawn();
            }
        } else {
            let _ = fs::remove_file(&dst);
            if let Some(dir) = dst.parent() {
                let _ = fs::remove_dir(dir);
            }
        }
    }
    tell("Battery Tray se desinstaló.", MB_ICONINFORMATION);
}
