//! Bluetooth Low Energy: GATT Battery Service (0x180F) → Battery Level (0x2A19).
//! Solo se consultan dispositivos emparejados y CONECTADOS (no se fuerza ninguna conexión).

use super::{Charge, Device, Kind};
use windows::core::{Result, HSTRING};
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicUuids, GattCommunicationStatus, GattServiceUuids,
};
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Storage::Streams::DataReader;

pub fn collect(out: &mut Vec<Device>) {
    let _ = collect_inner(out);
}

fn collect_inner(out: &mut Vec<Device>) -> Result<()> {
    let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
    let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.get()?;
    for i in 0..infos.Size()? {
        let Ok(info) = infos.GetAt(i) else { continue };
        let Ok(id) = info.Id() else { continue };
        if let Ok(Some(d)) = read_device(&id) {
            out.push(d);
        }
    }
    Ok(())
}

fn read_device(id: &HSTRING) -> Result<Option<Device>> {
    let dev = BluetoothLEDevice::FromIdAsync(id)?.get()?;
    let result: Result<Option<Device>> = (|| {
        if dev.ConnectionStatus()? != BluetoothConnectionStatus::Connected {
            return Ok(None);
        }
        let name = dev.Name()?.to_string().trim().to_string();
        if name.is_empty() {
            return Ok(None);
        }
        let Some(level) = read_battery(&dev)? else { return Ok(None) };

        let kind = match dev.Appearance().and_then(|a| a.RawValue()) {
            Ok(raw) => kind_from_appearance(raw),
            Err(_) => Kind::Other,
        };
        let kind = if kind == Kind::Other { Kind::from_name(&name) } else { kind };
        Ok(Some(Device { name, kind, level: Some(level), charge: Charge::Unknown, priority: 30, asleep: None }))
    })();
    // Libera la conexión GATT de inmediato (sin esperar al recolector de WinRT).
    let _ = dev.Close();
    result
}

fn read_battery(dev: &BluetoothLEDevice) -> Result<Option<u8>> {
    let services = dev
        .GetGattServicesForUuidWithCacheModeAsync(GattServiceUuids::Battery()?, BluetoothCacheMode::Cached)?
        .get()?;
    if services.Status()? != GattCommunicationStatus::Success {
        return Ok(None);
    }
    let list = services.Services()?;
    let mut level = None;
    for i in 0..list.Size()? {
        let service = list.GetAt(i)?;
        let value = (|| -> Result<Option<u8>> {
            let chars = service
                .GetCharacteristicsForUuidWithCacheModeAsync(
                    GattCharacteristicUuids::BatteryLevel()?,
                    BluetoothCacheMode::Cached,
                )?
                .get()?;
            if chars.Status()? != GattCommunicationStatus::Success {
                return Ok(None);
            }
            let chars = chars.Characteristics()?;
            if chars.Size()? == 0 {
                return Ok(None);
            }
            // Lectura no cacheada: valor real del dispositivo.
            let read = chars.GetAt(0)?.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?.get()?;
            if read.Status()? != GattCommunicationStatus::Success {
                return Ok(None);
            }
            let reader = DataReader::FromBuffer(&read.Value()?)?;
            let v = reader.ReadByte()?;
            Ok((v <= 100).then_some(v))
        })();
        let _ = service.Close();
        if let Ok(Some(v)) = value {
            level = Some(v);
            break;
        }
    }
    Ok(level)
}

/// Apariencia GAP: categoría = bits 15..6, subcategoría = bits 5..0.
fn kind_from_appearance(raw: u16) -> Kind {
    let (cat, sub) = (raw >> 6, raw & 0x3F);
    match cat {
        0x0F => match sub {
            1 => Kind::Keyboard,
            2 | 5 | 6 => Kind::Mouse,
            3 | 4 => Kind::Gamepad,
            _ => Kind::Other,
        },
        0x25 => Kind::Headset, // Wearable Audio Device
        0x21 => Kind::Headset, // Audio Sink
        _ => Kind::Other,
    }
}
