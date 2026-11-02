//! USB device inventory. Enumeration is distinct from an OpenWASD connection:
//! a plugged-in device is not necessarily able to exchange companion input.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDevice {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inventory {
    Available(Vec<UsbDevice>),
    Unavailable(String),
}

/// Lists USB devices connected to this host. This does not open a USB session.
pub fn connected_devices() -> Inventory {
    #[cfg(target_os = "linux")]
    {
        match enumerate_usb() {
            Ok(devices) => Inventory::Available(devices),
            Err(error) => Inventory::Unavailable(format!("USB enumeration failed: {error}")),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Inventory::Unavailable("USB enumeration is not available on this platform yet.".into())
    }
}

#[cfg(target_os = "linux")]
fn enumerate_usb() -> Result<Vec<UsbDevice>, nusb::Error> {
    use nusb::MaybeFuture;
    let mut devices = Vec::new();
    for device in nusb::list_devices().wait()? {
        // Hubs are infrastructure, not devices users can connect as companions.
        if device.class() == 0x09 {
            continue;
        }
        let vendor_id = device.vendor_id();
        let product_id = device.product_id();
        let product = device.product_string();
        let manufacturer = device.manufacturer_string();
        let name = match (manufacturer, product) {
            (Some(m), Some(p)) if !p.to_lowercase().contains(&m.to_lowercase()) => {
                format!("{m} {p}")
            }
            (_, Some(p)) => p.to_owned(),
            (Some(m), None) => m.to_owned(),
            _ => format!("USB device {vendor_id:04x}:{product_id:04x}"),
        };
        devices.push(UsbDevice {
            id: format!(
                "usb:{}",
                device
                    .sysfs_path()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            name,
            vendor_id,
            product_id,
            serial: device.serial_number().map(str::to_owned),
        });
    }
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(devices)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub mod bridge;
