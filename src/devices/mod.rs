//! Modelo de dispositivo y fusión de las distintas fuentes de batería.
//!
//! Fuentes (de mayor a menor prioridad cuando un mismo periférico aparece varias veces):
//!   40  Protocolos de fabricante: Logitech HID++ 2.0, Razer, SteelSeries, Corsair, HyperX,
//!       Compx (3554:FA09) y mandos PlayStation
//!   30  BLE GATT Battery Service 0x180F / 0x2A19 (WinRT) y mandos Xbox (XInput)
//!   20  HID estándar: Usage Page 0x06, Usage 0x20 "Battery Strength"
//!   10  Propiedad PnP de batería que mantiene la pila Bluetooth de Windows

mod ble;
mod compx;
mod gamepads;
mod headsets;
mod hid;
mod hidpp;
mod hyperx;
mod pnp;
mod razer;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    Mouse,
    Keyboard,
    Headset,
    Gamepad,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Mouse => "Mouse",
            Kind::Keyboard => "Teclado",
            Kind::Headset => "Auriculares",
            Kind::Gamepad => "Mando",
            Kind::Other => "Dispositivo",
        }
    }

    /// Último recurso: deducir el tipo a partir del nombre comercial.
    pub fn from_name(name: &str) -> Kind {
        let n = name.to_lowercase();
        let any = |words: &[&str]| words.iter().any(|w| n.contains(w));
        if any(&[
            "headset", "headphone", "auricular", "buds", "airpods", "earbud", "hands-free", "handsfree",
            "wh-", "wf-", "arctis", "cloud", "blackshark", "kraken", "g733", "g435", "g535", "g pro x",
            "quietcomfort", "jabra", "audio",
        ]) {
            Kind::Headset
        } else if any(&[
            "mouse", "ratón", "raton", "mx master", "mx anywhere", "mx ergo", "trackball", "g502", "g305",
            "g703", "g903", "g pro", "viper", "deathadder", "basilisk", "orochi", "harpoon", "dark core",
        ]) {
            Kind::Mouse
        } else if any(&["keyboard", "teclado", "keys", "mx mechanical", "craft", "k380", "k780", "k850"]) {
            Kind::Keyboard
        } else if any(&["controller", "gamepad", "mando", "xbox", "dualsense", "dualshock", "joy-con"]) {
            Kind::Gamepad
        } else {
            Kind::Other
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Charge {
    Charging,
    Discharging,
    Full,
    Unknown,
    /// Periférico con cable: sin batería.
    Wired,
}

impl Charge {
    pub fn label(self) -> &'static str {
        match self {
            Charge::Charging => "Cargando",
            Charge::Discharging => "Descargando",
            Charge::Full => "Carga completa",
            Charge::Unknown => "Estado no reportado",
            Charge::Wired => "Con cable",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub kind: Kind,
    /// `None` = sin batería (periférico con cable).
    pub level: Option<u8>,
    pub charge: Charge,
    pub priority: u8,
    /// `Some("HH:MM")`: el dispositivo está dormido y se muestra su última lectura.
    pub asleep: Option<String>,
}

/// Algún receptor está presente pero aún no tenemos ninguna lectura suya: conviene
/// reintentar pronto (p. ej. teclado dormido justo al arrancar).
pub fn needs_quick_retry() -> bool {
    compx::needs_retry()
}

/// Recorre todas las fuentes. Se ejecuta en el hilo de escaneo; cada fuente
/// captura sus propios errores y nunca hace pánico.
pub fn collect_all() -> Vec<Device> {
    let mut all = Vec::new();
    pnp::collect(&mut all);
    hid::collect(&mut all);
    gamepads::collect_xinput(&mut all);
    ble::collect(&mut all);
    merge(all)
}

/// Clave normalizada de nombre (minúsculas, solo alfanuméricos).
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn same_device(a: &str, b: &str) -> bool {
    let (a, b) = (norm(a), norm(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || (a.len() >= 4 && b.len() >= 4 && (a.contains(&b) || b.contains(&a)))
}

fn merge(mut all: Vec<Device>) -> Vec<Device> {
    all.sort_by(|a, b| b.priority.cmp(&a.priority));
    let mut out: Vec<Device> = Vec::new();
    for d in all {
        if let Some(e) = out.iter_mut().find(|e| same_device(&e.name, &d.name)) {
            if e.kind == Kind::Other {
                e.kind = d.kind;
            }
            if e.charge == Charge::Unknown && d.charge != Charge::Wired {
                e.charge = d.charge;
            }
            continue;
        }
        out.push(d);
    }
    out.sort_by(|a, b| (a.kind, a.name.to_lowercase()).cmp(&(b.kind, b.name.to_lowercase())));
    out
}
