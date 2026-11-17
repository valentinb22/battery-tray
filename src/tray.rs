//! Envoltorio de Shell_NotifyIconW para UN icono de bandeja (identificado por `uid`).
//! No posee los HICON: los gestiona quien lo usa (para poder alternar fotogramas).

use crate::ffi::*;
use std::mem::{size_of, zeroed};

pub struct Tray {
    hwnd: HWND,
    uid: u32,
    callback: u32,
    added: bool,
}

impl Tray {
    pub fn new(hwnd: HWND, uid: u32, callback: u32) -> Self {
        Self { hwnd, uid, callback, added: false }
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    fn base(&self) -> NOTIFYICONDATAW {
        let mut n: NOTIFYICONDATAW = unsafe { zeroed() };
        n.cb_size = size_of::<NOTIFYICONDATAW>() as u32;
        n.hwnd = self.hwnd;
        n.uid = self.uid;
        n
    }

    /// Añade o actualiza el icono. Devuelve `false` si el Explorer no lo aceptó.
    pub fn set(&mut self, icon: HICON, tip: &str) -> bool {
        let mut n = self.base();
        n.flags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        n.callback_message = self.callback;
        n.icon = icon;
        copy_wstr(&mut n.tip, tip);
        let ok = unsafe {
            if self.added {
                Shell_NotifyIconW(NIM_MODIFY, &n) != 0
            } else if Shell_NotifyIconW(NIM_ADD, &n) != 0 {
                n.version = NOTIFYICON_VERSION_4;
                Shell_NotifyIconW(NIM_SETVERSION, &n);
                true
            } else {
                false
            }
        };
        self.added = ok;
        ok
    }

    /// Solo cambia la imagen (fotograma de animación), sin tocar el tooltip.
    pub fn set_icon(&self, icon: HICON) {
        if self.added {
            let mut n = self.base();
            n.flags = NIF_ICON;
            n.icon = icon;
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &n) };
        }
    }

    /// Notificación nativa silenciosa (Windows 10/11 la muestra como toast).
    pub fn notify(&self, title: &str, text: &str) -> bool {
        if !self.added {
            return false;
        }
        let mut n = self.base();
        n.flags = NIF_INFO;
        copy_wstr(&mut n.info_title, title);
        copy_wstr(&mut n.info, text);
        n.info_flags = NIIF_WARNING | NIIF_NOSOUND | NIIF_RESPECT_QUIET_TIME;
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &n) != 0 }
    }

    /// El Explorer se reinició: el icono ya no existe.
    pub fn forget(&mut self) {
        self.added = false;
    }

    pub fn remove(&mut self) {
        if self.added {
            let n = self.base();
            unsafe { Shell_NotifyIconW(NIM_DELETE, &n) };
            self.added = false;
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}
