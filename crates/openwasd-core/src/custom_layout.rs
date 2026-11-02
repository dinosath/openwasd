//! Portable button mappings for a custom control surface.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A button's output. Keyboard usages follow the USB HID keyboard page.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ControlAction {
    Keyboard(u8),
    GamepadButton(u8),
    /// D-pad direction: up, right, down, left (0 through 3).
    GamepadHat(u8),
    MouseButton(u8),
    BluetoothSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomControl {
    /// Stable identifier for controls in factory layouts.
    #[serde(default)]
    pub builtin_id: Option<String>,
    pub label: String,
    pub action: ControlAction,
    /// Optional text emitted as keystrokes when this control is pressed.
    #[serde(default)]
    pub text_output: Option<String>,
    /// Normalized key name supplied by the platform input adapter.
    #[serde(default)]
    pub hardware_key: Option<String>,
    /// Whether the button's label is shown on the surface.
    #[serde(default = "default_true")]
    pub show_text: bool,
    /// Whether an action icon is shown on the surface.
    #[serde(default)]
    pub show_icon: bool,
    /// Custom symbol or emoji; `None` uses the action's default icon.
    #[serde(default)]
    pub custom_icon: Option<String>,
}

/// A hardware input mapping that is active with a custom layout but is not
/// represented by a visible surface control.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareMapping {
    /// Normalized input name supplied by the platform input adapter.
    pub hardware_key: String,
    pub action: ControlAction,
}

fn default_true() -> bool {
    true
}

/// Tracks sources independently so overlapping mappings don't release one another.
#[derive(Clone, Debug, Default)]
pub struct ActiveControls {
    pressed: BTreeMap<String, ControlAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlReports {
    pub keyboard: [u8; 8],
    pub gamepad: [u8; 7],
    pub mouse_buttons: u8,
}

impl ActiveControls {
    pub fn is_empty(&self) -> bool {
        self.pressed.is_empty()
    }

    pub fn press(&mut self, source: String, action: ControlAction) {
        if action != ControlAction::BluetoothSettings {
            self.pressed.insert(source, action);
        }
    }

    pub fn release(&mut self, source: &str) {
        self.pressed.remove(source);
    }

    pub fn clear(&mut self) {
        self.pressed.clear();
    }

    pub fn reports(&self) -> ControlReports {
        let mut keyboard = [0; 8];
        let mut gamepad = [0, 0, 0x0f, 0, 0, 0, 0];
        let mut mouse_buttons = 0;
        let mut hat_directions = 0_u8;
        let mut keys = BTreeSet::new();
        for action in self.pressed.values() {
            match *action {
                ControlAction::Keyboard(usage @ 0xe0..=0xe7) => {
                    keyboard[0] |= 1 << (usage - 0xe0);
                }
                ControlAction::Keyboard(usage) if usage != 0 => {
                    keys.insert(usage);
                }
                ControlAction::GamepadButton(button) if button < 16 => {
                    gamepad[(button / 8) as usize] |= 1 << (button % 8);
                }
                ControlAction::GamepadHat(direction) if direction < 4 => {
                    hat_directions |= 1 << direction;
                }
                ControlAction::MouseButton(buttons) => mouse_buttons |= buttons & 0x07,
                _ => {}
            }
        }
        for (slot, usage) in keyboard[2..].iter_mut().zip(keys) {
            *slot = usage;
        }
        gamepad[2] = match hat_directions & 0x0f {
            0b0001 => 0,
            0b0011 => 1,
            0b0010 => 2,
            0b0110 => 3,
            0b0100 => 4,
            0b1100 => 5,
            0b1000 => 6,
            0b1001 => 7,
            _ => 0x0f,
        };
        ControlReports {
            keyboard,
            gamepad,
            mouse_buttons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_sources_release_independently_and_clear_neutralizes() {
        let mut state = ActiveControls::default();
        state.press("touch".into(), ControlAction::GamepadButton(0));
        state.press("hardware".into(), ControlAction::GamepadButton(0));
        state.press("key".into(), ControlAction::Keyboard(44));
        state.release("touch");
        assert_eq!(state.reports().gamepad[0], 1);
        assert_eq!(state.reports().keyboard[2], 44);
        state.release("hardware");
        assert_eq!(state.reports().gamepad[0], 0);
        state.clear();
        assert_eq!(state.reports().keyboard, [0; 8]);
        assert_eq!(state.reports().gamepad, [0, 0, 0x0f, 0, 0, 0, 0]);
    }
}
