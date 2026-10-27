//! Host virtual input devices. A gamepad instance owns one OS device; dropping
//! it destroys that device and releases all inputs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GamepadReport {
    pub buttons: u16,
    /// HID hat: 0..=7 clockwise from up, or 15 for centered.
    pub hat: u8,
    /// Signed X/Y/RX/RY positions.
    pub axes: [i8; 4],
}

impl Default for GamepadReport {
    fn default() -> Self {
        Self {
            buttons: 0,
            hat: 15,
            axes: [0; 4],
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::GamepadReport;
    use evdev::{
        AbsInfo, AbsoluteAxisCode as Axis, AttributeSet, EventType, InputEvent, KeyCode,
        UinputAbsSetup, uinput::VirtualDevice,
    };
    use std::io;
    const KEYS: [KeyCode; 16] = [
        KeyCode::BTN_SOUTH,
        KeyCode::BTN_EAST,
        KeyCode::BTN_WEST,
        KeyCode::BTN_NORTH,
        KeyCode::BTN_TL,
        KeyCode::BTN_TR,
        KeyCode::BTN_TL2,
        KeyCode::BTN_TR2,
        KeyCode::BTN_SELECT,
        KeyCode::BTN_START,
        KeyCode::BTN_MODE,
        KeyCode::BTN_THUMBL,
        KeyCode::BTN_THUMBR,
        KeyCode::BTN_TRIGGER_HAPPY1,
        KeyCode::BTN_TRIGGER_HAPPY2,
        KeyCode::BTN_TRIGGER_HAPPY3,
    ];
    const AXES: [Axis; 4] = [Axis::ABS_X, Axis::ABS_Y, Axis::ABS_RX, Axis::ABS_RY];

    pub struct Gamepad {
        device: VirtualDevice,
    }
    impl Gamepad {
        pub fn create(name: &str) -> io::Result<Self> {
            if name.is_empty() || name.len() > 70 || name.as_bytes().contains(&0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "gamepad name must be 1–70 bytes without NUL",
                ));
            }
            let mut keys = AttributeSet::<KeyCode>::new();
            for key in KEYS {
                keys.insert(key);
            }
            let mut builder = VirtualDevice::builder()?.name(name).with_keys(&keys)?;
            for axis in AXES {
                builder = builder.with_absolute_axis(&UinputAbsSetup::new(
                    axis,
                    AbsInfo::new(0, -127, 127, 0, 0, 0),
                ))?;
            }
            for axis in [Axis::ABS_HAT0X, Axis::ABS_HAT0Y] {
                builder = builder.with_absolute_axis(&UinputAbsSetup::new(
                    axis,
                    AbsInfo::new(0, -1, 1, 0, 0, 0),
                ))?;
            }
            Ok(Self {
                device: builder.build()?,
            })
        }
        pub fn send(&mut self, report: GamepadReport) -> io::Result<()> {
            let hat = match report.hat {
                0 => (0, -1),
                1 => (1, -1),
                2 => (1, 0),
                3 => (1, 1),
                4 => (0, 1),
                5 => (-1, 1),
                6 => (-1, 0),
                7 => (-1, -1),
                15 => (0, 0),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid HID hat",
                    ));
                }
            };
            let mut events = Vec::with_capacity(22);
            for (i, key) in KEYS.iter().enumerate() {
                events.push(InputEvent::new(
                    EventType::KEY.0,
                    key.0,
                    i32::from((report.buttons & (1 << i)) != 0),
                ));
            }
            for (axis, value) in AXES.iter().zip(report.axes) {
                events.push(InputEvent::new(
                    EventType::ABSOLUTE.0,
                    axis.0,
                    i32::from(value).max(-127),
                ));
            }
            events.push(InputEvent::new(
                EventType::ABSOLUTE.0,
                Axis::ABS_HAT0X.0,
                hat.0,
            ));
            events.push(InputEvent::new(
                EventType::ABSOLUTE.0,
                Axis::ABS_HAT0Y.0,
                hat.1,
            ));
            self.device.emit(&events)
        }
    }
    fn usage(code: u8) -> Option<KeyCode> {
        let keys = [
            KeyCode::KEY_A,
            KeyCode::KEY_B,
            KeyCode::KEY_C,
            KeyCode::KEY_D,
            KeyCode::KEY_E,
            KeyCode::KEY_F,
            KeyCode::KEY_G,
            KeyCode::KEY_H,
            KeyCode::KEY_I,
            KeyCode::KEY_J,
            KeyCode::KEY_K,
            KeyCode::KEY_L,
            KeyCode::KEY_M,
            KeyCode::KEY_N,
            KeyCode::KEY_O,
            KeyCode::KEY_P,
            KeyCode::KEY_Q,
            KeyCode::KEY_R,
            KeyCode::KEY_S,
            KeyCode::KEY_T,
            KeyCode::KEY_U,
            KeyCode::KEY_V,
            KeyCode::KEY_W,
            KeyCode::KEY_X,
            KeyCode::KEY_Y,
            KeyCode::KEY_Z,
            KeyCode::KEY_1,
            KeyCode::KEY_2,
            KeyCode::KEY_3,
            KeyCode::KEY_4,
            KeyCode::KEY_5,
            KeyCode::KEY_6,
            KeyCode::KEY_7,
            KeyCode::KEY_8,
            KeyCode::KEY_9,
            KeyCode::KEY_0,
            KeyCode::KEY_ENTER,
            KeyCode::KEY_ESC,
            KeyCode::KEY_BACKSPACE,
            KeyCode::KEY_TAB,
            KeyCode::KEY_SPACE,
            KeyCode::KEY_MINUS,
            KeyCode::KEY_EQUAL,
            KeyCode::KEY_LEFTBRACE,
            KeyCode::KEY_RIGHTBRACE,
            KeyCode::KEY_BACKSLASH,
            KeyCode::KEY_102ND,
            KeyCode::KEY_SEMICOLON,
            KeyCode::KEY_APOSTROPHE,
            KeyCode::KEY_GRAVE,
            KeyCode::KEY_COMMA,
            KeyCode::KEY_DOT,
            KeyCode::KEY_SLASH,
            KeyCode::KEY_CAPSLOCK,
            KeyCode::KEY_F1,
            KeyCode::KEY_F2,
            KeyCode::KEY_F3,
            KeyCode::KEY_F4,
            KeyCode::KEY_F5,
            KeyCode::KEY_F6,
            KeyCode::KEY_F7,
            KeyCode::KEY_F8,
            KeyCode::KEY_F9,
            KeyCode::KEY_F10,
            KeyCode::KEY_F11,
            KeyCode::KEY_F12,
            KeyCode::KEY_SYSRQ,
            KeyCode::KEY_SCROLLLOCK,
            KeyCode::KEY_PAUSE,
            KeyCode::KEY_INSERT,
            KeyCode::KEY_HOME,
            KeyCode::KEY_PAGEUP,
            KeyCode::KEY_DELETE,
            KeyCode::KEY_END,
            KeyCode::KEY_PAGEDOWN,
            KeyCode::KEY_RIGHT,
            KeyCode::KEY_LEFT,
            KeyCode::KEY_DOWN,
            KeyCode::KEY_UP,
        ];
        if (4..=82).contains(&code) {
            return keys.get((code - 4) as usize).copied();
        }
        match code {
            128 => Some(KeyCode::KEY_VOLUMEUP),
            129 => Some(KeyCode::KEY_VOLUMEDOWN),
            _ => None,
        }
    }
    const MODIFIERS: [KeyCode; 8] = [
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_LEFTMETA,
        KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_RIGHTALT,
        KeyCode::KEY_RIGHTMETA,
    ];
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn hid_keyboard_usage_maps_to_linux_keys() {
            assert_eq!(usage(4), Some(KeyCode::KEY_A));
            assert_eq!(usage(29), Some(KeyCode::KEY_Z));
            assert_eq!(usage(40), Some(KeyCode::KEY_ENTER));
            assert_eq!(usage(44), Some(KeyCode::KEY_SPACE));
            assert_eq!(usage(79), Some(KeyCode::KEY_RIGHT));
            assert_eq!(usage(82), Some(KeyCode::KEY_UP));
            assert_eq!(usage(128), Some(KeyCode::KEY_VOLUMEUP));
            assert_eq!(usage(129), Some(KeyCode::KEY_VOLUMEDOWN));
            assert_eq!(usage(0), None);
        }
    }
    pub struct Keyboard {
        device: VirtualDevice,
        held: std::collections::BTreeSet<KeyCode>,
    }
    impl Keyboard {
        pub fn create(name: &str) -> io::Result<Self> {
            let mut keys = AttributeSet::<KeyCode>::new();
            for code in 4..=129 {
                if let Some(key) = usage(code) {
                    keys.insert(key);
                }
            }
            for key in MODIFIERS {
                keys.insert(key);
            }
            Ok(Self {
                device: VirtualDevice::builder()?
                    .name(name)
                    .with_keys(&keys)?
                    .build()?,
                held: Default::default(),
            })
        }
        pub fn send(&mut self, report: [u8; 8]) -> io::Result<()> {
            let mut next = std::collections::BTreeSet::new();
            for (i, key) in MODIFIERS.iter().enumerate() {
                if report[0] & (1 << i) != 0 {
                    next.insert(*key);
                }
            }
            for &code in &report[2..] {
                if let Some(key) = usage(code) {
                    next.insert(key);
                }
            }
            let mut events = Vec::new();
            for key in self.held.difference(&next) {
                events.push(InputEvent::new(EventType::KEY.0, key.0, 0));
            }
            for key in next.difference(&self.held) {
                events.push(InputEvent::new(EventType::KEY.0, key.0, 1));
            }
            self.device.emit(&events)?;
            self.held = next;
            Ok(())
        }
    }
    pub struct Mouse {
        device: VirtualDevice,
    }
    impl Mouse {
        pub fn create(name: &str) -> io::Result<Self> {
            use evdev::RelativeAxisCode as Rel;
            let mut keys = AttributeSet::<KeyCode>::new();
            for key in [KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT, KeyCode::BTN_MIDDLE] {
                keys.insert(key);
            }
            let mut rel = AttributeSet::<Rel>::new();
            for axis in [Rel::REL_X, Rel::REL_Y, Rel::REL_WHEEL] {
                rel.insert(axis);
            }
            Ok(Self {
                device: VirtualDevice::builder()?
                    .name(name)
                    .with_keys(&keys)?
                    .with_relative_axes(&rel)?
                    .build()?,
            })
        }
        pub fn send(&mut self, buttons: u8, dx: i8, dy: i8, wheel: i8) -> io::Result<()> {
            use evdev::RelativeAxisCode as Rel;
            let mut events = Vec::new();
            for (i, key) in [KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT, KeyCode::BTN_MIDDLE]
                .iter()
                .enumerate()
            {
                events.push(InputEvent::new(
                    EventType::KEY.0,
                    key.0,
                    i32::from(buttons & (1 << i) != 0),
                ));
            }
            for (axis, value) in [(Rel::REL_X, dx), (Rel::REL_Y, dy), (Rel::REL_WHEEL, wheel)] {
                if value != 0 {
                    events.push(InputEvent::new(
                        EventType::RELATIVE.0,
                        axis.0,
                        i32::from(value),
                    ));
                }
            }
            self.device.emit(&events)
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::{Gamepad, Keyboard, Mouse};

#[cfg(all(test, target_os = "linux"))]
mod hardware_tests {
    use super::*;
    use evdev::{Device, KeyCode};

    #[test]
    #[ignore = "requires writable /dev/uinput and readable generated input nodes"]
    fn independent_gamepads_keep_separate_button_states() {
        let mut first = Gamepad::create("OpenWASD test first").unwrap();
        let mut second = Gamepad::create("OpenWASD test second").unwrap();
        let open_by_name = |name: &str| -> Device {
            for _ in 0..50 {
                if let Some((_, device)) =
                    evdev::enumerate().find(|(_, dev)| dev.name() == Some(name))
                {
                    return device;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            panic!("virtual gamepad not readable: {name}");
        };
        let input_first = open_by_name("OpenWASD test first");
        let input_second = open_by_name("OpenWASD test second");
        first
            .send(GamepadReport {
                buttons: 1,
                ..Default::default()
            })
            .unwrap();
        second
            .send(GamepadReport {
                buttons: 2,
                ..Default::default()
            })
            .unwrap();
        let a = input_first.get_key_state().unwrap();
        let b = input_second.get_key_state().unwrap();
        assert!(a.contains(KeyCode::BTN_SOUTH));
        assert!(!a.contains(KeyCode::BTN_EAST));
        assert!(b.contains(KeyCode::BTN_EAST));
        assert!(!b.contains(KeyCode::BTN_SOUTH));
        first.send(GamepadReport::default()).unwrap();
        assert!(
            !input_first
                .get_key_state()
                .unwrap()
                .contains(KeyCode::BTN_SOUTH)
        );
        assert!(
            input_second
                .get_key_state()
                .unwrap()
                .contains(KeyCode::BTN_EAST)
        );
    }
}
