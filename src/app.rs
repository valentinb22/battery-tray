//! Estado de la aplicación, procedimiento de ventana, menú, temporizadores y alertas.

use crate::devices::{self, Charge, Device, Kind};
use crate::icon::Glyph;
use crate::ffi::*;
use crate::tray::Tray;
use crate::{config, icon};
use core::ffi::c_void;
use std::cell::RefCell;
use std::collections::HashSet;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const WM_TRAY: u32 = WM_APP + 1;
const WM_SCAN_DONE: u32 = WM_APP + 2;

const TIMER_POLL: usize = 1;
const TIMER_DEBOUNCE: usize = 2;
const TIMER_RETRY_TRAY: usize = 3;
const TIMER_ANIM: usize = 4;
const ANIM_MS: u32 = 700;

const ID_REFRESH: usize = 10;
const ID_AUTOSTART: usize = 11;
const ID_EXIT: usize = 12;
const ID_ANIMATE: usize = 13;
const ID_INTERVAL: usize = 100; // 100 + minutos
const ID_DEVICE: usize = 1000;
const INTERVALS: [u32; 6] = [1, 3, 5, 10, 15, 30];

/// Hay un escaneo en curso (lo comparten el hilo de UI y el hilo de escaneo).
static BUSY: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Un icono de bandeja (uno por dispositivo inalámbrico, o uno genérico si no hay).
struct Slot {
    key: String,
    tray: Tray,
    frames: Vec<HICON>,
    frame: usize,
    /// Parámetros con los que se dibujaron los fotogramas (evita redibujar sin cambios).
    sig: Option<(Glyph, Option<u8>, bool, bool, i32, bool)>,
}

impl Slot {
    fn destroy_frames(&mut self) {
        for f in self.frames.drain(..) {
            if !f.is_null() {
                unsafe { DestroyIcon(f) };
            }
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.tray.remove();
        self.destroy_frames();
    }
}

/// FNV-1a de 32 bits (nunca 0) a partir de la clave normalizada del dispositivo.
fn stable_uid(key: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in key.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    (h & 0x7FFF_FFFF).max(1)
}

const NO_DEVICE_KEY: &str = "\u{0}app";

struct App {
    hwnd: HWND,
    slots: Vec<Slot>,
    animate: bool,
    anim_running: bool,
    devices: Vec<Device>,
    alerted: HashSet<String>,
    poll_minutes: u32,
    threshold: u8,
    pending: bool,
    suspended: bool,
    scanned_once: bool,
    updated_at: Option<(u16, u16)>,
    dev_notify: HANDLE,
}

pub unsafe fn init(hwnd: HWND) {
    TASKBAR_CREATED.store(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()), Ordering::Relaxed);

    let mut app = App {
        hwnd,
        slots: Vec::new(),
        animate: config::animate_charging(),
        anim_running: false,
        devices: Vec::new(),
        alerted: HashSet::new(),
        poll_minutes: config::poll_minutes(),
        threshold: config::low_threshold(),
        pending: false,
        suspended: false,
        scanned_once: false,
        updated_at: None,
        dev_notify: null_mut(),
    };

    // Notificaciones de llegada/retirada de cualquier interfaz de dispositivo
    // (HID, Bluetooth, audio…). Se agrupan con un debounce antes de reescanear.
    let mut filter: DEV_BROADCAST_DEVICEINTERFACE_W = zeroed();
    filter.size = size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32;
    filter.device_type = DBT_DEVTYP_DEVICEINTERFACE;
    app.dev_notify = RegisterDeviceNotificationW(
        hwnd,
        &filter as *const _ as *const c_void,
        DEVICE_NOTIFY_WINDOW_HANDLE | DEVICE_NOTIFY_ALL_INTERFACE_CLASSES,
    );

    app.redraw();
    app.start_poll();
    app.refresh();
    APP.with(|c| *c.borrow_mut() = Some(app));
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|c| match c.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map(f),
        Err(_) => None,
    })
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        WM_TRAY => {
            // NOTIFYICON_VERSION_4: LOWORD(lParam) = evento, wParam = coordenadas ancla.
            match (lp as u32) & 0xFFFF {
                WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT => {
                    let x = (wp & 0xFFFF) as u16 as i16 as i32;
                    let y = ((wp >> 16) & 0xFFFF) as u16 as i16 as i32;
                    show_menu(hwnd, x, y);
                }
                _ => {}
            }
            0
        }
        WM_SCAN_DONE => {
            BUSY.store(false, Ordering::SeqCst);
            let list = *Box::from_raw(lp as *mut Vec<Device>);
            with_app(|a| a.on_scan(list));
            0
        }
        WM_TIMER => {
            with_app(|a| a.on_timer(wp));
            0
        }
        WM_DEVICECHANGE => {
            if matches!(wp as u32, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE | DBT_DEVNODES_CHANGED) {
                with_app(|a| a.schedule(3_000));
            }
            1
        }
        WM_POWERBROADCAST => {
            match wp as u32 {
                PBT_APMSUSPEND => {
                    with_app(|a| a.suspend());
                }
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                    with_app(|a| a.resume());
                }
                _ => {}
            }
            1
        }
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_app(|a| a.redraw());
            0
        }
        WM_ENDSESSION => {
            if wp != 0 {
                with_app(|a| a.slots.clear());
            }
            0
        }
        WM_DESTROY => {
            let app = APP.with(|c| c.try_borrow_mut().ok().and_then(|mut g| g.take()));
            drop(app);
            PostQuitMessage(0);
            0
        }
        _ => {
            let tc = TASKBAR_CREATED.load(Ordering::Relaxed);
            if tc != 0 && msg == tc {
                with_app(|a| {
                    for s in &mut a.slots {
                        s.tray.forget();
                    }
                    a.redraw();
                });
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }
}

/// El menú se muestra SIN mantener prestado el estado: TrackPopupMenu ejecuta un bucle
/// modal que sigue despachando WM_TIMER / WM_SCAN_DONE a este mismo wndproc.
unsafe fn show_menu(hwnd: HWND, mut x: i32, mut y: i32) {
    let Some(menu) = with_app(|a| a.build_menu()) else { return };
    if menu.is_null() {
        return;
    }
    if x == 0 && y == 0 {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt);
        x = pt.x;
        y = pt.y;
    }
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY, x, y, 0, hwnd, null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    if cmd > 0 && with_app(|a| a.on_command(cmd as usize)) == Some(true) {
        DestroyWindow(hwnd);
    }
}

impl App {
    /// Lanza un escaneo en un hilo efímero (pila de 512 KB) y vuelve inmediatamente.
    fn refresh(&mut self) {
        if self.suspended {
            return;
        }
        if BUSY.swap(true, Ordering::SeqCst) {
            self.pending = true;
            return;
        }
        let hwnd = self.hwnd as usize;
        let spawned = std::thread::Builder::new()
            .name("scan".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let list = devices::collect_all();
                let ptr = Box::into_raw(Box::new(list));
                unsafe {
                    if PostMessageW(hwnd as HWND, WM_SCAN_DONE, 0, ptr as isize) == 0 {
                        drop(Box::from_raw(ptr));
                        BUSY.store(false, Ordering::SeqCst);
                    }
                }
            });
        if spawned.is_err() {
            BUSY.store(false, Ordering::SeqCst);
        }
    }

    fn on_scan(&mut self, list: Vec<Device>) {
        self.devices = list;
        self.scanned_once = true;
        let mut t = SYSTEMTIME::default();
        unsafe { GetLocalTime(&mut t) };
        self.updated_at = Some((t.hour, t.minute));
        self.redraw();
        self.check_alerts();
        // Devuelve al sistema las páginas tocadas durante el escaneo (WinRT, HID…).
        unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
        if std::mem::take(&mut self.pending) {
            self.refresh();
        } else if devices::needs_quick_retry() {
            // Receptor presente sin ninguna lectura (teclado dormido): reintento en 1 min.
            self.schedule(60_000);
        }
    }

    fn on_timer(&mut self, id: usize) {
        match id {
            TIMER_POLL => self.refresh(),
            TIMER_DEBOUNCE => {
                unsafe { KillTimer(self.hwnd, TIMER_DEBOUNCE) };
                self.refresh();
            }
            TIMER_RETRY_TRAY => {
                if self.redraw() {
                    unsafe { KillTimer(self.hwnd, TIMER_RETRY_TRAY) };
                }
            }
            TIMER_ANIM => {
                for s in &mut self.slots {
                    if s.frames.len() > 1 {
                        s.frame = (s.frame + 1) % s.frames.len();
                        s.tray.set_icon(s.frames[s.frame]);
                    }
                }
            }
            _ => {}
        }
    }

    /// Reescaneo diferido: varias notificaciones seguidas reinician la cuenta atrás.
    fn schedule(&mut self, ms: u32) {
        if !self.suspended {
            unsafe { SetTimer(self.hwnd, TIMER_DEBOUNCE, ms, None) };
        }
    }

    /// Temporizador coalescible: Windows puede agruparlo con otros despertares (±10 %).
    fn start_poll(&mut self) {
        let ms = self.poll_minutes * 60_000;
        unsafe { SetCoalescableTimer(self.hwnd, TIMER_POLL, ms, None, ms / 10) };
    }

    fn suspend(&mut self) {
        self.suspended = true;
        unsafe {
            KillTimer(self.hwnd, TIMER_POLL);
            KillTimer(self.hwnd, TIMER_DEBOUNCE);
            KillTimer(self.hwnd, TIMER_ANIM);
        }
        self.anim_running = false;
    }

    fn resume(&mut self) {
        if !self.suspended {
            return;
        }
        self.suspended = false;
        self.start_poll();
        self.redraw();
        // Los periféricos tardan unos segundos en reconectarse tras la reanudación.
        self.schedule(8_000);
    }

    /// Sincroniza los iconos de bandeja con los dispositivos: uno por cada periférico
    /// inalámbrico (con batería); los de cable no tienen icono. Sin ninguno, queda un
    /// icono genérico para poder abrir el menú. Devuelve `false` si el Explorer rechazó alguno.
    fn redraw(&mut self) -> bool {
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
        let light = config::light_taskbar();

        let mut wanted: Vec<(String, Glyph, Option<u8>, bool, String)> = self
            .devices
            .iter()
            .filter_map(|d| {
                let level = d.level?;
                let glyph = match d.kind {
                    Kind::Keyboard => Glyph::Keyboard,
                    Kind::Mouse => Glyph::Mouse,
                    Kind::Headset => Glyph::Headset,
                    _ => Glyph::Battery,
                };
                let mut tip = format!("{}\n{}%  ·  {}", d.name, level, d.charge.label());
                if let Some(t) = &d.asleep {
                    tip.push_str(&format!("\nDormido · última lectura {t}"));
                }
                Some((devices::norm(&d.name), glyph, Some(level), d.charge == Charge::Charging, tip))
            })
            .collect();
        if wanted.is_empty() {
            wanted.push((NO_DEVICE_KEY.into(), Glyph::Battery, None, false, self.summary_tip()));
        }

        // Quita los iconos de dispositivos que ya no están (Drop → NIM_DELETE).
        self.slots.retain(|s| wanted.iter().any(|w| w.0 == s.key));

        let mut ok = true;
        for (key, glyph, level, charging, tip) in wanted {
            let idx = match self.slots.iter().position(|s| s.key == key) {
                Some(i) => i,
                None => {
                    // ID fijo por dispositivo: Windows recuerda por ID si el icono va en la
                    // barra o en el panel oculto, así que no debe depender del orden de detección.
                    let mut uid = stable_uid(&key);
                    while self.slots.iter().any(|s| s.tray.uid() == uid) {
                        uid = uid.wrapping_add(1).max(1);
                    }
                    self.slots.push(Slot { key, tray: Tray::new(self.hwnd, uid, WM_TRAY), frames: Vec::new(), frame: 0, sig: None });
                    self.slots.len() - 1
                }
            };
            let animate = self.animate;
            let slot = &mut self.slots[idx];
            let sig = (glyph, level, charging, light, size, animate);
            if slot.sig != Some(sig) {
                slot.destroy_frames();
                slot.frames.push(icon::render(size, glyph, level, charging, charging, light));
                if charging && animate {
                    slot.frames.push(icon::render(size, glyph, level, false, charging, light));
                }
                slot.frame = 0;
                slot.sig = Some(sig);
            }
            ok &= slot.tray.set(slot.frames[slot.frame], &tip);
        }

        // La animación solo existe mientras algo carga: en reposo no hay temporizador.
        let need_anim = !self.suspended && self.slots.iter().any(|s| s.frames.len() > 1);
        unsafe {
            if need_anim && !self.anim_running {
                SetTimer(self.hwnd, TIMER_ANIM, ANIM_MS, None);
            } else if !need_anim && self.anim_running {
                KillTimer(self.hwnd, TIMER_ANIM);
            }
        }
        self.anim_running = need_anim;

        if !ok {
            unsafe { SetTimer(self.hwnd, TIMER_RETRY_TRAY, 2_000, None) };
        }
        ok
    }

    /// Tooltip del icono genérico (sin periféricos inalámbricos).
    fn summary_tip(&self) -> String {
        if !self.scanned_once {
            return "Batería de periféricos\nBuscando dispositivos…".into();
        }
        let mut lines = vec!["Batería de periféricos".to_string(), "Sin periféricos inalámbricos".to_string()];
        for d in &self.devices {
            lines.push(format!("{}: {}", d.name, d.charge.label().to_lowercase()));
        }
        lines.join("\n")
    }

    fn check_alerts(&mut self) {
        let th = self.threshold;
        let mut low = Vec::new();
        for d in &self.devices {
            let Some(level) = d.level else { continue };
            let key = devices::norm(&d.name);
            let powered = matches!(d.charge, Charge::Charging | Charge::Full);
            if level < th && !powered {
                // Solo la primera vez que cruza el umbral.
                if self.alerted.insert(key) {
                    low.push(format!("{}: {}%", d.name, level));
                }
            } else if powered || level >= th.saturating_add(5) {
                // Histéresis de 5 puntos para no alternar alrededor del umbral.
                self.alerted.remove(&key);
            }
        }
        if !low.is_empty() {
            let text = format!("{}\nConéctalo para cargar.", low.join("\n"));
            for s in &self.slots {
                if s.tray.notify("Batería baja", &text) {
                    break;
                }
            }
        }
    }

    unsafe fn build_menu(&self) -> HMENU {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return menu;
        }
        let add = |m: HMENU, flags: u32, id: usize, text: &str| {
            let w = wide(text);
            AppendMenuW(m, flags, id, w.as_ptr());
        };

        if self.devices.is_empty() {
            let msg = if self.scanned_once { "Sin periféricos detectados" } else { "Buscando dispositivos…" };
            add(menu, MF_STRING | MF_GRAYED, ID_DEVICE, msg);
        }
        for (i, d) in self.devices.iter().enumerate() {
            // "\t" alinea a la derecha la segunda columna del menú.
            let mut status = match d.level {
                Some(l) => format!("{}%  ·  {}", l, d.charge.label()),
                None => d.charge.label().to_string(),
            };
            if let Some(t) = &d.asleep {
                status.push_str(&format!("  ·  dormido ({t})"));
            }
            let text = format!("{}  ·  {}\t{}", d.kind.label(), d.name.replace('&', "&&"), status);
            add(menu, MF_STRING, ID_DEVICE + 1 + i, &text);
        }

        add(menu, MF_SEPARATOR, 0, "");
        if let Some((h, m)) = self.updated_at {
            add(menu, MF_STRING | MF_GRAYED, 0, &format!("Actualizado a las {h:02}:{m:02}"));
        }
        add(menu, MF_STRING, ID_REFRESH, "Actualizar ahora");

        let sub = CreatePopupMenu();
        if !sub.is_null() {
            for m in INTERVALS {
                let flags = if m == self.poll_minutes { MF_STRING | MF_CHECKED | MFT_RADIOCHECK } else { MF_STRING };
                let label = if m == 1 { "Cada minuto".to_string() } else { format!("Cada {m} minutos") };
                add(sub, flags, ID_INTERVAL + m as usize, &label);
            }
            add(menu, MF_POPUP, sub as usize, "Intervalo de consulta");
        }

        let anim = if self.animate { MF_STRING | MF_CHECKED } else { MF_STRING };
        add(menu, anim, ID_ANIMATE, "Animar icono al cargar");
        let auto = if config::autostart_enabled() { MF_STRING | MF_CHECKED } else { MF_STRING };
        add(menu, auto, ID_AUTOSTART, "Iniciar con Windows");
        add(menu, MF_SEPARATOR, 0, "");
        add(menu, MF_STRING, ID_EXIT, "Salir");
        menu
    }

    /// Devuelve `true` si hay que cerrar la aplicación.
    fn on_command(&mut self, id: usize) -> bool {
        match id {
            ID_REFRESH => self.refresh(),
            ID_AUTOSTART => config::set_autostart(!config::autostart_enabled()),
            ID_EXIT => return true,
            ID_ANIMATE => {
                self.animate = !self.animate;
                config::set_animate_charging(self.animate);
                self.redraw();
            }
            i if (ID_INTERVAL + 1..ID_DEVICE).contains(&i) => {
                let m = (i - ID_INTERVAL) as u32;
                self.poll_minutes = m;
                config::set_poll_minutes(m);
                self.start_poll();
            }
            _ => {}
        }
        false
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            KillTimer(self.hwnd, TIMER_POLL);
            KillTimer(self.hwnd, TIMER_DEBOUNCE);
            KillTimer(self.hwnd, TIMER_RETRY_TRAY);
            KillTimer(self.hwnd, TIMER_ANIM);
            if !self.dev_notify.is_null() {
                UnregisterDeviceNotification(self.dev_notify);
            }
        }
        self.slots.clear();
    }
}
