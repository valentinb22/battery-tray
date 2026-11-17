//! Battery Tray — nivel de batería de periféricos inalámbricos en la bandeja de Windows.
//!
//! Un único proceso nativo: una ventana oculta (nunca visible, no aparece en Alt+Tab)
//! que recibe los mensajes del icono de bandeja, cambios PnP y eventos de energía.
//! El escaneo de dispositivos corre en un hilo efímero; el hilo de UI solo duerme
//! en `GetMessageW` entre eventos (0 % de CPU en reposo).
#![windows_subsystem = "windows"]

mod app;
mod config;
mod devices;
mod ffi;
mod icon;
mod tray;

use core::ffi::c_void;
use ffi::*;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

fn main() {
    // Diagnóstico: `battery-tray.exe --list | more` imprime lo detectado y sale.
    if std::env::args().any(|a| a == "--list") {
        let mut cookie: HANDLE = null_mut();
        unsafe { CoIncrementMTAUsage(&mut cookie) };
        let start = std::time::Instant::now();
        let list = devices::collect_all();
        for d in &list {
            let level = d.level.map_or("  -".to_string(), |l| format!("{l:>3}%"));
            let note = d.asleep.as_ref().map_or(String::new(), |t| format!("  (dormido, lectura de las {t})"));
            println!("{:<12} {:<40} {:>4}  {}{}", d.kind.label(), d.name, level, d.charge.label(), note);
        }
        println!("{} dispositivo(s) en {} ms", list.len(), start.elapsed().as_millis());
        return;
    }
    unsafe {
        // Instancia única por sesión de usuario.
        let mutex_name = wide("Local\\BatteryTray.SingleInstance");
        let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        let _mutex = HandleGuard(mutex);

        // Mantiene la copia instalada (la que arranca con Windows) al día.
        config::sync_install();

        // Iconos nítidos en cualquier escala (125 %, 150 %…).
        SetProcessDpiAwarenessContext(dpi_context_per_monitor_v2());

        // MTA implícito y permanente para las llamadas WinRT de los hilos de escaneo.
        // Evita inicializar/desinicializar COM en cada escaneo (y factorías colgantes).
        let mut cookie: HANDLE = null_mut();
        CoIncrementMTAUsage(&mut cookie);

        // EcoQoS: Windows puede ejecutar este proceso en núcleos de eficiencia/baja frecuencia.
        let eco = PROCESS_POWER_THROTTLING_STATE { version: 1, control_mask: 1, state_mask: 1 };
        SetProcessInformation(
            GetCurrentProcess(),
            PROCESS_POWER_THROTTLING,
            &eco as *const _ as *const c_void,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        let instance = GetModuleHandleW(null());
        let class = wide("BatteryTrayHiddenWindow");
        let wc = WNDCLASSEXW {
            cb_size: size_of::<WNDCLASSEXW>() as u32,
            wnd_proc: Some(app::wndproc),
            instance,
            class_name: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassExW(&wc) == 0 {
            return;
        }

        // Ventana de nivel superior oculta (no "message-only"): así recibe los broadcasts
        // WM_POWERBROADCAST, WM_SETTINGCHANGE y "TaskbarCreated". WS_EX_TOOLWINDOW +
        // nunca llamar a ShowWindow ⇒ jamás aparece en la barra de tareas ni en Alt+Tab.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }

        app::init(hwnd);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
