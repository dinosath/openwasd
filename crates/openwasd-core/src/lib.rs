//! Platform-independent product model for OpenWASD.
//!
//! Platform adapters should translate native events into this model and turn
//! `Action`s into native output. Keeping the state here makes profiles portable
//! between desktop and companion applications.

use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Transport {
    Bluetooth,
    Usb,
    Wifi,
}

impl Transport {
    pub const ALL: [Self; 3] = [Self::Bluetooth, Self::Usb, Self::Wifi];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Bluetooth => "Bluetooth",
            Self::Usb => "USB",
            Self::Wifi => "Wi-Fi",
        }
    }

    pub const fn glyph(&self) -> &'static str {
        match self {
            Self::Bluetooth => "BT",
            Self::Usb => "USB",
            Self::Wifi => "Wi-Fi",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceKind {
    Gamepad,
    Keyboard,
    Mouse,
    Companion,
}

impl DeviceKind {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Gamepad => "Gamepad",
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse",
            Self::Companion => "Companion",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum VirtualDeviceKind {
    Gamepad,
    Keyboard,
    Mouse,
}

impl VirtualDeviceKind {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Gamepad => "Gamepad",
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VirtualDevice {
    pub id: String,
    pub name: String,
    pub kind: VirtualDeviceKind,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceMapping {
    pub virtual_device_id: String,
    pub actual_device_id: String,
}

/// A small transport-neutral packet used by Bluetooth platform adapters.
/// Adapters are responsible for translating OS/HID reports into these bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BluetoothPacket(pub Vec<u8>);

#[derive(Debug)]
pub enum BluetoothError {
    NotFound(String),
    PermissionDenied,
    ConnectionFailed(String),
    Disconnected,
}

pub trait BluetoothConnector: Send + Sync {
    fn connect(&self, device_id: &str) -> Result<Box<dyn BluetoothLink>, BluetoothError>;
}

pub trait BluetoothLink: Send {
    fn send(&mut self, packet: BluetoothPacket) -> Result<(), BluetoothError>;
    fn receive(&mut self) -> Result<Option<BluetoothPacket>, BluetoothError>;
    fn close(&mut self);
}

/// Owns the input/output queues for one Bluetooth link. A native adapter can
/// bridge its event loop to these queues while the mapping engine stays pure.
pub struct BluetoothSession {
    outgoing: Sender<BluetoothPacket>,
    incoming: Receiver<BluetoothPacket>,
}

impl BluetoothSession {
    pub fn connect(
        connector: &dyn BluetoothConnector,
        device_id: &str,
    ) -> Result<Self, BluetoothError> {
        let mut link = connector.connect(device_id)?;
        let (outgoing_tx, outgoing_rx) = mpsc::channel();
        let (incoming_tx, incoming_rx) = mpsc::channel();

        std::thread::spawn(move || {
            while let Ok(packet) = outgoing_rx.recv() {
                if link.send(packet).is_err() {
                    break;
                }
                match link.receive() {
                    Ok(Some(packet)) => {
                        if incoming_tx.send(packet).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
            link.close();
        });

        Ok(Self {
            outgoing: outgoing_tx,
            incoming: incoming_rx,
        })
    }

    pub fn send(&self, packet: BluetoothPacket) -> Result<(), BluetoothError> {
        self.outgoing
            .send(packet)
            .map_err(|_| BluetoothError::Disconnected)
    }

    pub fn try_receive(&self) -> Option<BluetoothPacket> {
        self.incoming.try_recv().ok()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub transport: Transport,
    pub connected: bool,
    pub battery_percent: Option<u8>,
    pub latency_ms: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Input {
    Button(String),
    Axis(String),
    Key(String),
    Touch(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Button(String),
    Axis(String),
    Key(String),
    Mouse(String),
    Macro(String),
    Layer(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mapping {
    pub input: Input,
    pub action: Action,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub activation: String,
    pub mappings: Vec<Mapping>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub app_hint: Option<String>,
    pub layers: Vec<Layer>,
}

impl Profile {
    pub fn starter() -> Self {
        Self {
            id: "profile.default".into(),
            name: "Desktop baseline".into(),
            app_hint: Some("All applications".into()),
            layers: vec![Layer {
                name: "Base layer".into(),
                activation: "Always on".into(),
                mappings: vec![
                    Mapping {
                        input: Input::Button("South".into()),
                        action: Action::Button("South".into()),
                        enabled: true,
                    },
                    Mapping {
                        input: Input::Button("East".into()),
                        action: Action::Button("East".into()),
                        enabled: true,
                    },
                    Mapping {
                        input: Input::Button("Left stick".into()),
                        action: Action::Axis("Left stick".into()),
                        enabled: true,
                    },
                ],
            }],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EngineState {
    pub devices: Vec<Device>,
    pub virtual_devices: Vec<VirtualDevice>,
    pub mappings: Vec<DeviceMapping>,
    pub profiles: Vec<Profile>,
    pub active_profile: usize,
    pub active_layer: usize,
}

impl EngineState {
    pub fn demo() -> Self {
        Self {
            devices: vec![
                Device {
                    id: "g8".into(),
                    name: "GameSir G8 Galileo".into(),
                    kind: DeviceKind::Gamepad,
                    transport: Transport::Usb,
                    connected: true,
                    battery_percent: None,
                    latency_ms: Some(2),
                },
                Device {
                    id: "phone".into(),
                    name: "Pixel companion".into(),
                    kind: DeviceKind::Companion,
                    transport: Transport::Wifi,
                    connected: true,
                    battery_percent: Some(82),
                    latency_ms: Some(14),
                },
                Device {
                    id: "kbd".into(),
                    name: "Keychron K2".into(),
                    kind: DeviceKind::Keyboard,
                    transport: Transport::Bluetooth,
                    connected: true,
                    battery_percent: Some(68),
                    latency_ms: Some(7),
                },
            ],
            virtual_devices: vec![
                VirtualDevice {
                    id: "virtual-gamepad".into(),
                    name: "OpenWASD Gamepad".into(),
                    kind: VirtualDeviceKind::Gamepad,
                    enabled: true,
                },
                VirtualDevice {
                    id: "virtual-keyboard".into(),
                    name: "OpenWASD Keyboard".into(),
                    kind: VirtualDeviceKind::Keyboard,
                    enabled: true,
                },
                VirtualDevice {
                    id: "virtual-mouse".into(),
                    name: "OpenWASD Mouse".into(),
                    kind: VirtualDeviceKind::Mouse,
                    enabled: false,
                },
            ],
            mappings: vec![
                DeviceMapping {
                    virtual_device_id: "virtual-gamepad".into(),
                    actual_device_id: "g8".into(),
                },
                DeviceMapping {
                    virtual_device_id: "virtual-keyboard".into(),
                    actual_device_id: "kbd".into(),
                },
            ],
            profiles: vec![Profile::starter()],
            active_profile: 0,
            active_layer: 0,
        }
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        self.profiles.get(self.active_profile)
    }

    pub fn map_virtual_device(&mut self, virtual_device_id: &str, actual_device_id: &str) -> bool {
        if !self
            .virtual_devices
            .iter()
            .any(|device| device.id == virtual_device_id)
            || !self
                .devices
                .iter()
                .any(|device| device.id == actual_device_id)
        {
            return false;
        }
        self.mappings
            .retain(|mapping| mapping.virtual_device_id != virtual_device_id);
        self.mappings.push(DeviceMapping {
            virtual_device_id: virtual_device_id.into(),
            actual_device_id: actual_device_id.into(),
        });
        true
    }

    pub fn unmap_virtual_device(&mut self, virtual_device_id: &str) {
        self.mappings
            .retain(|mapping| mapping.virtual_device_id != virtual_device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_profile_has_a_safe_base_layer() {
        let profile = Profile::starter();
        assert_eq!(profile.layers.len(), 1);
        assert!(
            profile.layers[0]
                .mappings
                .iter()
                .all(|mapping| mapping.enabled)
        );
    }

    #[test]
    fn demo_state_exposes_all_primary_transports() {
        let state = EngineState::demo();
        for transport in Transport::ALL {
            assert!(
                state
                    .devices
                    .iter()
                    .any(|device| device.transport == transport)
            );
        }
    }

    #[test]
    fn virtual_devices_can_be_routed_to_actual_devices() {
        let mut state = EngineState::demo();
        assert!(state.map_virtual_device("virtual-mouse", "kbd"));
        assert_eq!(
            state
                .mappings
                .iter()
                .filter(|m| m.virtual_device_id == "virtual-mouse")
                .count(),
            1
        );
        state.unmap_virtual_device("virtual-mouse");
        assert!(
            !state
                .mappings
                .iter()
                .any(|m| m.virtual_device_id == "virtual-mouse")
        );
    }
}
