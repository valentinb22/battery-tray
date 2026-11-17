//! Declaraciones Win32 mínimas, escritas a mano a partir de los headers del SDK.
//! El enlace se resuelve con las bibliotecas de importación de `windows-targets`
//! (no se añade código al binario, solo entradas en la tabla de importación).
#![allow(non_snake_case, non_camel_case_types, dead_code, clippy::upper_case_acronyms)]

use core::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type HWND = HANDLE;
pub type HMENU = HANDLE;
pub type HICON = HANDLE;
pub type HBITMAP = HANDLE;
pub type HINSTANCE = HANDLE;
pub type HKEY = HANDLE;
pub type BOOL = i32;
pub type PREPARSED = isize;

// ───────────────────────────── Estructuras ─────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl GUID {
    pub const fn from_u128(v: u128) -> Self {
        Self {
            data1: (v >> 96) as u32,
            data2: (v >> 80) as u16,
            data3: (v >> 64) as u16,
            data4: (v as u64).to_be_bytes(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DEVPROPKEY {
    pub fmtid: GUID,
    pub pid: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub time: u32,
    pub pt: POINT,
    pub private: u32,
}

pub type WNDPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;
pub type TIMERPROC = unsafe extern "system" fn(HWND, u32, usize, u32);

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cb_size: u32,
    pub style: u32,
    pub wnd_proc: Option<WNDPROC>,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: HINSTANCE,
    pub icon: HICON,
    pub cursor: HANDLE,
    pub background: HANDLE,
    pub menu_name: *const u16,
    pub class_name: *const u16,
    pub icon_sm: HICON,
}

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cb_size: u32,
    pub hwnd: HWND,
    pub uid: u32,
    pub flags: u32,
    pub callback_message: u32,
    pub icon: HICON,
    pub tip: [u16; 128],
    pub state: u32,
    pub state_mask: u32,
    pub info: [u16; 256],
    pub version: u32, // unión uTimeout / uVersion
    pub info_title: [u16; 64],
    pub info_flags: u32,
    pub guid_item: GUID,
    pub balloon_icon: HICON,
}

#[repr(C)]
pub struct BITMAPINFOHEADER {
    pub size: u32,
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bit_count: u16,
    pub compression: u32,
    pub size_image: u32,
    pub x_ppm: i32,
    pub y_ppm: i32,
    pub clr_used: u32,
    pub clr_important: u32,
}

#[repr(C)]
pub struct BITMAPINFO {
    pub header: BITMAPINFOHEADER,
    pub colors: [u32; 1],
}

#[repr(C)]
pub struct ICONINFO {
    pub is_icon: BOOL,
    pub x_hotspot: u32,
    pub y_hotspot: u32,
    pub mask: HBITMAP,
    pub color: HBITMAP,
}

#[repr(C)]
pub struct DEV_BROADCAST_DEVICEINTERFACE_W {
    pub size: u32,
    pub device_type: u32,
    pub reserved: u32,
    pub class_guid: GUID,
    pub name: [u16; 1],
}

#[repr(C)]
pub struct OVERLAPPED {
    pub internal: usize,
    pub internal_high: usize,
    pub offset: u32,
    pub offset_high: u32,
    pub event: HANDLE,
}

#[repr(C)]
pub struct HIDD_ATTRIBUTES {
    pub size: u32,
    pub vendor_id: u16,
    pub product_id: u16,
    pub version: u16,
}

#[repr(C)]
pub struct HIDP_CAPS {
    pub usage: u16,
    pub usage_page: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub reserved: [u16; 17],
    pub n_link_nodes: u16,
    pub n_in_buttons: u16,
    pub n_in_values: u16,
    pub n_in_indices: u16,
    pub n_out_buttons: u16,
    pub n_out_values: u16,
    pub n_out_indices: u16,
    pub n_feat_buttons: u16,
    pub n_feat_values: u16,
    pub n_feat_indices: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HIDP_VALUE_CAPS {
    pub usage_page: u16,
    pub report_id: u8,
    pub is_alias: u8,
    pub bit_field: u16,
    pub link_collection: u16,
    pub link_usage: u16,
    pub link_usage_page: u16,
    pub is_range: u8,
    pub is_string_range: u8,
    pub is_designator_range: u8,
    pub is_absolute: u8,
    pub has_null: u8,
    pub reserved: u8,
    pub bit_size: u16,
    pub report_count: u16,
    pub reserved2: [u16; 5],
    pub units_exp: u32,
    pub units: u32,
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32,
    /// Unión Range/NotRange: u[0] = UsageMin (o Usage), u[1] = UsageMax.
    pub u: [u16; 8],
}

#[repr(C)]
#[derive(Default)]
pub struct SYSTEMTIME {
    pub year: u16,
    pub month: u16,
    pub day_of_week: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub millis: u16,
}

#[repr(C)]
pub struct PROCESS_POWER_THROTTLING_STATE {
    pub version: u32,
    pub control_mask: u32,
    pub state_mask: u32,
}

// ───────────────────────────── Constantes ─────────────────────────────

pub const WM_NULL: u32 = 0x0000;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_ENDSESSION: u32 = 0x0016;
pub const WM_SETTINGCHANGE: u32 = 0x001A;
pub const WM_CONTEXTMENU: u32 = 0x007B;
pub const WM_DISPLAYCHANGE: u32 = 0x007E;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_POWERBROADCAST: u32 = 0x0218;
pub const WM_DEVICECHANGE: u32 = 0x0219;
pub const WM_DPICHANGED: u32 = 0x02E0;
pub const WM_USER: u32 = 0x0400;
pub const WM_APP: u32 = 0x8000;

pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIM_SETVERSION: u32 = 4;
pub const NIF_MESSAGE: u32 = 0x01;
pub const NIF_ICON: u32 = 0x02;
pub const NIF_TIP: u32 = 0x04;
pub const NIF_INFO: u32 = 0x10;
pub const NIF_SHOWTIP: u32 = 0x80;
pub const NIIF_WARNING: u32 = 0x02;
pub const NIIF_NOSOUND: u32 = 0x10;
pub const NIIF_RESPECT_QUIET_TIME: u32 = 0x80;
pub const NOTIFYICON_VERSION_4: u32 = 4;
pub const NIN_SELECT: u32 = WM_USER;
pub const NIN_KEYSELECT: u32 = WM_USER | 1;

pub const MF_STRING: u32 = 0x0000;
pub const MF_GRAYED: u32 = 0x0001;
pub const MF_CHECKED: u32 = 0x0008;
pub const MF_POPUP: u32 = 0x0010;
pub const MFT_RADIOCHECK: u32 = 0x0200;
pub const MF_SEPARATOR: u32 = 0x0800;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;
pub const TPM_NONOTIFY: u32 = 0x0080;
pub const TPM_RETURNCMD: u32 = 0x0100;

pub const DBT_DEVNODES_CHANGED: u32 = 0x0007;
pub const DBT_DEVICEARRIVAL: u32 = 0x8000;
pub const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;
pub const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;
pub const DEVICE_NOTIFY_WINDOW_HANDLE: u32 = 0;
pub const DEVICE_NOTIFY_ALL_INTERFACE_CLASSES: u32 = 4;

pub const PBT_APMSUSPEND: u32 = 0x0004;
pub const PBT_APMRESUMESUSPEND: u32 = 0x0007;
pub const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;

pub const SM_CXSMICON: i32 = 49;
pub const BI_RGB: u32 = 0;
pub const DIB_RGB_COLORS: u32 = 0;

pub const ERROR_ALREADY_EXISTS: u32 = 183;
pub const ERROR_IO_PENDING: u32 = 997;
pub const WAIT_OBJECT_0: u32 = 0;

pub const GENERIC_READ: u32 = 0x8000_0000;
pub const GENERIC_WRITE: u32 = 0x4000_0000;
pub const FILE_SHARE_READ: u32 = 1;
pub const FILE_SHARE_WRITE: u32 = 2;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;

pub const REG_SZ: u32 = 1;
pub const REG_DWORD: u32 = 4;
pub const RRF_RT_REG_SZ: u32 = 0x02;
pub const RRF_RT_REG_DWORD: u32 = 0x10;

pub const CR_SUCCESS: u32 = 0;
pub const CR_BUFFER_SMALL: u32 = 0x1A;
pub const CM_GETIDLIST_FILTER_PRESENT: u32 = 0x100;
pub const CM_GET_DEVICE_INTERFACE_LIST_PRESENT: u32 = 0;
pub const DEVPROP_TYPE_BYTE: u32 = 0x03;
pub const DEVPROP_TYPE_UINT32: u32 = 0x07;
pub const DEVPROP_TYPE_BOOLEAN: u32 = 0x11;
pub const DEVPROP_TYPE_STRING: u32 = 0x12;

pub const HIDP_INPUT: i32 = 0;
pub const HIDP_FEATURE: i32 = 2;
pub const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

pub const PROCESS_POWER_THROTTLING: i32 = 4;

pub fn invalid_handle() -> HANDLE {
    -1isize as HANDLE
}
pub fn hkey_current_user() -> HKEY {
    0x8000_0001u32 as i32 as isize as HKEY
}
pub fn dpi_context_per_monitor_v2() -> HANDLE {
    -4isize as HANDLE
}

// ───────────────────────────── Funciones ─────────────────────────────

// user32
windows_targets::link!("user32.dll" "system" fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16);
windows_targets::link!("user32.dll" "system" fn CreateWindowExW(ex: u32, class: *const u16, name: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: HMENU, inst: HINSTANCE, param: *const c_void) -> HWND);
windows_targets::link!("user32.dll" "system" fn DestroyWindow(hwnd: HWND) -> BOOL);
windows_targets::link!("user32.dll" "system" fn DefWindowProcW(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize);
windows_targets::link!("user32.dll" "system" fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> BOOL);
windows_targets::link!("user32.dll" "system" fn TranslateMessage(msg: *const MSG) -> BOOL);
windows_targets::link!("user32.dll" "system" fn DispatchMessageW(msg: *const MSG) -> isize);
windows_targets::link!("user32.dll" "system" fn PostMessageW(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> BOOL);
windows_targets::link!("user32.dll" "system" fn PostQuitMessage(code: i32));
windows_targets::link!("user32.dll" "system" fn RegisterWindowMessageW(name: *const u16) -> u32);
windows_targets::link!("user32.dll" "system" fn SetTimer(hwnd: HWND, id: usize, ms: u32, f: Option<TIMERPROC>) -> usize);
windows_targets::link!("user32.dll" "system" fn SetCoalescableTimer(hwnd: HWND, id: usize, ms: u32, f: Option<TIMERPROC>, tolerance: u32) -> usize);
windows_targets::link!("user32.dll" "system" fn KillTimer(hwnd: HWND, id: usize) -> BOOL);
windows_targets::link!("user32.dll" "system" fn CreatePopupMenu() -> HMENU);
windows_targets::link!("user32.dll" "system" fn AppendMenuW(menu: HMENU, flags: u32, id: usize, text: *const u16) -> BOOL);
windows_targets::link!("user32.dll" "system" fn TrackPopupMenu(menu: HMENU, flags: u32, x: i32, y: i32, reserved: i32, hwnd: HWND, rect: *const c_void) -> BOOL);
windows_targets::link!("user32.dll" "system" fn DestroyMenu(menu: HMENU) -> BOOL);
windows_targets::link!("user32.dll" "system" fn SetForegroundWindow(hwnd: HWND) -> BOOL);
windows_targets::link!("user32.dll" "system" fn GetCursorPos(pt: *mut POINT) -> BOOL);
windows_targets::link!("user32.dll" "system" fn GetSystemMetrics(index: i32) -> i32);
windows_targets::link!("user32.dll" "system" fn SetProcessDpiAwarenessContext(ctx: HANDLE) -> BOOL);
windows_targets::link!("user32.dll" "system" fn CreateIconIndirect(info: *const ICONINFO) -> HICON);
windows_targets::link!("user32.dll" "system" fn DestroyIcon(icon: HICON) -> BOOL);
windows_targets::link!("user32.dll" "system" fn RegisterDeviceNotificationW(recipient: HANDLE, filter: *const c_void, flags: u32) -> HANDLE);
windows_targets::link!("user32.dll" "system" fn UnregisterDeviceNotification(h: HANDLE) -> BOOL);

// gdi32
windows_targets::link!("gdi32.dll" "system" fn CreateDIBSection(hdc: HANDLE, bmi: *const BITMAPINFO, usage: u32, bits: *mut *mut c_void, section: HANDLE, offset: u32) -> HBITMAP);
windows_targets::link!("gdi32.dll" "system" fn CreateBitmap(w: i32, h: i32, planes: u32, bpp: u32, bits: *const c_void) -> HBITMAP);
windows_targets::link!("gdi32.dll" "system" fn DeleteObject(obj: HANDLE) -> BOOL);

// shell32
windows_targets::link!("shell32.dll" "system" fn Shell_NotifyIconW(msg: u32, data: *const NOTIFYICONDATAW) -> BOOL);

// kernel32
windows_targets::link!("kernel32.dll" "system" fn GetModuleHandleW(name: *const u16) -> HINSTANCE);
windows_targets::link!("kernel32.dll" "system" fn GetModuleFileNameW(module: HINSTANCE, buf: *mut u16, size: u32) -> u32);
windows_targets::link!("kernel32.dll" "system" fn GetLastError() -> u32);
windows_targets::link!("kernel32.dll" "system" fn CreateMutexW(attrs: *const c_void, owner: BOOL, name: *const u16) -> HANDLE);
windows_targets::link!("kernel32.dll" "system" fn CreateEventW(attrs: *const c_void, manual: BOOL, initial: BOOL, name: *const u16) -> HANDLE);
windows_targets::link!("kernel32.dll" "system" fn ResetEvent(h: HANDLE) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32);
windows_targets::link!("kernel32.dll" "system" fn CloseHandle(h: HANDLE) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn CreateFileW(name: *const u16, access: u32, share: u32, sec: *const c_void, disposition: u32, flags: u32, template: HANDLE) -> HANDLE);
windows_targets::link!("kernel32.dll" "system" fn ReadFile(h: HANDLE, buf: *mut u8, len: u32, read: *mut u32, ov: *mut OVERLAPPED) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn WriteFile(h: HANDLE, buf: *const u8, len: u32, written: *mut u32, ov: *mut OVERLAPPED) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn GetOverlappedResult(h: HANDLE, ov: *const OVERLAPPED, n: *mut u32, wait: BOOL) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn CancelIo(h: HANDLE) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn GetCurrentProcess() -> HANDLE);
windows_targets::link!("kernel32.dll" "system" fn SetProcessWorkingSetSize(h: HANDLE, min: usize, max: usize) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn SetProcessInformation(h: HANDLE, class: i32, info: *const c_void, size: u32) -> BOOL);
windows_targets::link!("kernel32.dll" "system" fn GetLocalTime(t: *mut SYSTEMTIME));

// advapi32
windows_targets::link!("advapi32.dll" "system" fn RegGetValueW(key: HKEY, subkey: *const u16, value: *const u16, flags: u32, ty: *mut u32, data: *mut c_void, len: *mut u32) -> i32);
windows_targets::link!("advapi32.dll" "system" fn RegSetKeyValueW(key: HKEY, subkey: *const u16, value: *const u16, ty: u32, data: *const c_void, len: u32) -> i32);
windows_targets::link!("advapi32.dll" "system" fn RegDeleteKeyValueW(key: HKEY, subkey: *const u16, value: *const u16) -> i32);

// ole32
windows_targets::link!("ole32.dll" "system" fn CoIncrementMTAUsage(cookie: *mut HANDLE) -> i32);

// cfgmgr32
windows_targets::link!("cfgmgr32.dll" "system" fn CM_Get_Device_ID_List_SizeW(len: *mut u32, filter: *const u16, flags: u32) -> u32);
windows_targets::link!("cfgmgr32.dll" "system" fn CM_Get_Device_ID_ListW(filter: *const u16, buf: *mut u16, len: u32, flags: u32) -> u32);
windows_targets::link!("cfgmgr32.dll" "system" fn CM_Locate_DevNodeW(inst: *mut u32, id: *const u16, flags: u32) -> u32);
windows_targets::link!("cfgmgr32.dll" "system" fn CM_Get_DevNode_PropertyW(inst: u32, key: *const DEVPROPKEY, ty: *mut u32, buf: *mut u8, size: *mut u32, flags: u32) -> u32);
windows_targets::link!("cfgmgr32.dll" "system" fn CM_Get_Device_Interface_List_SizeW(len: *mut u32, class: *const GUID, id: *const u16, flags: u32) -> u32);
windows_targets::link!("cfgmgr32.dll" "system" fn CM_Get_Device_Interface_ListW(class: *const GUID, id: *const u16, buf: *mut u16, len: u32, flags: u32) -> u32);

// hid
windows_targets::link!("hid.dll" "system" fn HidD_GetAttributes(h: HANDLE, attrs: *mut HIDD_ATTRIBUTES) -> u8);
windows_targets::link!("hid.dll" "system" fn HidD_GetPreparsedData(h: HANDLE, pp: *mut PREPARSED) -> u8);
windows_targets::link!("hid.dll" "system" fn HidD_FreePreparsedData(pp: PREPARSED) -> u8);
windows_targets::link!("hid.dll" "system" fn HidD_GetFeature(h: HANDLE, buf: *mut c_void, len: u32) -> u8);
windows_targets::link!("hid.dll" "system" fn HidD_SetFeature(h: HANDLE, buf: *const c_void, len: u32) -> u8);
windows_targets::link!("hid.dll" "system" fn HidD_GetInputReport(h: HANDLE, buf: *mut c_void, len: u32) -> u8);
windows_targets::link!("hid.dll" "system" fn HidD_GetProductString(h: HANDLE, buf: *mut c_void, len: u32) -> u8);
windows_targets::link!("hid.dll" "system" fn HidP_GetCaps(pp: PREPARSED, caps: *mut HIDP_CAPS) -> i32);
windows_targets::link!("hid.dll" "system" fn HidP_GetValueCaps(ty: i32, caps: *mut HIDP_VALUE_CAPS, len: *mut u16, pp: PREPARSED) -> i32);
windows_targets::link!("hid.dll" "system" fn HidP_GetUsageValue(ty: i32, page: u16, link: u16, usage: u16, value: *mut u32, pp: PREPARSED, report: *const u8, len: u32) -> i32);

// ───────────────────────────── Utilidades ─────────────────────────────

/// UTF-16 terminado en NUL.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Copia `s` en un buffer fijo UTF-16, truncando con "…" si no cabe.
pub fn copy_wstr(dst: &mut [u16], s: &str) {
    let src: Vec<u16> = s.encode_utf16().collect();
    let cap = dst.len().saturating_sub(1);
    if src.len() <= cap {
        dst[..src.len()].copy_from_slice(&src);
        dst[src.len()] = 0;
    } else if cap > 0 {
        let n = cap - 1;
        dst[..n].copy_from_slice(&src[..n]);
        dst[n] = 0x2026;
        dst[cap] = 0;
    }
}

/// UTF-16 (hasta el primer NUL) → String.
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Cierra un HANDLE al salir de ámbito.
pub struct HandleGuard(pub HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != invalid_handle() {
            unsafe { CloseHandle(self.0) };
        }
    }
}
