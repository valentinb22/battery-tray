# Battery Tray

Nivel de batería de mouse, teclado y auriculares inalámbricos en la bandeja del sistema de
Windows 10/11. Binario nativo de ~340 KB sin runtime.

**Medido en este equipo (Windows 10 22H2):** 1,5 MB de working set en reposo (4,3 MB privados),
4 hilos, **0 ms de CPU en 60 s de reposo**. Un escaneo completo con auriculares HyperX y
teclado Compx tarda ~70 ms (~2 s si el teclado está dormido y no contesta), en un hilo aparte.

## 1. Arquitectura

**Rust + Win32 directo + WinRT solo para BLE.**

- **Rust** genera código máquina sin GC ni runtime. La biblioteca estándar no pesa casi nada y
  con `panic = "abort"` + LTO el `.exe` queda en ~340 KB.
- **Win32 declarado a mano** (`src/ffi.rs`): solo las ~70 funciones que se usan, enlazadas con las
  import libs de `windows-targets`. No hay framework de UI ni capa de abstracción.
- **WinRT** (crate `windows`) se usa únicamente para el GATT Battery Service de Bluetooth LE,
  que no tiene equivalente en Win32.
- **CPU ~0 % en reposo:** el hilo de UI duerme dentro de `GetMessageW`. Lo despiertan solo estos eventos:
  - `WM_DEVICECHANGE` (PnP/Bluetooth), con un debounce de 3 s.
  - `WM_POWERBROADCAST`: en suspensión se paran los timers; al reanudar se reescanea a los 8 s.
  - Un timer **coalescible** (`SetCoalescableTimer`, ±10 %) cada N minutos (5 por defecto,
    configurable de 1 a 30 desde el menú).
  - Además, el proceso pide **EcoQoS** (`SetProcessInformation`).
- **El escaneo corre en un hilo efímero** (pila de 512 KB). Devuelve un `Vec<Device>` por `PostMessageW`
  y termina. Después se recorta el working set.
- **Un icono por periférico inalámbrico** (un `NOTIFYICONDATA` por dispositivo). Su `uID` es un
  hash fijo del nombre (FNV-1a), así Windows recuerda si cada icono va en la barra o en el panel
  oculto aunque cambie el orden en que se detectan.
- **Inicio con Windows:** al activarlo, el `.exe` se copia a `%LOCALAPPDATA%\BatteryTray\` y la
  clave `Run` apunta a esa copia. Si se ejecuta una compilación más nueva desde otra carpeta, la
  copia instalada se actualiza sola al arrancar.
- **Auriculares HyperX sin saltos:** en cada escaneo se toman 5 muestras (mediana de la tensión y
  estado mayoritario). Mientras el estado no cambia, el % no sube descargando ni baja cargando.
  Los iconos se crean y se quitan a medida que los dispositivos aparecen o desaparecen. Los
  periféricos con cable no tienen icono (solo figuran en el menú). Si no hay ninguno inalámbrico,
  queda un único icono genérico para poder abrir el menú.
- **Animación de carga sin coste en reposo:** cada icono que carga tiene dos fotogramas
  pre-renderizados (con y sin rayo) que se alternan cada 0,7 s con `NIM_MODIFY`. El temporizador
  solo existe mientras algún dispositivo está cargando, y se puede desactivar desde el menú
  (*Animar icono al cargar*; entonces el rayo queda fijo).

### Detección de batería (de mayor a menor prioridad; los duplicados se fusionan por nombre)

| Fuente | API | Cubre |
|---|---|---|
| HyperX (0951:170B) | `HidD_SetFeature`/`HidD_GetFeature` 0xFF en MI_03 Col03 (sondeo hasta respuesta fresca); estado en byte 9, mV en bytes 11-12 → curva Li-Po | Cloud Stinger Core Wireless (+ 7.1). Con **Cargando/Descargando/Lleno**. Probado en hardware real |
| Compx (3554:FA09) | Informe 0x13 por la colección 0xFF02: `13 4A…` o `13 05 01…` con checksum; % en byte 5/6, estado 0x10 = cable | Teclados 2.4 GHz AULA, Redragon, MCHOSE, LeoBog… Probado en hardware real. Si el teclado está dormido, se conserva la última lectura 1 h. El modelo sale del código de identidad (`13 05 01…`) y la tabla `MODELS` de `compx.rs` (p. ej. `…CD` = AULA F75) |
| Logitech HID++ 2.0 | `CreateFile`/`ReadFile` superpuestos sobre la colección HID 0xFF00/0x0002 (o 0xFF43/0x0202 por BT); features 0x1004, 0x1000 y 0x1001 (voltaje → %) | Receptores Unifying, Bolt y Lightspeed, cable USB, Bluetooth. Incluye estado **Cargando/Descargando** |
| BLE GATT | `BluetoothLEDevice` → servicio 0x180F → característica 0x2A19 (lectura sin caché) | Cualquier periférico BLE emparejado **y conectado** |
| HID estándar | `HidP_GetValueCaps`/`HidP_GetUsageValue`, Usage Page 0x06 / Usage 0x20 *Battery Strength* | Dongles 2.4 GHz que siguen el estándar HID |
| PnP | `CM_Get_DevNode_PropertyW` con `{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2` | Bluetooth clásico y BLE: es el mismo valor que muestra Configuración → Bluetooth |

Si un dispositivo no reporta batería, simplemente no aparece. Todas las fuentes capturan sus
errores, todas las E/S HID tienen timeout con `CancelIo`, y todos los handles se liberan con RAII
(`HandleGuard`, `Preparsed`, `Close()` en WinRT).

## 2. Estructura

```
Batery/
├── Cargo.toml            # dependencias y perfil release optimizado
├── README.md
└── src/
    ├── main.rs           # instancia única, DPI, MTA, EcoQoS, ventana oculta, bucle de mensajes
    ├── ffi.rs            # declaraciones Win32 (structs, constantes, funciones)
    ├── app.rs            # wndproc, estado, menú, timers, suspensión, alertas
    ├── tray.rs           # Shell_NotifyIconW (icono, tooltip, notificación)
    ├── icon.rs           # icono dinámico dibujado en memoria (DIB 32 bpp + alfa)
    ├── config.rs         # HKCU\Software\BatteryTray y clave Run (inicio con Windows)
    └── devices/
        ├── mod.rs        # modelo Device, fusión y deduplicación
        ├── pnp.rs        # propiedad PnP de batería Bluetooth
        ├── ble.rs        # GATT Battery Service (WinRT)
        ├── hid.rs        # enumeración HID + Battery Strength estándar
        ├── hidpp.rs      # protocolo Logitech HID++ 2.0
        ├── hyperx.rs     # HyperX Cloud Stinger Core Wireless (0951:170B)
        └── compx.rs      # receptores de teclado Compx (3554:FA09)
examples/                 # herramientas de investigación (no forman parte de la app)
    ├── probe.rs          # vuelca colecciones, informes Feature/Input de un VID (solo lectura)
    ├── watch.rs          # muestrea informes Feature candidatos
    ├── hyperx.rs         # prueba aislada del protocolo HyperX
    └── compx.rs          # prueba aislada del protocolo Compx
```

## 3. Interfaz

- **Iconos:** uno por dispositivo, con el símbolo arriba (teclado, auriculares o mouse; batería
  para mandos u otros) y el porcentaje en pequeño abajo. Se dibujan con suavizado y son nítidos
  a 16, 20, 24 y 32 px.
  - El número se pone en rojo por debajo del 20 %, y todo se adapta al tema claro u oscuro de la barra de tareas.
  - Mientras carga, aparece un rayo verde arriba a la derecha que parpadea.
  - Sin dispositivos inalámbricos, queda un icono de batería con `--` en gris.
  - Vista previa ampliada: `cargo run --release --example icons -- iconos.bmp`.
- **Tooltip** de cada icono: nombre, porcentaje y estado (por ejemplo `AULA F75` / `100% · Descargando`).
- **Mouse con cable:** los mouse USB con cable aparecen en el menú como `Con cable`, sin
  porcentaje. No tienen icono ni cuentan para las alertas. Los receptores inalámbricos se descartan por
  fabricante o por nombre ("Receiver", "Wireless", "2.4G"…).
- **Menú (clic izquierdo o derecho):**
  - Dispositivos con tipo, nombre, % y estado.
  - Hora de la última actualización y *Actualizar ahora*.
  - *Intervalo de consulta* y *Iniciar con Windows* (clave `HKCU\…\Run`).
  - *Salir*.
- **Alerta:** una notificación silenciosa (`NIIF_NOSOUND`, respeta el modo "no molestar") cuando un
  dispositivo baja del 15 %. No se repite hasta que vuelve a superar el 20 % o se conecta a cargar.
  El umbral se cambia en `HKCU\Software\BatteryTray\LowThreshold` (DWORD).

## 4. Compilación

La toolchain GNU no necesita Visual Studio. En este equipo está instalada en `D:\Rust`.

```bash
rustup toolchain install stable-x86_64-pc-windows-gnu
```

```bash
cargo build --release
```

El ejecutable queda en `target\release\battery-tray.exe`.

Alternativa con MSVC (requiere Visual Studio Build Tools con "Desarrollo para escritorio con C++"):

```bash
cargo build --release --target x86_64-pc-windows-msvc
```

Para enlazar el runtime de C de forma estática con MSVC (y no depender de `vcruntime140.dll`):

```bash
set RUSTFLAGS=-C target-feature=+crt-static && cargo build --release --target x86_64-pc-windows-msvc
```

Diagnóstico (lista lo que detecta y sale, sin icono de bandeja):

```bash
target/release/battery-tray.exe --list | cat
```

## Limitaciones conocidas

- Otros fabricantes con protocolos propietarios (Razer, Corsair, SteelSeries, otros modelos HyperX)
  necesitan un módulo propio, como `hyperx.rs` o `compx.rs`. Para investigarlos se puede usar
  `cargo run --release --example probe -- <VID>`.
- HyperX no informa porcentaje, solo tensión: el % es una estimación por curva Li-Po (NGENUITY
  solo muestra niveles aproximados).
- Un teclado Compx dormido no responde. Si la app arranca con el teclado dormido, no aparece
  hasta que un escaneo lo encuentre despierto (cada N minutos o al pulsar *Actualizar ahora*).

Protocolos documentados por los proyectos open source
[nogenuity](https://github.com/esdedev/nogenuity),
[HyperBatteryMonitor](https://github.com/davidsantanaalves/HyperBatteryMonitor),
[volt](https://github.com/Comasss/volt),
[omarchy-gadget-batteries](https://github.com/xii69/omarchy-gadget-batteries) y
[BatteryTray](https://github.com/leesy03069-netizen/BatteryTray).
