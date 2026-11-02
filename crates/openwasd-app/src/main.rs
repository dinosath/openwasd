use dioxus::prelude::*;
use openwasd_bluetooth::{
    BluetoothCommand, BluetoothDeviceInfo, BluetoothEvent, BluetoothService, BtleplugAdapter,
    ConnectionState, HidHostState, merge_devices,
};
use openwasd_core::custom_layout::{ActiveControls, ControlAction, CustomControl, HardwareMapping};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, rc::Rc};
use tokio::runtime::Runtime;

const APP_CSS: &str = include_str!("../assets/dioxus.css");
const CUSTOM_LIMIT: usize = 6;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
enum LayoutKind {
    Keyboard60,
    Keyboard80,
    Keyboard100,
    Gamepad,
    Custom,
    #[default]
    Touchpad,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum KeyboardFormFactor {
    Compact60,
    #[default]
    Tenkeyless80,
    Full100,
}

impl LayoutKind {
    fn title(self) -> &'static str {
        match self {
            Self::Keyboard60 => "Keyboard 60%",
            Self::Keyboard80 => "Keyboard 80%",
            Self::Keyboard100 => "Keyboard 100%",
            Self::Gamepad => "Gamepad",
            Self::Custom => "Custom layout",
            Self::Touchpad => "Mouse / keyboard",
        }
    }

    fn keyboard_form_factor(self) -> Option<KeyboardFormFactor> {
        match self {
            Self::Keyboard60 => Some(KeyboardFormFactor::Compact60),
            Self::Keyboard80 => Some(KeyboardFormFactor::Tenkeyless80),
            Self::Keyboard100 => Some(KeyboardFormFactor::Full100),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SavedLayout {
    name: String,
    kind: LayoutKind,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    controls: Vec<CustomControl>,
    #[serde(default)]
    geometry: Vec<ControlGeometry>,
    #[serde(default)]
    hardware_mappings: Vec<HardwareMapping>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LearnTarget {
    Button(usize),
    Layout(ControlAction),
}

#[derive(Clone, Debug, Deserialize)]
struct HardwareInputEvent {
    code: String,
    down: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ControlGeometry {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

#[derive(Clone, Debug)]
struct LayoutSnapshot {
    controls: Vec<CustomControl>,
    geometry: Vec<ControlGeometry>,
    hardware_mappings: Vec<HardwareMapping>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureMode {
    Move,
    ResizeLeft,
    ResizeRight,
}

#[derive(Clone, Copy, Debug)]
struct PointerGesture {
    control_index: usize,
    mode: GestureMode,
    start_x: f64,
    start_y: f64,
    surface_width: f64,
    surface_height: f64,
    original: ControlGeometry,
}

#[derive(Clone, Copy, Debug)]
struct TouchpadGesture {
    pointer_id: i32,
    start_x: f64,
    start_y: f64,
    last_x: f64,
    last_y: f64,
    moved: bool,
}

#[derive(Clone, Copy)]
enum GamepadControl {
    Button(u8),
    Dpad(u8),
}

#[derive(Clone, Copy)]
struct StickGesture {
    axis_x: usize,
    axis_y: usize,
    center_x: f64,
    center_y: f64,
    radius: f64,
}

#[derive(Default)]
struct GamepadInputState {
    controls: HashMap<i32, GamepadControl>,
    sticks: HashMap<i32, StickGesture>,
    axes: [i8; 4],
}

#[derive(Clone, Copy)]
struct KeyboardKey {
    label: &'static str,
    usage: u8,
    flex: u8,
    class_name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Devices,
    Layouts,
    CreateLayout,
    Surface,
    Settings,
    ButtonEditor,
    LayoutHardwareMappings,
}

struct Backend {
    _runtime: Runtime,
    bluetooth: BluetoothService,
}

impl PartialEq for Backend {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Backend {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Bluetooth runtime should initialize");
        let bluetooth = {
            let _guard = runtime.enter();
            BluetoothService::spawn(BtleplugAdapter::new())
        };
        let _ = bluetooth
            .command_sender()
            .try_send(BluetoothCommand::RefreshConnected);
        Self {
            _runtime: runtime,
            bluetooth,
        }
    }

    fn send(&self, command: BluetoothCommand) {
        if let Err(error) = self.bluetooth.command_sender().try_send(command) {
            eprintln!("[OpenWASD] Bluetooth command could not be queued: {error}");
        }
    }
}

fn layouts_path() -> Option<PathBuf> {
    robius_directories::ProjectDirs::from("io", "openwasd", "OpenWASD")
        .map(|dirs| dirs.data_local_dir().join("layouts.json"))
}

fn load_layouts() -> Vec<SavedLayout> {
    let mut layouts: Vec<SavedLayout> = layouts_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    for layout in &mut layouts {
        for index in layout.geometry.len()..layout.controls.len() {
            layout.geometry.push(default_geometry(index));
        }
        layout.geometry.truncate(layout.controls.len());
    }
    for kind in [
        LayoutKind::Keyboard60,
        LayoutKind::Keyboard80,
        LayoutKind::Keyboard100,
        LayoutKind::Touchpad,
        LayoutKind::Gamepad,
    ] {
        if !layouts
            .iter()
            .any(|layout| layout.is_default && layout.kind == kind)
        {
            layouts.push(default_layout(kind));
        }
    }
    layouts
}

fn default_layout_index(layouts: &[SavedLayout], kind: LayoutKind) -> Option<usize> {
    layouts
        .iter()
        .position(|layout| layout.is_default && layout.kind == kind)
}

fn save_layouts(layouts: &[SavedLayout]) -> Result<(), String> {
    let path = layouts_path().ok_or_else(|| "App data directory is unavailable.".to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "Layout storage path is invalid.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(layouts).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

fn default_control() -> CustomControl {
    CustomControl {
        builtin_id: None,
        label: "clickme".into(),
        action: ControlAction::GamepadButton(0),
        text_output: None,
        hardware_key: None,
        show_text: true,
        show_icon: false,
        custom_icon: None,
    }
}

fn default_layout(kind: LayoutKind) -> SavedLayout {
    let controls = match kind {
        LayoutKind::Keyboard60 | LayoutKind::Keyboard80 | LayoutKind::Keyboard100 => {
            let form_factor = kind.keyboard_form_factor().unwrap_or_default();
            full_keyboard_main_rows(form_factor)
                .into_iter()
                .chain(full_keyboard_aux_rows(form_factor))
                .chain(if form_factor == KeyboardFormFactor::Full100 {
                    keyboard_numpad_rows()
                } else {
                    Vec::new()
                })
                .flatten()
                .filter(|key| key.usage != 0)
                .map(|key| CustomControl {
                    builtin_id: Some(format!("key:{}", key.usage)),
                    label: key.label.to_string(),
                    action: ControlAction::Keyboard(key.usage),
                    ..default_control()
                })
                .collect()
        }
        LayoutKind::Touchpad => [
            ("mouse-left", "Left click", ControlAction::MouseButton(1)),
            (
                "mouse-middle",
                "Middle click",
                ControlAction::MouseButton(4),
            ),
            ("mouse-right", "Right click", ControlAction::MouseButton(2)),
        ]
        .into_iter()
        .map(|(id, label, action)| CustomControl {
            builtin_id: Some(id.into()),
            label: label.into(),
            action,
            ..default_control()
        })
        .collect(),
        LayoutKind::Gamepad => [
            ("button:0", "A", ControlAction::GamepadButton(0)),
            ("button:1", "B", ControlAction::GamepadButton(1)),
            ("button:2", "X", ControlAction::GamepadButton(2)),
            ("button:3", "Y", ControlAction::GamepadButton(3)),
            ("button:4", "LB", ControlAction::GamepadButton(4)),
            ("button:5", "RB", ControlAction::GamepadButton(5)),
            ("button:6", "LT", ControlAction::GamepadButton(6)),
            ("button:7", "RT", ControlAction::GamepadButton(7)),
            ("button:8", "Back", ControlAction::GamepadButton(8)),
            ("button:9", "Start", ControlAction::GamepadButton(9)),
            ("button:11", "L3", ControlAction::GamepadButton(11)),
            ("button:12", "R3", ControlAction::GamepadButton(12)),
            ("hat:0", "D-pad up", ControlAction::GamepadHat(0)),
            ("hat:1", "D-pad right", ControlAction::GamepadHat(1)),
            ("hat:2", "D-pad down", ControlAction::GamepadHat(2)),
            ("hat:3", "D-pad left", ControlAction::GamepadHat(3)),
        ]
        .into_iter()
        .map(|(id, label, action)| CustomControl {
            builtin_id: Some(id.into()),
            label: label.into(),
            action,
            ..default_control()
        })
        .collect(),
        LayoutKind::Custom => Vec::new(),
    };
    SavedLayout {
        name: kind.title().into(),
        kind,
        is_default: true,
        geometry: (0..controls.len()).map(default_geometry).collect(),
        controls,
        hardware_mappings: Vec::new(),
    }
}

fn action_from_id(id: &str) -> ControlAction {
    if let Some(button) = id
        .strip_prefix("gamepad-")
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|button| *button < 16)
    {
        return ControlAction::GamepadButton(button);
    }

    match id {
        "key-space" => ControlAction::Keyboard(44),
        "key-enter" => ControlAction::Keyboard(40),
        "key-escape" => ControlAction::Keyboard(41),
        "gamepad-b" => ControlAction::GamepadButton(1),
        "gamepad-x" => ControlAction::GamepadButton(2),
        "gamepad-y" => ControlAction::GamepadButton(3),
        "mouse-left" => ControlAction::MouseButton(1),
        "bluetooth-settings" => ControlAction::BluetoothSettings,
        _ => ControlAction::GamepadButton(0),
    }
}

fn gamepad_button_name(button: u8) -> String {
    match button {
        0 => "A".into(),
        1 => "B".into(),
        2 => "X".into(),
        3 => "Y".into(),
        4 => "L1".into(),
        5 => "R1".into(),
        6 => "L2".into(),
        7 => "R2".into(),
        8 => "Select".into(),
        9 => "Start".into(),
        10 => "Home".into(),
        11 => "L3".into(),
        12 => "R3".into(),
        _ => format!("Button {}", button + 1),
    }
}

fn action_label(action: ControlAction) -> String {
    match action {
        ControlAction::Keyboard(44) => "Space".into(),
        ControlAction::Keyboard(40) => "Enter".into(),
        ControlAction::Keyboard(_) => "Keyboard key".into(),
        ControlAction::GamepadButton(button) => format!("Gamepad {}", gamepad_button_name(button)),
        ControlAction::GamepadHat(direction) => format!(
            "Gamepad D-pad {}",
            ["up", "right", "down", "left"]
                .get(direction as usize)
                .copied()
                .unwrap_or("direction")
        ),
        ControlAction::MouseButton(_) => "Mouse button".into(),
        ControlAction::BluetoothSettings => "Bluetooth settings".into(),
    }
}

fn action_value(action: ControlAction) -> String {
    match action {
        ControlAction::Keyboard(value) => format!("key:{value}"),
        ControlAction::GamepadButton(value) => format!("button:{value}"),
        ControlAction::GamepadHat(value) => format!("hat:{value}"),
        ControlAction::MouseButton(value) => format!("mouse:{value}"),
        ControlAction::BluetoothSettings => "bluetooth".into(),
    }
}

fn parse_action_value(value: &str) -> Option<ControlAction> {
    let (kind, value) = value.split_once(':').unwrap_or((value, ""));
    match kind {
        "key" => value.parse().ok().map(ControlAction::Keyboard),
        "button" => value.parse().ok().map(ControlAction::GamepadButton),
        "hat" => value.parse().ok().map(ControlAction::GamepadHat),
        "mouse" => value.parse().ok().map(ControlAction::MouseButton),
        _ => None,
    }
}

fn layout_action_choices(kind: LayoutKind) -> Vec<(String, String)> {
    match kind {
        LayoutKind::Keyboard60 | LayoutKind::Keyboard80 | LayoutKind::Keyboard100 => {
            keyboard_mapping_choices()
                .into_iter()
                .map(|(label, usage)| (format!("key:{usage}"), label))
                .collect()
        }
        LayoutKind::Touchpad => [
            ("mouse:1", "Left mouse button"),
            ("mouse:4", "Middle mouse button"),
            ("mouse:2", "Right mouse button"),
        ]
        .into_iter()
        .map(|(v, l)| (v.into(), l.into()))
        .collect(),
        LayoutKind::Gamepad => (0..16)
            .filter(|button| *button != 10)
            .map(|button| {
                (
                    format!("button:{button}"),
                    format!("Gamepad {}", gamepad_button_name(button)),
                )
            })
            .chain((0..4).map(|direction| {
                (
                    format!("hat:{direction}"),
                    format!(
                        "D-pad {}",
                        ["up", "right", "down", "left"][direction as usize]
                    ),
                )
            }))
            .collect(),
        LayoutKind::Custom => Vec::new(),
    }
}

fn keyboard_mapping_choices() -> Vec<(String, u8)> {
    fn add(choices: &mut Vec<(String, u8)>, label: impl Into<String>, usage: u8) {
        choices.push((label.into(), usage));
    }

    let mut choices = Vec::new();
    for (index, letter) in ('A'..='Z').enumerate() {
        add(&mut choices, letter.to_string(), 4 + index as u8);
    }
    for (index, digit) in ('1'..='9').enumerate() {
        add(&mut choices, digit.to_string(), 30 + index as u8);
    }
    add(&mut choices, "0", 39);

    for (label, usage) in [
        ("Enter", 40),
        ("Escape", 41),
        ("Backspace", 42),
        ("Tab", 43),
        ("Space", 44),
        ("Minus (-)", 45),
        ("Equal (=)", 46),
        ("Left bracket ([)", 47),
        ("Right bracket (])", 48),
        ("Backslash (\\)", 49),
        ("Non-US # / ~", 50),
        ("Semicolon (;)", 51),
        ("Apostrophe (')", 52),
        ("Grave accent (`)", 53),
        ("Comma (,)", 54),
        ("Period (.)", 55),
        ("Slash (/)", 56),
        ("Caps Lock", 57),
    ] {
        add(&mut choices, label, usage);
    }

    for function_key in 1..=12 {
        add(
            &mut choices,
            format!("F{function_key}"),
            57 + function_key as u8,
        );
    }

    for (label, usage) in [
        ("Print Screen", 70),
        ("Scroll Lock", 71),
        ("Pause", 72),
        ("Insert", 73),
        ("Home", 74),
        ("Page Up", 75),
        ("Delete", 76),
        ("End", 77),
        ("Page Down", 78),
        ("Right Arrow", 79),
        ("Left Arrow", 80),
        ("Down Arrow", 81),
        ("Up Arrow", 82),
        ("Num Lock", 83),
        ("Keypad /", 84),
        ("Keypad *", 85),
        ("Keypad -", 86),
        ("Keypad +", 87),
        ("Keypad Enter", 88),
    ] {
        add(&mut choices, label, usage);
    }

    for digit in 1..=9 {
        add(&mut choices, format!("Keypad {digit}"), 88 + digit);
    }
    for (label, usage) in [
        ("Keypad 0", 98),
        ("Keypad .", 99),
        ("Non-US \\", 100),
        ("Application", 101),
        ("Power", 102),
        ("Keypad =", 103),
    ] {
        add(&mut choices, label, usage);
    }

    for function_key in 13..=24 {
        add(
            &mut choices,
            format!("F{function_key}"),
            91 + function_key as u8,
        );
    }

    for (label, usage) in [
        ("Execute", 116),
        ("Help", 117),
        ("Menu", 118),
        ("Select", 119),
        ("Stop", 120),
        ("Again", 121),
        ("Undo", 122),
        ("Cut", 123),
        ("Copy", 124),
        ("Paste", 125),
        ("Find", 126),
        ("Mute", 127),
        ("Volume Up", 128),
        ("Volume Down", 129),
        ("Locking Caps Lock", 130),
        ("Locking Num Lock", 131),
        ("Locking Scroll Lock", 132),
        ("Keypad Comma", 133),
        ("Keypad Equal (AS/400)", 134),
    ] {
        add(&mut choices, label, usage);
    }

    for language_key in 1..=9 {
        add(
            &mut choices,
            format!("International {language_key}"),
            134 + language_key as u8,
        );
    }
    for language_key in 1..=9 {
        add(
            &mut choices,
            format!("Language {language_key}"),
            143 + language_key as u8,
        );
    }

    for (label, usage) in [
        ("Alternate Erase", 153),
        ("SysReq / Attention", 154),
        ("Cancel", 155),
        ("Clear", 156),
        ("Prior", 157),
        ("Return", 158),
        ("Separator", 159),
        ("Out", 160),
        ("Oper", 161),
        ("Clear / Again", 162),
        ("CrSel / Props", 163),
        ("ExSel", 164),
        ("Keypad 00", 176),
        ("Keypad 000", 177),
        ("Thousands Separator", 178),
        ("Decimal Separator", 179),
        ("Currency Unit", 180),
        ("Currency Sub-unit", 181),
        ("Keypad (", 182),
        ("Keypad )", 183),
        ("Keypad {", 184),
        ("Keypad }", 185),
        ("Keypad Tab", 186),
        ("Keypad Backspace", 187),
        ("Keypad A", 188),
        ("Keypad B", 189),
        ("Keypad C", 190),
        ("Keypad D", 191),
        ("Keypad E", 192),
        ("Keypad F", 193),
        ("Keypad XOR", 194),
        ("Keypad ^", 195),
        ("Keypad %", 196),
        ("Keypad <", 197),
        ("Keypad >", 198),
        ("Keypad &", 199),
        ("Keypad &&", 200),
        ("Keypad |", 201),
        ("Keypad ||", 202),
        ("Keypad :", 203),
        ("Keypad #", 204),
        ("Keypad Space", 205),
        ("Keypad @", 206),
        ("Keypad !", 207),
        ("Keypad Memory Store", 208),
        ("Keypad Memory Recall", 209),
        ("Keypad Memory Clear", 210),
        ("Keypad Memory Add", 211),
        ("Keypad Memory Subtract", 212),
        ("Keypad Memory Multiply", 213),
        ("Keypad Memory Divide", 214),
        ("Keypad +/-", 215),
        ("Keypad Clear", 216),
        ("Keypad Clear Entry", 217),
        ("Keypad Binary", 218),
        ("Keypad Octal", 219),
        ("Keypad Decimal", 220),
        ("Keypad Hexadecimal", 221),
        ("Left Control", 224),
        ("Left Shift", 225),
        ("Left Alt", 226),
        ("Left GUI / Meta", 227),
        ("Right Control", 228),
        ("Right Shift", 229),
        ("Right Alt", 230),
        ("Right GUI / Meta", 231),
    ] {
        add(&mut choices, label, usage);
    }

    choices
}

fn keyboard_key(label: &'static str, usage: u8) -> KeyboardKey {
    KeyboardKey {
        label,
        usage,
        flex: 1,
        class_name: "",
    }
}

fn wide_keyboard_key(label: &'static str, usage: u8, flex: u8) -> KeyboardKey {
    KeyboardKey {
        label,
        usage,
        flex,
        class_name: "key-button-wide",
    }
}

fn keyboard_spacer(flex: u8) -> KeyboardKey {
    KeyboardKey {
        label: "",
        usage: 0,
        flex,
        class_name: "",
    }
}

fn full_keyboard_main_rows(form_factor: KeyboardFormFactor) -> Vec<Vec<KeyboardKey>> {
    let function_row = if form_factor == KeyboardFormFactor::Compact60 {
        vec![keyboard_key("Esc", 41), keyboard_spacer(12)]
    } else {
        let mut row = vec![keyboard_key("Esc", 41)];
        row.extend([
            keyboard_key("F1", 58),
            keyboard_key("F2", 59),
            keyboard_key("F3", 60),
            keyboard_key("F4", 61),
        ]);
        row.push(keyboard_spacer(1));
        row.extend([
            keyboard_key("F5", 62),
            keyboard_key("F6", 63),
            keyboard_key("F7", 64),
            keyboard_key("F8", 65),
        ]);
        row.push(keyboard_spacer(1));
        row.extend([
            keyboard_key("F9", 66),
            keyboard_key("F10", 67),
            keyboard_key("F11", 68),
            keyboard_key("F12", 69),
        ]);
        row
    };

    let mut rows = vec![
        function_row,
        vec![
            keyboard_key("` ~", 53),
            keyboard_key("1 !", 30),
            keyboard_key("2 @", 31),
            keyboard_key("3 #", 32),
            keyboard_key("4 $", 33),
            keyboard_key("5 %", 34),
            keyboard_key("6 ^", 35),
            keyboard_key("7 &", 36),
            keyboard_key("8 *", 37),
            keyboard_key("9 (", 38),
            keyboard_key("0 )", 39),
            keyboard_key("- _", 45),
            keyboard_key("= +", 46),
            wide_keyboard_key("Backspace", 42, 2),
        ],
        vec![
            wide_keyboard_key("Tab", 43, 3),
            keyboard_key("Q", 20),
            keyboard_key("W", 26),
            keyboard_key("E", 8),
            keyboard_key("R", 21),
            keyboard_key("T", 23),
            keyboard_key("Y", 28),
            keyboard_key("U", 24),
            keyboard_key("I", 12),
            keyboard_key("O", 18),
            keyboard_key("P", 19),
            keyboard_key("[ {", 47),
            keyboard_key("] }", 48),
            keyboard_key("\\ |", 49),
        ],
        vec![
            wide_keyboard_key("Caps", 57, 3),
            keyboard_key("A", 4),
            keyboard_key("S", 22),
            keyboard_key("D", 7),
            keyboard_key("F", 9),
            keyboard_key("G", 10),
            keyboard_key("H", 11),
            keyboard_key("J", 13),
            keyboard_key("K", 14),
            keyboard_key("L", 15),
            keyboard_key("; :", 51),
            keyboard_key("' \"", 52),
            wide_keyboard_key("Enter", 40, 3),
        ],
        vec![
            wide_keyboard_key("Shift", 225, 4),
            keyboard_key("Z", 29),
            keyboard_key("X", 27),
            keyboard_key("C", 6),
            keyboard_key("V", 25),
            keyboard_key("B", 5),
            keyboard_key("N", 17),
            keyboard_key("M", 16),
            keyboard_key(", <", 54),
            keyboard_key(". >", 55),
            keyboard_key("/ ?", 56),
            wide_keyboard_key("Shift", 229, 4),
        ],
        vec![
            wide_keyboard_key("Ctrl", 224, 2),
            wide_keyboard_key("Win", 227, 2),
            wide_keyboard_key("Alt", 226, 2),
            wide_keyboard_key("Space", 44, 7),
            wide_keyboard_key("Alt", 230, 2),
            wide_keyboard_key("Win", 231, 2),
            wide_keyboard_key("Menu", 101, 2),
            wide_keyboard_key("Ctrl", 228, 2),
        ],
    ];
    if form_factor == KeyboardFormFactor::Compact60 {
        rows[5].retain(|key| key.usage != 101);
    }
    rows
}

fn full_keyboard_aux_rows(form_factor: KeyboardFormFactor) -> Vec<Vec<KeyboardKey>> {
    if form_factor == KeyboardFormFactor::Compact60 {
        return Vec::new();
    }
    vec![
        vec![
            keyboard_key("PrtSc", 70),
            keyboard_key("ScrLk", 71),
            keyboard_key("Pause", 72),
        ],
        vec![
            keyboard_key("Ins", 73),
            keyboard_key("Home", 74),
            keyboard_key("PgUp", 75),
        ],
        vec![
            keyboard_key("Del", 76),
            keyboard_key("End", 77),
            keyboard_key("PgDn", 78),
        ],
        vec![
            keyboard_spacer(1),
            keyboard_key("↑", 82),
            keyboard_spacer(1),
        ],
        vec![
            keyboard_key("←", 80),
            keyboard_key("↓", 81),
            keyboard_key("→", 79),
        ],
        vec![keyboard_spacer(1), keyboard_spacer(1), keyboard_spacer(1)],
    ]
}

fn keyboard_numpad_rows() -> Vec<Vec<KeyboardKey>> {
    vec![
        vec![
            keyboard_key("Num", 83),
            keyboard_key("/", 84),
            keyboard_key("*", 85),
            keyboard_key("-", 86),
        ],
        vec![
            keyboard_key("7", 95),
            keyboard_key("8", 96),
            keyboard_key("9", 97),
            keyboard_spacer(1),
        ],
        vec![
            keyboard_key("4", 92),
            keyboard_key("5", 93),
            keyboard_key("6", 94),
            keyboard_key("+", 87),
        ],
        vec![
            keyboard_key("1", 89),
            keyboard_key("2", 90),
            keyboard_key("3", 91),
            keyboard_key("Enter", 88),
        ],
        vec![
            wide_keyboard_key("0", 98, 2),
            keyboard_key(".", 99),
            keyboard_spacer(1),
        ],
    ]
}

fn configured_keyboard_key(key: KeyboardKey, controls: &[CustomControl]) -> (String, u8) {
    let id = format!("key:{}", key.usage);
    let Some(control) = controls
        .iter()
        .find(|control| control.builtin_id.as_deref() == Some(id.as_str()))
    else {
        return (key.label.to_string(), key.usage);
    };
    let output = match control.action {
        ControlAction::Keyboard(usage) => usage,
        _ => key.usage,
    };
    (control.label.clone(), output)
}

fn configured_control(
    controls: &[CustomControl],
    id: &str,
    default_label: &str,
    default_action: ControlAction,
) -> (String, ControlAction) {
    controls
        .iter()
        .find(|control| control.builtin_id.as_deref() == Some(id))
        .map(|control| (control.label.clone(), control.action))
        .unwrap_or_else(|| (default_label.to_string(), default_action))
}

fn keyboard_report(usages: impl IntoIterator<Item = u8>) -> [u8; 8] {
    let mut report = [0; 8];
    for usage in usages {
        if (224..=231).contains(&usage) {
            report[0] |= 1 << (usage - 224);
        } else if usage != 0 && !report[2..].contains(&usage) {
            if let Some(slot) = report[2..].iter_mut().find(|slot| **slot == 0) {
                *slot = usage;
            }
        }
    }
    report
}

fn update_keyboard_pointer(
    backend: &Backend,
    mut pressed: Signal<HashMap<i32, u8>>,
    pointer_id: i32,
    usage: u8,
    down: bool,
) {
    pressed.with_mut(|keys| {
        if down {
            keys.insert(pointer_id, usage);
        } else {
            keys.remove(&pointer_id);
        }
    });
    backend.send(BluetoothCommand::HidKeyboard(keyboard_report(
        pressed.read().values().copied(),
    )));
}

fn send_text_output(backend: &Backend, text: &str) {
    for character in text.chars() {
        let value = character as u8;
        let key = if !character.is_ascii() {
            None
        } else {
            match character {
                'a'..='z' => Some((4 + value - b'a', 0)),
                'A'..='Z' => Some((4 + value - b'A', 2)),
                '1'..='9' => Some((30 + value - b'1', 0)),
                '0' => Some((39, 0)),
                ' ' => Some((44, 0)),
                '\n' | '\r' => Some((40, 0)),
                '\t' => Some((43, 0)),
                '-' => Some((45, 0)),
                '_' => Some((45, 2)),
                '=' => Some((46, 0)),
                '+' => Some((46, 2)),
                '[' => Some((47, 0)),
                '{' => Some((47, 2)),
                ']' => Some((48, 0)),
                '}' => Some((48, 2)),
                '\\' => Some((49, 0)),
                '|' => Some((49, 2)),
                ';' => Some((51, 0)),
                ':' => Some((51, 2)),
                '\'' => Some((52, 0)),
                '"' => Some((52, 2)),
                '`' => Some((53, 0)),
                '~' => Some((53, 2)),
                ',' => Some((54, 0)),
                '<' => Some((54, 2)),
                '.' => Some((55, 0)),
                '>' => Some((55, 2)),
                '/' => Some((56, 0)),
                '?' => Some((56, 2)),
                '!' => Some((30, 2)),
                '@' => Some((31, 2)),
                '#' => Some((32, 2)),
                '$' => Some((33, 2)),
                '%' => Some((34, 2)),
                '^' => Some((35, 2)),
                '&' => Some((36, 2)),
                '*' => Some((37, 2)),
                '(' => Some((38, 2)),
                ')' => Some((39, 2)),
                _ => None,
            }
        };
        if let Some((usage, modifier)) = key {
            let mut report = [0; 8];
            report[0] = modifier;
            report[2] = usage;
            backend.send(BluetoothCommand::HidKeyboard(report));
            backend.send(BluetoothCommand::HidKeyboard([0; 8]));
        }
    }
}

fn process_hardware_input(
    event: HardwareInputEvent,
    mut learning: Signal<Option<LearnTarget>>,
    screen: Signal<Screen>,
    selected_kind: Signal<LayoutKind>,
    selected_layout: Signal<Option<usize>>,
    mut layouts: Signal<Vec<SavedLayout>>,
    mut undo: Signal<Vec<LayoutSnapshot>>,
    mut redo: Signal<Vec<LayoutSnapshot>>,
    mut active: Signal<ActiveControls>,
    backend: &Backend,
    mut pressed_sources: Signal<HashMap<String, Vec<String>>>,
) {
    if event.down {
        if let Some(target) = learning() {
            if let Some(layout_index) = selected_layout() {
                let mut updated = layouts();
                if let Some(layout) = updated.get_mut(layout_index) {
                    undo.with_mut(|history| history.push(layout_snapshot(layout)));
                    redo.set(Vec::new());
                    layout
                        .hardware_mappings
                        .retain(|mapping| mapping.hardware_key != event.code);
                    for control in &mut layout.controls {
                        if control.hardware_key.as_deref() == Some(event.code.as_str()) {
                            control.hardware_key = None;
                        }
                    }
                    match target {
                        LearnTarget::Button(index) => {
                            if let Some(control) = layout.controls.get_mut(index) {
                                control.hardware_key = Some(event.code);
                            }
                        }
                        LearnTarget::Layout(action) => {
                            layout.hardware_mappings.push(HardwareMapping {
                                hardware_key: event.code,
                                action,
                            })
                        }
                    }
                    if save_layouts(&updated).is_ok() {
                        layouts.set(updated);
                    }
                }
            }
            learning.set(None);
            return;
        }
        if screen() != Screen::Surface || selected_kind() != LayoutKind::Custom {
            return;
        }
        let Some(layout_index) = selected_layout() else {
            return;
        };
        let current_layouts = layouts();
        let Some(layout) = current_layouts.get(layout_index) else {
            return;
        };
        let mut sources = Vec::new();
        for (index, control) in layout.controls.iter().enumerate() {
            if control.hardware_key.as_deref() == Some(event.code.as_str()) {
                if let Some(text) = control.text_output.as_deref() {
                    send_text_output(backend, text);
                } else {
                    let source = format!("hardware-button:{}:{index}", event.code);
                    press_control(backend, &mut active, source.clone(), control.action);
                    sources.push(source);
                }
            }
        }
        for (index, mapping) in layout.hardware_mappings.iter().enumerate() {
            if mapping.hardware_key == event.code {
                let source = format!("hardware-layout:{}:{index}", event.code);
                press_control(backend, &mut active, source.clone(), mapping.action);
                sources.push(source);
            }
        }
        if !sources.is_empty() {
            pressed_sources.with_mut(|pressed| {
                pressed.insert(event.code, sources);
            });
        }
    } else if let Some(sources) = pressed_sources.with_mut(|pressed| pressed.remove(&event.code)) {
        for source in sources {
            release_control(backend, &mut active, &source);
        }
    }
}

fn default_geometry(index: usize) -> ControlGeometry {
    ControlGeometry {
        x: 4 + (index % 3) as u16 * 32,
        y: 5 + (index / 3) as u16 * 22,
        width: 29,
        height: 18,
    }
}

fn layout_snapshot(layout: &SavedLayout) -> LayoutSnapshot {
    LayoutSnapshot {
        controls: layout.controls.clone(),
        geometry: layout.geometry.clone(),
        hardware_mappings: layout.hardware_mappings.clone(),
    }
}

fn geometry_for_pointer(gesture: PointerGesture, client_x: f64, client_y: f64) -> ControlGeometry {
    let dx = (client_x - gesture.start_x) / gesture.surface_width * 100.0;
    let dy = (client_y - gesture.start_y) / gesture.surface_height * 100.0;
    let original = gesture.original;
    match gesture.mode {
        GestureMode::Move => {
            let x = (original.x as f64 + dx).clamp(0.0, (100 - original.width) as f64);
            let y = (original.y as f64 + dy).clamp(0.0, (100 - original.height) as f64);
            ControlGeometry {
                x: x.round() as u16,
                y: y.round() as u16,
                ..original
            }
        }
        GestureMode::ResizeLeft => {
            let width =
                (original.width as f64 - dx).clamp(10.0, (original.x + original.width) as f64);
            let x = (original.x as f64 + original.width as f64 - width).clamp(0.0, 100.0 - width);
            let height = (original.height as f64 + dy).clamp(10.0, (100 - original.y) as f64);
            ControlGeometry {
                x: x.round() as u16,
                width: width.round() as u16,
                height: height.round() as u16,
                ..original
            }
        }
        GestureMode::ResizeRight => {
            let width = (original.width as f64 + dx).clamp(10.0, (100 - original.x) as f64);
            let height = (original.height as f64 + dy).clamp(10.0, (100 - original.y) as f64);
            ControlGeometry {
                width: width.round() as u16,
                height: height.round() as u16,
                ..original
            }
        }
    }
}

fn state_name(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Discovered => "Nearby",
        ConnectionState::Pairing => "Pairing",
        ConnectionState::Paired => "Paired",
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Connected => "Connected",
        ConnectionState::Reconnecting { .. } => "Reconnecting",
        ConnectionState::Disconnected => "Disconnected",
        ConnectionState::Failed { .. } => "Failed",
    }
}

fn send_reports(backend: &Backend, controls: &Signal<ActiveControls>) {
    let reports = controls.read().reports();
    backend.send(BluetoothCommand::HidKeyboard(reports.keyboard));
    backend.send(BluetoothCommand::HidGamepad(reports.gamepad));
    backend.send(BluetoothCommand::HidMouse {
        buttons: reports.mouse_buttons as i8,
        dx: 0,
        dy: 0,
        wheel: 0,
    });
}

fn send_gamepad_report(backend: &Backend, state: &Signal<GamepadInputState>) {
    let state = state.read();
    let mut report = [
        0_u8,
        0,
        0x0f,
        state.axes[0] as u8,
        state.axes[1] as u8,
        state.axes[2] as u8,
        state.axes[3] as u8,
    ];
    let mut directions = 0_u8;
    for control in state.controls.values() {
        match control {
            GamepadControl::Button(button) if *button < 16 => {
                report[(*button / 8) as usize] |= 1 << (*button % 8);
            }
            GamepadControl::Dpad(direction) => directions |= 1 << direction,
            _ => {}
        }
    }
    report[2] = match directions & 0x0f {
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
    backend.send(BluetoothCommand::HidGamepad(report));
}

fn set_gamepad_control(
    backend: &Backend,
    mut state: Signal<GamepadInputState>,
    pointer_id: i32,
    control: GamepadControl,
    pressed: bool,
) {
    state.with_mut(|state| {
        if pressed {
            state.controls.insert(pointer_id, control);
        } else {
            state.controls.remove(&pointer_id);
        }
    });
    send_gamepad_report(backend, &state);
}

async fn begin_gamepad_stick(
    backend: &Backend,
    mut state: Signal<GamepadInputState>,
    pointer_id: i32,
    axis_x: usize,
    axis_y: usize,
    click_button: Option<u8>,
) {
    let script = format!(
        "const el=document.getElementById('gamepad-stick-{axis_x}'); if(!el) return null; el.setPointerCapture({pointer_id}); const r=el.getBoundingClientRect(); return [r.left+r.width/2,r.top+r.height/2,r.width*0.36];"
    );
    let bounds = dioxus::document::eval(&script)
        .await
        .ok()
        .and_then(|value| serde_json::from_value::<(f64, f64, f64)>(value).ok());
    let Some((center_x, center_y, radius)) = bounds.filter(|(_, _, radius)| *radius > 0.0) else {
        return;
    };
    if click_button.is_some() && !state.read().controls.contains_key(&pointer_id) {
        return;
    }
    state.with_mut(|state| {
        state.sticks.insert(
            pointer_id,
            StickGesture {
                axis_x,
                axis_y,
                center_x,
                center_y,
                radius,
            },
        );
    });
    send_gamepad_report(backend, &state);
}

fn end_gamepad_stick(backend: &Backend, mut state: Signal<GamepadInputState>, pointer_id: i32) {
    state.with_mut(|state| {
        state.controls.remove(&pointer_id);
        if let Some(stick) = state.sticks.remove(&pointer_id) {
            state.axes[stick.axis_x] = 0;
            state.axes[stick.axis_y] = 0;
        }
    });
    send_gamepad_report(backend, &state);
}

fn press_control(
    backend: &Backend,
    active: &mut Signal<ActiveControls>,
    source: String,
    action: ControlAction,
) {
    active.with_mut(|controls| controls.press(source, action));
    send_reports(backend, active);
    if action == ControlAction::BluetoothSettings {
        #[cfg(target_os = "android")]
        if let Err(error) = openwasd_bluetooth::open_android_bluetooth_settings() {
            eprintln!("[OpenWASD] Could not open Bluetooth settings: {error}");
        }
    }
}

fn release_control(backend: &Backend, active: &mut Signal<ActiveControls>, source: &str) {
    active.with_mut(|controls| controls.release(source));
    send_reports(backend, active);
}

async fn custom_surface_size() -> Option<(f64, f64)> {
    let script = r#"const surface = document.querySelector('.custom-surface-grid');
if (!surface) return null;
const rect = surface.getBoundingClientRect();
return [rect.width, rect.height];"#;
    dioxus::document::eval(&script)
        .await
        .ok()
        .and_then(|value| serde_json::from_value::<(f64, f64)>(value).ok())
}

async fn begin_pointer_gesture(
    index: usize,
    mode: GestureMode,
    x: f64,
    y: f64,
    pointer_id: i32,
    geometry: ControlGeometry,
    mut state: Signal<Option<PointerGesture>>,
    mut preview: Signal<Option<(usize, ControlGeometry)>>,
) {
    let capture_script = format!(
        "document.getElementById('control-frame-{index}')?.setPointerCapture({pointer_id});"
    );
    let _ = dioxus::document::eval(&capture_script).await;
    if let Some((surface_width, surface_height)) = custom_surface_size().await {
        if surface_width > 0.0 && surface_height > 0.0 {
            state.set(Some(PointerGesture {
                control_index: index,
                mode,
                start_x: x,
                start_y: y,
                surface_width,
                surface_height,
                original: geometry,
            }));
            preview.set(Some((index, geometry)));
        }
    }
}

fn commit_control_geometry(
    mut layouts: Signal<Vec<SavedLayout>>,
    selected_layout: Signal<Option<usize>>,
    mut undo: Signal<Vec<LayoutSnapshot>>,
    mut redo: Signal<Vec<LayoutSnapshot>>,
    control_index: usize,
    geometry: ControlGeometry,
) {
    let Some(layout_index) = selected_layout() else {
        return;
    };
    let mut updated = layouts();
    let Some(layout) = updated.get_mut(layout_index) else {
        return;
    };
    if control_index >= layout.controls.len() || control_index >= layout.geometry.len() {
        return;
    }
    if layout.geometry[control_index] == geometry {
        return;
    }
    undo.with_mut(|history| history.push(layout_snapshot(layout)));
    redo.set(Vec::new());
    layout.geometry[control_index] = geometry;
    if save_layouts(&updated).is_ok() {
        layouts.set(updated);
    }
}

fn update_custom_control(
    mut layouts: Signal<Vec<SavedLayout>>,
    selected_layout: Signal<Option<usize>>,
    mut undo: Signal<Vec<LayoutSnapshot>>,
    mut redo: Signal<Vec<LayoutSnapshot>>,
    index: usize,
    update: impl FnOnce(&mut CustomControl),
) {
    let Some(layout_index) = selected_layout() else {
        return;
    };
    let mut updated = layouts();
    let Some(layout) = updated.get_mut(layout_index) else {
        return;
    };
    if index >= layout.controls.len() {
        return;
    }
    undo.with_mut(|history| history.push(layout_snapshot(layout)));
    redo.set(Vec::new());
    update(&mut layout.controls[index]);
    if save_layouts(&updated).is_ok() {
        layouts.set(updated);
    }
}

#[allow(non_snake_case)] // Dioxus components use PascalCase names.
fn OpenWASDApp() -> Element {
    let backend = use_hook(|| Rc::new(Backend::new()));
    let mut screen = use_signal(|| Screen::Dashboard);
    let mut drawer_open = use_signal(|| false);
    let devices = use_signal(Vec::<BluetoothDeviceInfo>::new);
    let mut bluetooth_status = use_signal(|| "Bluetooth is ready.".to_string());
    let host = use_signal(HidHostState::default);
    let hid_ready = use_signal(|| false);
    let mut layouts = use_signal(load_layouts);
    let mut selected_kind = use_signal(|| LayoutKind::Touchpad);
    let mut selected_layout = use_signal(|| None::<usize>);
    let mut controls = use_signal(Vec::<CustomControl>::new);
    let mut active_controls = use_signal(ActiveControls::default);
    let keyboard_pressed_keys = use_signal(HashMap::<i32, u8>::new);
    let mut edit_mode = use_signal(|| false);
    let mut layout_undo = use_signal(Vec::<LayoutSnapshot>::new);
    let mut layout_redo = use_signal(Vec::<LayoutSnapshot>::new);
    let gesture_state = use_signal(|| None::<PointerGesture>);
    let preview_geometry = use_signal(|| None::<(usize, ControlGeometry)>);
    let button_editor_index = use_signal(|| None::<usize>);
    let mut button_editor_tab = use_signal(|| 0_u8);
    let mut hardware_learning = use_signal(|| None::<LearnTarget>);
    let mut touchpad_gesture = use_signal(|| None::<TouchpadGesture>);
    let mut layout_mapping_action = use_signal(|| "gamepad-0".to_string());
    let mut hardware_sources = use_signal(HashMap::<String, Vec<String>>::new);
    let mut layout_name = use_signal(String::new);
    let mut page_status = use_signal(String::new);

    let event_backend = backend.clone();
    use_future(move || {
        let backend = event_backend.clone();
        let mut devices = devices;
        let mut status = bluetooth_status;
        let mut host = host;
        let mut hid_ready = hid_ready;
        async move {
            while let Some(event) = backend.bluetooth.next_event().await {
                match event {
                    BluetoothEvent::ScanStarted => {
                        status.set("Scanning for nearby devices…".into())
                    }
                    BluetoothEvent::Devices(found) => {
                        let count = found.len();
                        devices.set(found);
                        status.set(format!("Found {count} nearby Bluetooth device(s)."));
                    }
                    BluetoothEvent::SystemDevices(found) => {
                        let mut current = devices();
                        current.retain(|device| !device.id.starts_with("system:"));
                        merge_devices(&mut current, found);
                        devices.set(current);
                    }
                    BluetoothEvent::DeviceUpdated(updated) => {
                        let mut current = devices();
                        merge_devices(&mut current, vec![updated]);
                        devices.set(current);
                    }
                    BluetoothEvent::ScanFinished => {}
                    BluetoothEvent::HidReady => {
                        hid_ready.set(true);
                        status.set("Bluetooth HID is ready.".into());
                    }
                    BluetoothEvent::HidHost(current) => host.set(current),
                    BluetoothEvent::Error { message, .. } => status.set(message),
                }
            }
        }
    });

    let input_backend = backend.clone();
    let keyboard_down_backend = backend.clone();
    let keyboard_up_backend = backend.clone();
    use_future(move || {
        let mut eval = dioxus::document::eval(
            r#"
                let previous = new Set();
                setInterval(() => {
                    if (!navigator.getGamepads) return;
                    const current = new Set();
                    for (const [padIndex, pad] of Array.from(navigator.getGamepads()).entries()) {
                        if (!pad) continue;
                        pad.buttons.forEach((button, buttonIndex) => {
                            if (!button.pressed) return;
                            const code = `Gamepad${padIndex}Button${buttonIndex}`;
                            current.add(code);
                            if (!previous.has(code)) dioxus.send({ code, down: true });
                        });
                    }
                    for (const code of previous) {
                        if (!current.has(code)) dioxus.send({ code, down: false });
                    }
                    previous = current;
                }, 60);
                await new Promise(() => {});
            "#,
        );
        let backend = input_backend.clone();
        async move {
            while let Ok(event) = eval.recv::<HardwareInputEvent>().await {
                process_hardware_input(
                    event,
                    hardware_learning,
                    screen,
                    selected_kind,
                    selected_layout,
                    layouts,
                    layout_undo,
                    layout_redo,
                    active_controls,
                    &backend,
                    hardware_sources,
                );
            }
        }
    });

    let cleanup_backend = backend.clone();
    use_effect(move || {
        let current_screen = screen();
        if !matches!(
            current_screen,
            Screen::ButtonEditor | Screen::LayoutHardwareMappings
        ) {
            hardware_learning.set(None);
        }
        if current_screen != Screen::Surface && !hardware_sources().is_empty() {
            hardware_sources.set(HashMap::new());
            if !active_controls.read().is_empty() {
                active_controls.with_mut(ActiveControls::clear);
                send_reports(&cleanup_backend, &active_controls);
            }
        }
    });

    let current_screen = screen();
    let is_drawer_open = drawer_open();
    let selected_layout_index = selected_layout();
    let layout_name_display = selected_layout_index
        .and_then(|index| layouts().get(index).map(|layout| layout.name.clone()))
        .unwrap_or_else(|| selected_kind().title().to_string());
    let current_controls = selected_layout_index
        .and_then(|index| layouts().get(index).map(|layout| layout.controls.clone()))
        .unwrap_or_else(|| controls());
    let layout_title = if current_screen == Screen::ButtonEditor {
        button_editor_index()
            .and_then(|index| {
                current_controls
                    .get(index)
                    .map(|control| control.label.clone())
            })
            .unwrap_or_else(|| layout_name_display.clone())
    } else if current_screen == Screen::LayoutHardwareMappings {
        format!("{} hardware mappings", layout_name_display)
    } else {
        layout_name_display
    };
    let current_geometry = selected_layout_index
        .and_then(|index| layouts().get(index).map(|layout| layout.geometry.clone()))
        .unwrap_or_else(|| (0..current_controls.len()).map(default_geometry).collect());
    let device_list = devices();
    let edit = edit_mode();
    let app_backend = backend.clone();
    let refresh_backend = app_backend.clone();
    let scan_backend = app_backend.clone();
    let gamepad_backend = app_backend.clone();
    let create_backend = app_backend.clone();
    let surface_gamepad_backend = app_backend.clone();
    let surface_touchpad_backend = app_backend.clone();
    let surface_left_backend = app_backend.clone();
    let surface_middle_backend = app_backend.clone();
    let surface_right_backend = app_backend.clone();
    let status_text = bluetooth_status();
    let hid_host = host();
    let hid_is_ready = hid_ready();

    rsx! {
        style { {APP_CSS} }
        div {
            class: "app-shell",
            tabindex: "0",
            autofocus: true,
            onkeydown: move |event| {
                if !event.data().is_auto_repeating() {
                    let code = format!("{:?}", event.data().code());
                    if code != "Unidentified" {
                        process_hardware_input(
                            HardwareInputEvent { code, down: true },
                            hardware_learning,
                            screen,
                            selected_kind,
                            selected_layout,
                            layouts,
                            layout_undo,
                            layout_redo,
                            active_controls,
                            &keyboard_down_backend,
                            hardware_sources,
                        );
                    }
                }
            },
            onkeyup: move |event| {
                let code = format!("{:?}", event.data().code());
                if code != "Unidentified" {
                    process_hardware_input(
                        HardwareInputEvent { code, down: false },
                        hardware_learning,
                        screen,
                        selected_kind,
                        selected_layout,
                        layouts,
                        layout_undo,
                        layout_redo,
                        active_controls,
                        &keyboard_up_backend,
                        hardware_sources,
                    );
                }
            },
            AppTopBar {
                current_screen,
                is_drawer_open,
                title: layout_title.clone(),
                selected_kind: selected_kind(),
                edit,
                control_count: current_controls.len(),
                undo_available: !layout_undo().is_empty(),
                redo_available: !layout_redo().is_empty(),
                screen,
                drawer_open,
                edit_mode,
                layout_name,
                page_status,
                layouts,
                selected_layout,
                layout_undo,
                layout_redo,
            }

            if is_drawer_open {
                div { class: "drawer-overlay",
                    button { class: "drawer-scrim", aria_label: "Close navigation menu", onclick: move |_| drawer_open.set(false) }
                    aside { class: "navigation-drawer", aria_label: "Main navigation",
                        div { class: "drawer-heading", div { class: "brand-mark", "O" }, div { h2 { "OpenWASD" }, p { "Input workspace" } } }
                        nav { class: "drawer-links",
                            button { class: if current_screen == Screen::Dashboard { "drawer-link active" } else { "drawer-link" }, onclick: move |_| { screen.set(Screen::Dashboard); drawer_open.set(false); }, span { class: "drawer-icon", "⌂" } "Home" }
                            button { class: if current_screen == Screen::Devices { "drawer-link active" } else { "drawer-link" }, onclick: move |_| { screen.set(Screen::Devices); drawer_open.set(false); }, span { class: "drawer-icon", "◉" } "Devices" }
                            button { class: if matches!(current_screen, Screen::Layouts | Screen::CreateLayout) { "drawer-link active" } else { "drawer-link" }, onclick: move |_| { screen.set(Screen::Layouts); drawer_open.set(false); }, span { class: "drawer-icon", "▦" } "Layouts" }
                        }
                    }
                }
            }

            main { class: if current_screen == Screen::Surface && selected_kind().keyboard_form_factor().is_some() { "page-content keyboard-page-content" } else { "page-content" },
                match current_screen {
                    Screen::Dashboard => rsx! {
                        section { class: "hero-card",
                            p { class: "eyebrow", "LOCAL-FIRST INPUT WORKSPACE" }
                            h2 { "Make every control fit." }
                            p { "Connect your devices, pick a control surface, and adapt it to the game or workflow." }
                            div { class: "hero-actions",
                                button { class: "button primary", onclick: move |_| screen.set(Screen::Layouts), "Open layouts" }
                                button { class: "button secondary", onclick: move |_| screen.set(Screen::Devices), "View devices" }
                            }
                        }
                        section { class: "status-card",
                            div { class: "section-heading", h2 { "Connection status" }, button { class: "text-button", onclick: move |_| { refresh_backend.send(BluetoothCommand::RefreshConnected); }, "Refresh" } }
                            p { class: "status-copy", "{status_text}" }
                            if let Some(host_name) = hid_host.name { p { class: "muted", "Bluetooth HID host: {host_name}" } }
                            if !hid_is_ready { p { class: "muted", "Open a control surface to start Android Bluetooth HID." } }
                        }
                        section { class: "quick-grid",
                            QuickCard { title: "Mouse / keyboard", detail: "Use the screen as a pointer and key surface.", onclick: move |_| { selected_kind.set(LayoutKind::Touchpad); selected_layout.set(None); screen.set(Screen::Surface); } }
                            QuickCard { title: "Gamepad", detail: "Use the screen as an Xbox 360 style gamepad.", onclick: move |_| { selected_kind.set(LayoutKind::Gamepad); selected_layout.set(default_layout_index(&layouts(), LayoutKind::Gamepad)); screen.set(Screen::Surface); gamepad_backend.send(BluetoothCommand::StartHid); } }
                        }
                    },
                    Screen::Devices => rsx! {
                        section { class: "panel",
                            div { class: "section-heading",
                                div { h2 { "Bluetooth devices" } p { class: "muted", "Pair a device in system settings if it is not shown." } }
                                button { class: "button primary", onclick: move |_| { bluetooth_status.set("Scanning for nearby devices…".into()); scan_backend.send(BluetoothCommand::Scan); }, "Scan" }
                            }
                            p { class: "status-copy", "{status_text}" }
                            if device_list.is_empty() {
                                div { class: "empty-state", "No Bluetooth devices yet. Start a scan to find nearby hardware." }
                            } else {
                                div { class: "device-list",
                                for device in device_list.clone().into_iter() {
                                    {
                                        let name = device.name.clone().unwrap_or_else(|| "Unnamed device".into());
                                        let state_name = state_name(&device.state);
                                        let device_action = match device.state {
                                            ConnectionState::Connected | ConnectionState::Connecting => "Disconnect",
                                            ConnectionState::Paired => "Connect",
                                            _ => "Pair",
                                        };
                                        let device_command = match device.state {
                                            ConnectionState::Connected | ConnectionState::Connecting => BluetoothCommand::Disconnect(device.id.clone()),
                                            ConnectionState::Paired => BluetoothCommand::Connect(device.id.clone()),
                                            _ => BluetoothCommand::Pair(device.id.clone()),
                                        };
                                        let device_status = match device.rssi_dbm {
                                            Some(signal) => format!("{state_name} · {signal} dBm"),
                                            None => state_name.to_string(),
                                        };
                                        let row_backend = app_backend.clone();
                                        rsx! { article { class: "device-row",
                                            div { class: "device-symbol", "◉" }
                                            div { class: "device-info", h3 { "{name}" }, p { "{device_status}" } }
                                            if !device.id.starts_with("system:") {
                                                button { class: "button secondary", onclick: move |_| row_backend.send(device_command.clone()), "{device_action}" }
                                            }
                                        } }
                                    }
                                    }
                                }
                            }
                        }
                    },
                    Screen::Layouts => rsx! {
                        section { class: "panel layout-list-panel",
                            div { class: "layout-grid",
                                for (index, layout) in layouts().into_iter().enumerate() {
                                    {
                                        let title = layout.name.clone();
                                        let kind = layout.kind;
                                        let icon = match kind {
                                            LayoutKind::Keyboard60 | LayoutKind::Keyboard80 | LayoutKind::Keyboard100 => "⌨",
                                            LayoutKind::Touchpad => "▱",
                                            LayoutKind::Gamepad => "◉",
                                            LayoutKind::Custom => "▦",
                                        };
                                        let layout_backend = app_backend.clone();
                                        rsx! { LayoutCard { title, icon: icon.to_string(), selected: selected_layout() == Some(index), onclick: move |_| { selected_kind.set(kind); selected_layout.set(Some(index)); screen.set(Screen::Surface); edit_mode.set(false); layout_backend.send(BluetoothCommand::StartHid); } } }
                                    }
                                }
                            }
                        }
                    },
                    Screen::CreateLayout => rsx! {
                        section { class: "panel form-panel",
                            input { class: "text-input", placeholder: "Name this layout", value: "{layout_name}", oninput: move |event| layout_name.set(event.value()) }
                            div { class: "form-actions",
                                button { class: "button primary", onclick: move |_| {
                                    let name = layout_name().trim().to_string();
                                    if name.is_empty() { page_status.set("Enter a name before saving.".into()); }
                                    else {
                                        let mut updated = layouts();
                                        updated.push(SavedLayout { name: name.clone(), kind: LayoutKind::Custom, is_default: false, controls: vec![default_control()], geometry: vec![default_geometry(0)], hardware_mappings: Vec::new() });
                                        match save_layouts(&updated) {
                                            Ok(()) => { let index = updated.len() - 1; layouts.set(updated); selected_layout.set(Some(index)); selected_kind.set(LayoutKind::Custom); controls.set(vec![default_control()]); edit_mode.set(false); screen.set(Screen::Surface); create_backend.send(BluetoothCommand::StartHid); }
                                            Err(error) => page_status.set(format!("Could not save layout: {error}")),
                                        }
                                    }
                                }, "Save layout" }
                                button { class: "button secondary", onclick: move |_| screen.set(Screen::Layouts), "Cancel" }
                            }
                            if !page_status().is_empty() { p { class: "form-status", "{page_status}" } }
                        }
                    },
                    Screen::Surface => {
                        let touch_move_backend = surface_touchpad_backend.clone();
                        let touch_click_backend = surface_touchpad_backend.clone();
                        let (touch_left_label, touch_left_action) = configured_control(&current_controls, "mouse-left", "Left click", ControlAction::MouseButton(1));
                        let (touch_middle_label, touch_middle_action) = configured_control(&current_controls, "mouse-middle", "Middle click", ControlAction::MouseButton(4));
                        let (touch_right_label, touch_right_action) = configured_control(&current_controls, "mouse-right", "Right click", ControlAction::MouseButton(2));
                        let current_keyboard_form_factor =
                            selected_kind().keyboard_form_factor().unwrap_or_default();
                        let keyboard_main_rows =
                            full_keyboard_main_rows(current_keyboard_form_factor);
                        let keyboard_aux_rows =
                            full_keyboard_aux_rows(current_keyboard_form_factor);
                        let keyboard_numpad_rows = keyboard_numpad_rows();
                        rsx! {
                        section { class: if selected_kind().keyboard_form_factor().is_some() { "surface-panel keyboard-panel" } else { "surface-panel" },
                            if selected_kind() == LayoutKind::Custom {
                                div { class: "custom-surface-grid",
                                    for (index, control) in current_controls.clone().into_iter().enumerate() {
                                        {
                                            let geometry = preview_geometry()
                                                .filter(|(preview_index, _)| *preview_index == index)
                                                .map(|(_, geometry)| geometry)
                                                .unwrap_or_else(|| current_geometry.get(index).copied().unwrap_or_else(|| default_geometry(index)));
                                            rsx! {
                                                CustomControlView {
                                                    index,
                                                    control,
                                                    geometry,
                                                    edit,
                                                    backend: app_backend.clone(),
                                                    active_controls,
                                                    layouts,
                                                    selected_layout,
                                                    layout_undo,
                                                    layout_redo,
                                                    gesture_state,
                                                    preview_geometry,
                                                    screen,
                                                    button_editor_index,
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if selected_kind() == LayoutKind::Gamepad {
                                GamepadLayout { backend: surface_gamepad_backend.clone(), controls: current_controls.clone() }
                            } else if selected_kind().keyboard_form_factor().is_some() {
                                div { class: match current_keyboard_form_factor {
                                    KeyboardFormFactor::Compact60 => "keyboard-surface keyboard-60",
                                    KeyboardFormFactor::Tenkeyless80 => "keyboard-surface keyboard-80",
                                    KeyboardFormFactor::Full100 => "keyboard-surface keyboard-100",
                                },
                                    div { class: "keyboard-main",
                                        for row in keyboard_main_rows.clone() {
                                            div { class: "keyboard-row",
                                                for key in row {
                                                    if key.usage == 0 {
                                                        div { class: "keyboard-spacer", style: "flex:{key.flex};" }
                                                    } else {
                                                        {
                                                            let (label, usage) = configured_keyboard_key(key, &current_controls);
                                                            rsx! { KeyboardSurfaceKey { label, usage, flex: key.flex, class_name: key.class_name.to_string(), backend: app_backend.clone(), pressed_keys: keyboard_pressed_keys } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if current_keyboard_form_factor != KeyboardFormFactor::Compact60 {
                                        div { class: "keyboard-aux",
                                            for row in keyboard_aux_rows.clone() {
                                                div { class: "keyboard-row",
                                                    for key in row {
                                                        if key.usage == 0 {
                                                            div { class: "keyboard-spacer", style: "flex:{key.flex};" }
                                                        } else {
                                                            {
                                                                let (label, usage) = configured_keyboard_key(key, &current_controls);
                                                                rsx! { KeyboardSurfaceKey { label, usage, flex: key.flex, class_name: key.class_name.to_string(), backend: app_backend.clone(), pressed_keys: keyboard_pressed_keys } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if current_keyboard_form_factor == KeyboardFormFactor::Full100 {
                                        div { class: "keyboard-numpad",
                                            for row in keyboard_numpad_rows.clone() {
                                                div { class: "keyboard-row",
                                                    for key in row {
                                                        if key.usage == 0 {
                                                            div { class: "keyboard-spacer", style: "flex:{key.flex};" }
                                                        } else {
                                                            {
                                                                let (label, usage) = configured_keyboard_key(key, &current_controls);
                                                                rsx! { KeyboardSurfaceKey { label, usage, flex: key.flex, class_name: key.class_name.to_string(), backend: app_backend.clone(), pressed_keys: keyboard_pressed_keys } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div {
                                    id: "touch-surface",
                                    class: "touch-surface",
                                    onpointerdown: move |event| {
                                        if touchpad_gesture().is_some() { return; }
                                        let point = event.data().client_coordinates();
                                        let pointer_id = event.data().pointer_id();
                                        touchpad_gesture.set(Some(TouchpadGesture {
                                            pointer_id,
                                            start_x: point.x,
                                            start_y: point.y,
                                            last_x: point.x,
                                            last_y: point.y,
                                            moved: false,
                                        }));
                                        let script = format!("document.getElementById('touch-surface')?.setPointerCapture({pointer_id});");
                                        spawn(async move { let _ = dioxus::document::eval(&script).await; });
                                    },
                                    onpointermove: move |event| {
                                        let Some(mut gesture) = touchpad_gesture() else { return; };
                                        let pointer_id = event.data().pointer_id();
                                        if pointer_id != gesture.pointer_id { return; }
                                        let point = event.data().client_coordinates();
                                        let dx = (point.x - gesture.last_x).round().clamp(-127.0, 127.0) as i8;
                                        let dy = (point.y - gesture.last_y).round().clamp(-127.0, 127.0) as i8;
                                        gesture.last_x = point.x;
                                        gesture.last_y = point.y;
                                        gesture.moved |= (point.x - gesture.start_x).hypot(point.y - gesture.start_y) >= 5.0;
                                        touchpad_gesture.set(Some(gesture));
                                        if dx != 0 || dy != 0 {
                                            touch_move_backend.send(BluetoothCommand::HidMouse { buttons: 0, dx, dy, wheel: 0 });
                                        }
                                    },
                                    onpointerup: move |event| {
                                        let Some(gesture) = touchpad_gesture() else { return; };
                                        if event.data().pointer_id() != gesture.pointer_id { return; }
                                        touchpad_gesture.set(None);
                                        if !gesture.moved {
                                            if let ControlAction::MouseButton(buttons) = touch_left_action { touch_click_backend.send(BluetoothCommand::HidClick(buttons as i8)); }
                                        }
                                    },
                                    onpointercancel: move |_| touchpad_gesture.set(None),
                                    span { "Drag to move · tap to click" }
                                }
                                div { class: "mouse-actions",
                                    button { class: "button secondary", onclick: move |_| if let ControlAction::MouseButton(buttons) = touch_left_action { surface_left_backend.send(BluetoothCommand::HidClick(buttons as i8)); }, "{touch_left_label}" }
                                    button { class: "button secondary", onclick: move |_| if let ControlAction::MouseButton(buttons) = touch_middle_action { surface_middle_backend.send(BluetoothCommand::HidClick(buttons as i8)); }, "{touch_middle_label}" }
                                    button { class: "button secondary", onclick: move |_| if let ControlAction::MouseButton(buttons) = touch_right_action { surface_right_backend.send(BluetoothCommand::HidClick(buttons as i8)); }, "{touch_right_label}" }
                                }
                            }
                        }
                    }
                    },
                    Screen::Settings => rsx! {
                        section { class: "panel form-panel",
                            if selected_kind() == LayoutKind::Custom {
                                if let Some(index) = selected_layout() {
                                    input { class: "text-input", value: "{layouts().get(index).map(|layout| layout.name.as_str()).unwrap_or(\"\")}", oninput: move |event| {
                                        let mut updated = layouts();
                                        if let Some(layout) = updated.get_mut(index) { layout.name = event.value(); let _ = save_layouts(&updated); layouts.set(updated); }
                                    } }
                                }
                                p { class: "muted", "Changes are saved on this device." }
                            } else {
                                if let Some(index) = selected_layout() {
                                    if let Some(layout) = layouts().get(index).cloned() {
                                        p { class: "muted", "Edit labels and output mappings. Changes are saved on this device." }
                                        if layout.is_default {
                                            button { class: "button secondary", onclick: move |_| {
                                                let mut updated = layouts();
                                                if index < updated.len() {
                                                    updated[index] = default_layout(layout.kind);
                                                    if save_layouts(&updated).is_ok() {
                                                        layouts.set(updated);
                                                        layout_undo.set(Vec::new());
                                                        layout_redo.set(Vec::new());
                                                    }
                                                }
                                            }, "Restore defaults" }
                                        }
                                        div { class: "builtin-control-list",
                                            for (control_index, control) in layout.controls.iter().cloned().enumerate() {
                                                div { class: "builtin-control-row",
                                                    input { class: "text-input compact-input", aria_label: "Control label", value: "{control.label}", oninput: move |event| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| control.label = event.value()) }
                                                    select { class: "text-input compact-input", aria_label: "Output mapping", value: "{action_value(control.action)}", onchange: move |event| {
                                                        if let Some(action) = parse_action_value(&event.value()) {
                                                            update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| control.action = action);
                                                        }
                                                    },
                                                        for (value, label) in layout_action_choices(layout.kind) { option { value: "{value}", selected: action_value(control.action) == value, "{label}" } }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            button { class: "button secondary", onclick: move |_| screen.set(Screen::Surface), "Done" }
                        }
                    },
                    Screen::ButtonEditor => rsx! {
                        section { class: "panel mapping-editor-panel",
                            if let (Some(layout_index), Some(control_index)) = (selected_layout(), button_editor_index()) {
                                if let Some(control) = layouts().get(layout_index).and_then(|layout| layout.controls.get(control_index)).cloned() {
                                    nav { class: "mapping-tabs", aria_label: "Button mapping tabs",
                                        for (tab, title) in [(0_u8, "Keyboard"), (1, "Function"), (2, "Text"), (3, "Hardware"), (4, "Settings")] {
                                            button { class: if button_editor_tab() == tab { "mapping-tab active" } else { "mapping-tab" }, onclick: move |_| button_editor_tab.set(tab), "{title}" }
                                        }
                                    }
                                    if button_editor_tab() == 0 {
                                        div { class: "mapping-content",
                                            p { class: "muted", "Choose a keyboard key to send when this button is pressed." }
                                            div { class: "mapping-choice-grid",
                                                for (label, usage) in keyboard_mapping_choices() {
                                                    button { class: if control.action == ControlAction::Keyboard(usage) && control.text_output.is_none() { "mapping-choice selected" } else { "mapping-choice" }, onclick: move |_| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| { control.action = ControlAction::Keyboard(usage); control.text_output = None; }), "{label}" }
                                                }
                                            }
                                        }
                                    }
                                    if button_editor_tab() == 1 {
                                        div { class: "mapping-content",
                                            p { class: "muted", "Choose an action for this button." }
                                            div { class: "mapping-choice-grid",
                                                for (label, action) in [("Gamepad A", ControlAction::GamepadButton(0)), ("Gamepad B", ControlAction::GamepadButton(1)), ("Gamepad X", ControlAction::GamepadButton(2)), ("Gamepad Y", ControlAction::GamepadButton(3)), ("Mouse left", ControlAction::MouseButton(1)), ("Bluetooth settings", ControlAction::BluetoothSettings)] {
                                                    button { class: if control.action == action && control.text_output.is_none() { "mapping-choice selected" } else { "mapping-choice" }, onclick: move |_| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| { control.action = action; control.text_output = None; }), "{label}" }
                                                }
                                            }
                                        }
                                    }
                                    if button_editor_tab() == 2 {
                                        div { class: "mapping-content",
                                            label { class: "mapping-label", "Text to type" }
                                            textarea { class: "text-input mapping-textarea", aria_label: "Text to type", value: "{control.text_output.clone().unwrap_or_default()}", oninput: move |event| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| { control.text_output = if event.value().is_empty() { None } else { Some(event.value()) }; }) }
                                            p { class: "muted", "Text is sent as keyboard keystrokes using a US keyboard layout." }
                                        }
                                    }
                                    if button_editor_tab() == 3 {
                                        div { class: "mapping-content",
                                            p { class: "muted", "Press a physical keyboard or gamepad button on this device to assign it." }
                                            if let Some(LearnTarget::Button(target)) = hardware_learning() {
                                                if target == control_index { div { class: "hardware-listening", "Waiting for hardware input…" } }
                                            } else if let Some(key) = control.hardware_key.as_ref() {
                                                div { class: "hardware-assignment", "Assigned input: {key}" }
                                            } else {
                                                div { class: "hardware-assignment muted", "No hardware input assigned" }
                                            }
                                            div { class: "form-actions",
                                                button { class: "button primary", onclick: move |_| hardware_learning.set(Some(LearnTarget::Button(control_index))), if matches!(hardware_learning(), Some(LearnTarget::Button(index)) if index == control_index) { "Listening…" } else { "Assign hardware input" } }
                                                if control.hardware_key.is_some() {
                                                    button { class: "button secondary", onclick: move |_| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| control.hardware_key = None), "Clear" }
                                                }
                                            }
                                        }
                                    }
                                    if button_editor_tab() == 4 {
                                        div { class: "mapping-content",
                                            label { class: "mapping-label", "Button name" }
                                            input { class: "text-input mapping-name-input", aria_label: "Button name", value: "{control.label}", oninput: move |event| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| control.label = event.value()) }
                                            label { class: "mapping-toggle", input { r#type: "checkbox", checked: control.show_text, onchange: move |event| update_custom_control(layouts, selected_layout, layout_undo, layout_redo, control_index, |control| control.show_text = event.checked()) } "Show button label" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Screen::LayoutHardwareMappings => rsx! {
                        section { class: "panel mapping-editor-panel",
                            div { class: "mapping-content",
                                h2 { "Hidden hardware mappings" }
                                p { class: "muted", "These mappings do not add buttons to the layout. They work whenever this layout is active." }
                                div { class: "hidden-mapping-create",
                                    select { aria_label: "Action for hidden hardware mapping", value: "{layout_mapping_action()}", onchange: move |event| layout_mapping_action.set(event.value()),
                                        for button in 0_u8..16 {
                                            option { value: "gamepad-{button}", "Gamepad {gamepad_button_name(button)}" }
                                        }
                                        option { value: "key-space", "Space" }
                                        option { value: "key-enter", "Enter" }
                                        option { value: "mouse-left", "Mouse left" }
                                        option { value: "bluetooth-settings", "Bluetooth settings" }
                                    }
                                    button { class: "button primary", onclick: move |_| hardware_learning.set(Some(LearnTarget::Layout(action_from_id(&layout_mapping_action())))), if matches!(hardware_learning(), Some(LearnTarget::Layout(_))) { "Press hardware input…" } else { "Add hidden mapping" } }
                                }
                                if let Some(layout_index) = selected_layout() {
                                    if let Some(layout) = layouts().get(layout_index) {
                                        if layout.hardware_mappings.is_empty() {
                                            div { class: "empty-state", "No hidden hardware mappings yet." }
                                        } else {
                                            div { class: "hidden-mapping-list",
                                                for (mapping_index, mapping) in layout.hardware_mappings.iter().enumerate() {
                                                    div { class: "hidden-mapping-row",
                                                        div { class: "hidden-mapping-info", strong { "{mapping.hardware_key}" }, span { " → {action_label(mapping.action)}" } }
                                                        button { class: "icon-button danger-icon", aria_label: "Remove hidden mapping", onclick: move |_| {
                                                            let mut updated = layouts();
                                                            if let Some(layout) = updated.get_mut(layout_index) {
                                                                if mapping_index < layout.hardware_mappings.len() {
                                                                    layout_undo.with_mut(|history| history.push(layout_snapshot(layout)));
                                                                    layout_redo.set(Vec::new());
                                                                    layout.hardware_mappings.remove(mapping_index);
                                                                    if save_layouts(&updated).is_ok() { layouts.set(updated); }
                                                                }
                                                            }
                                                        }, "×" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }

        }
    }
}

#[component]
fn AppTopBar(
    current_screen: Screen,
    is_drawer_open: bool,
    title: String,
    selected_kind: LayoutKind,
    edit: bool,
    control_count: usize,
    undo_available: bool,
    redo_available: bool,
    mut screen: Signal<Screen>,
    mut drawer_open: Signal<bool>,
    mut edit_mode: Signal<bool>,
    mut layout_name: Signal<String>,
    mut page_status: Signal<String>,
    mut layouts: Signal<Vec<SavedLayout>>,
    selected_layout: Signal<Option<usize>>,
    mut layout_undo: Signal<Vec<LayoutSnapshot>>,
    mut layout_redo: Signal<Vec<LayoutSnapshot>>,
) -> Element {
    rsx! {
        header { class: if is_drawer_open { "topbar drawer-open" } else { "topbar" },
            button { class: "icon-button menu-toggle", aria_label: if is_drawer_open { "Close navigation menu" } else { "Open navigation menu" }, onclick: move |_| drawer_open.set(!is_drawer_open), if is_drawer_open { "×" } else { "☰" } }
            if !matches!(current_screen, Screen::Dashboard | Screen::Devices | Screen::Layouts) {
                button { class: "icon-button", aria_label: "Back", onclick: move |_| {
                    screen.set(match current_screen {
                        Screen::Settings | Screen::ButtonEditor | Screen::LayoutHardwareMappings => Screen::Surface,
                        _ => Screen::Layouts,
                    });
                }, "←" }
            }
            div { class: "title-block",
                h1 { "{title}" }
                if current_screen == Screen::Layouts {
                    p { "Build and save control surfaces for your devices." }
                }
            }
            if current_screen == Screen::Layouts {
                button { class: "icon-button primary-icon", aria_label: "New layout", onclick: move |_| {
                    layout_name.set(String::new());
                    page_status.set(String::new());
                    screen.set(Screen::CreateLayout);
                }, "+" }
            }
            if current_screen == Screen::Surface && selected_kind == LayoutKind::Custom {
                if edit {
                    button { class: "icon-button", aria_label: "Layout hardware mappings", onclick: move |_| screen.set(Screen::LayoutHardwareMappings), "⌨" }
                }
                button { class: "icon-button", aria_label: "Edit layout", onclick: move |_| edit_mode.set(!edit_mode()), if edit { "✓" } else { "✎" } }
                if edit {
                    if control_count < CUSTOM_LIMIT {
                        button { class: "icon-button primary-icon", aria_label: "Add button", onclick: move |_| {
                            if let Some(index) = selected_layout() {
                                let mut updated = layouts();
                                if let Some(layout) = updated.get_mut(index) {
                                    layout_undo.with_mut(|history| history.push(layout_snapshot(layout)));
                                    layout_redo.set(Vec::new());
                                    let number = layout.controls.len() + 1;
                                    layout.controls.push(CustomControl { label: format!("Button {number}"), ..default_control() });
                                    layout.geometry.push(default_geometry(layout.geometry.len()));
                                    if save_layouts(&updated).is_ok() { layouts.set(updated); }
                                }
                            }
                        }, "+" }
                    }
                    button { class: "icon-button", aria_label: "Undo", disabled: !undo_available, onclick: move |_| {
                        if let Some(index) = selected_layout() {
                            if let Some(previous) = layout_undo.with_mut(|history| history.pop()) {
                                let mut updated = layouts();
                                if let Some(layout) = updated.get_mut(index) {
                                        layout_redo.with_mut(|history| history.push(layout_snapshot(layout)));
                                        layout.controls = previous.controls;
                                        layout.geometry = previous.geometry;
                                        layout.hardware_mappings = previous.hardware_mappings;
                                    if save_layouts(&updated).is_ok() { layouts.set(updated); }
                                }
                            }
                        }
                    }, "↶" }
                    button { class: "icon-button", aria_label: "Redo", disabled: !redo_available, onclick: move |_| {
                        if let Some(index) = selected_layout() {
                            if let Some(next) = layout_redo.with_mut(|history| history.pop()) {
                                let mut updated = layouts();
                                if let Some(layout) = updated.get_mut(index) {
                                        layout_undo.with_mut(|history| history.push(layout_snapshot(layout)));
                                        layout.controls = next.controls;
                                        layout.geometry = next.geometry;
                                        layout.hardware_mappings = next.hardware_mappings;
                                    if save_layouts(&updated).is_ok() { layouts.set(updated); }
                                }
                            }
                        }
                    }, "↷" }
                }
                button { class: "icon-button", aria_label: "Layout settings", onclick: move |_| screen.set(Screen::Settings), "⚙" }
            }
            if current_screen == Screen::Surface && selected_kind != LayoutKind::Custom {
                button { class: "icon-button", aria_label: "Layout settings", onclick: move |_| screen.set(Screen::Settings), "⚙" }
            }
            if matches!(current_screen, Screen::Dashboard | Screen::Devices | Screen::Layouts) {
                div { class: "connection-indicators",
                    span { class: "status-dot", title: "Bluetooth" }
                    span { class: "status-label", "Bluetooth" }
                }
            }
        }
    }
}

#[component]
fn CustomControlView(
    index: usize,
    control: CustomControl,
    geometry: ControlGeometry,
    edit: bool,
    backend: Rc<Backend>,
    mut active_controls: Signal<ActiveControls>,
    mut layouts: Signal<Vec<SavedLayout>>,
    selected_layout: Signal<Option<usize>>,
    mut layout_undo: Signal<Vec<LayoutSnapshot>>,
    mut layout_redo: Signal<Vec<LayoutSnapshot>>,
    mut gesture_state: Signal<Option<PointerGesture>>,
    mut preview_geometry: Signal<Option<(usize, ControlGeometry)>>,
    mut screen: Signal<Screen>,
    mut button_editor_index: Signal<Option<usize>>,
) -> Element {
    let active_gesture = gesture_state().filter(|gesture| gesture.control_index == index);
    let moving = active_gesture.is_some();
    let frame_class = if moving {
        "control-frame is-moving"
    } else {
        "control-frame"
    };
    let label = control.label.clone();
    let action = control.action;
    let text_output = control.text_output.clone();
    let source = format!("surface:{index}");
    let source_down = source.clone();
    let source_up = source.clone();
    let source_cancel = source.clone();
    let source_leave = source.clone();
    let backend_down = backend.clone();
    let backend_up = backend.clone();
    let backend_cancel = backend.clone();
    let backend_leave = backend.clone();
    let layout_for_move = layouts;
    let selected_for_move = selected_layout;
    let undo_for_move = layout_undo;
    let redo_for_move = layout_redo;

    rsx! {
        div {
            class: frame_class,
            id: "control-frame-{index}",
            style: "left:{geometry.x}%; top:{geometry.y}%; width:{geometry.width}%; height:{geometry.height}%;",
            onpointerdown: move |event| {
                let point = event.data().client_coordinates();
                let pointer_id = event.data().pointer_id();
                async move {
                    if edit {
                        begin_pointer_gesture(index, GestureMode::Move, point.x, point.y, pointer_id, geometry, gesture_state, preview_geometry).await;
                    }
                }
            },
            onpointermove: move |event| {
                if let Some(gesture) = gesture_state().filter(|gesture| gesture.control_index == index) {
                    let point = event.data().client_coordinates();
                    preview_geometry.set(Some((index, geometry_for_pointer(gesture, point.x, point.y))));
                }
            },
            onpointerup: move |event| {
                if let Some(gesture) = gesture_state().filter(|gesture| gesture.control_index == index) {
                    let point = event.data().client_coordinates();
                    let updated_geometry = geometry_for_pointer(gesture, point.x, point.y);
                    commit_control_geometry(layout_for_move, selected_for_move, undo_for_move, redo_for_move, index, updated_geometry);
                    gesture_state.set(None);
                    preview_geometry.set(None);
                }
            },
            onpointercancel: move |_| { gesture_state.set(None); preview_geometry.set(None); },
            button {
                class: "surface-control",
                aria_label: "{label}",
                onpointerdown: move |_| {
                    if !edit {
                        if let Some(text) = text_output.as_deref() {
                            send_text_output(&backend_down, text);
                        } else {
                            press_control(&backend_down, &mut active_controls, source_down.clone(), action);
                        }
                    }
                },
                onpointerup: move |_| {
                    if !edit { release_control(&backend_up, &mut active_controls, &source_up); }
                },
                onpointercancel: move |_| {
                    if !edit { release_control(&backend_cancel, &mut active_controls, &source_cancel); }
                },
                onpointerleave: move |_| if !edit { release_control(&backend_leave, &mut active_controls, &source_leave); },
                "{label}"
            }
            if edit {
                button {
                    class: "control-badge delete-badge",
                    aria_label: "Delete button",
                    onpointerdown: move |event| event.stop_propagation(),
                    onclick: move |event| {
                        event.stop_propagation();
                        if let Some(layout_index) = selected_layout() {
                            let mut updated = layouts();
                            if let Some(layout) = updated.get_mut(layout_index) {
                                if index < layout.controls.len() {
                                    layout_undo.with_mut(|history| history.push(layout_snapshot(layout)));
                                    layout_redo.set(Vec::new());
                                    layout.controls.remove(index);
                                    if index < layout.geometry.len() { layout.geometry.remove(index); }
                                    if save_layouts(&updated).is_ok() { layouts.set(updated); }
                                }
                            }
                        }
                        button_editor_index.set(None);
                    },
                    "−"
                }
                button {
                    class: "control-badge edit-badge",
                    aria_label: "Edit button",
                    onpointerdown: move |event| event.stop_propagation(),
                    onclick: move |event| {
                        event.stop_propagation();
                        button_editor_index.set(Some(index));
                        screen.set(Screen::ButtonEditor);
                    },
                    "✎"
                }
                button {
                    class: "resize-handle resize-left",
                    aria_label: "Resize button from bottom left",
                    onpointerdown: move |event| {
                        event.stop_propagation();
                        let point = event.data().client_coordinates();
                        let pointer_id = event.data().pointer_id();
                        begin_pointer_gesture(index, GestureMode::ResizeLeft, point.x, point.y, pointer_id, geometry, gesture_state, preview_geometry)
                    },
                    "◢"
                }
                button {
                    class: "resize-handle resize-right",
                    aria_label: "Resize button from bottom right",
                    onpointerdown: move |event| {
                        event.stop_propagation();
                        let point = event.data().client_coordinates();
                        let pointer_id = event.data().pointer_id();
                        begin_pointer_gesture(index, GestureMode::ResizeRight, point.x, point.y, pointer_id, geometry, gesture_state, preview_geometry)
                    },
                    "◢"
                }
            }
        }
    }
}

#[component]
fn KeyboardSurfaceKey(
    label: String,
    usage: u8,
    flex: u8,
    class_name: String,
    backend: Rc<Backend>,
    pressed_keys: Signal<HashMap<i32, u8>>,
) -> Element {
    let down_backend = backend.clone();
    let up_backend = backend.clone();
    let cancel_backend = backend;
    let down_keys = pressed_keys;
    let up_keys = pressed_keys;
    let cancel_keys = pressed_keys;

    rsx! {
        button {
            id: "keyboard-key-{usage}",
            class: "key-button {class_name}",
            style: "flex:{flex};",
            aria_label: "{label}",
            onpointerdown: move |event| {
                event.prevent_default();
                let pointer_id = event.data().pointer_id();
                update_keyboard_pointer(&down_backend, down_keys, pointer_id, usage, true);
                let script = format!("document.getElementById('keyboard-key-{usage}')?.setPointerCapture({pointer_id});");
                spawn(async move { let _ = dioxus::document::eval(&script).await; });
            },
            onpointerup: move |event| {
                update_keyboard_pointer(&up_backend, up_keys, event.data().pointer_id(), usage, false);
            },
            onpointercancel: move |event| {
                update_keyboard_pointer(&cancel_backend, cancel_keys, event.data().pointer_id(), usage, false);
            },
            "{label}"
        }
    }
}

#[component]
fn QuickCard(title: String, detail: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! { button { class: "quick-card", onclick: move |event| onclick.call(event), h3 { "{title}" }, p { "{detail}" } } }
}

#[component]
fn LayoutCard(
    title: String,
    icon: String,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! { button { class: if selected { "layout-card selected" } else { "layout-card" }, onclick: move |event| onclick.call(event), span { class: "layout-icon", "{icon}" }, span { class: "layout-title", "{title}" } } }
}

#[component]
fn GamepadLayout(backend: Rc<Backend>, controls: Vec<CustomControl>) -> Element {
    let state = use_signal(GamepadInputState::default);
    rsx! {
        div { class: "gamepad-phone-surface",
            div { class: "xbox-phone-shoulders",
                div { class: "xbox-phone-shoulder-pair",
                    { let (label, action) = configured_control(&controls, "button:4", "LB", ControlAction::GamepadButton(4)); rsx! { GamepadButton { label, action, style_class: "phone-shoulder", backend: backend.clone(), state } } }
                    { let (label, action) = configured_control(&controls, "button:6", "LT", ControlAction::GamepadButton(6)); rsx! { GamepadButton { label, action, style_class: "phone-trigger", backend: backend.clone(), state } } }
                }
                div { class: "xbox-phone-shoulder-pair",
                    { let (label, action) = configured_control(&controls, "button:5", "RB", ControlAction::GamepadButton(5)); rsx! { GamepadButton { label, action, style_class: "phone-shoulder", backend: backend.clone(), state } } }
                    { let (label, action) = configured_control(&controls, "button:7", "RT", ControlAction::GamepadButton(7)); rsx! { GamepadButton { label, action, style_class: "phone-trigger", backend: backend.clone(), state } } }
                }
            }
            div { class: "xbox-phone-controls",
                GamepadDpad { backend: backend.clone(), state, style_class: "phone-dpad", controls: controls.clone() }
                div { class: "xbox-phone-middle",
                    div { class: "xbox-phone-start-buttons",
                        { let (label, action) = configured_control(&controls, "button:8", "Back", ControlAction::GamepadButton(8)); rsx! { GamepadButton { label, action, style_class: "phone-center-button", backend: backend.clone(), state } } }
                        { let (label, action) = configured_control(&controls, "button:9", "Start", ControlAction::GamepadButton(9)); rsx! { GamepadButton { label, action, style_class: "phone-center-button", backend: backend.clone(), state } } }
                    }
                    div { class: "xbox-phone-sticks",
                        { let (label, action) = configured_control(&controls, "button:11", "L3", ControlAction::GamepadButton(11)); rsx! { GamepadStick { label, button_action: action, axis_x: 0, axis_y: 1, backend: backend.clone(), state } } }
                        { let (label, action) = configured_control(&controls, "button:12", "R3", ControlAction::GamepadButton(12)); rsx! { GamepadStick { label, button_action: action, axis_x: 2, axis_y: 3, backend: backend.clone(), state } } }
                    }
                }
                GamepadFaceButtons { backend: backend.clone(), state, controls: controls.clone() }
            }
        }
    }
}

#[component]
fn GamepadButton(
    label: String,
    action: ControlAction,
    style_class: String,
    backend: Rc<Backend>,
    state: Signal<GamepadInputState>,
    #[props(default)] accessibility_label: Option<String>,
) -> Element {
    let down_backend = backend.clone();
    let up_backend = backend.clone();
    let cancel_backend = backend;
    let control = match action {
        ControlAction::GamepadButton(button) => GamepadControl::Button(button),
        ControlAction::GamepadHat(direction) => GamepadControl::Dpad(direction),
        _ => GamepadControl::Button(0),
    };
    let accessibility_label = accessibility_label.unwrap_or_else(|| label.clone());
    rsx! { button {
        id: "gamepad-control-{label}", class: "gamepad-control {style_class}", aria_label: "{accessibility_label}",
        onpointerdown: move |event| {
            event.prevent_default();
            let pointer_id = event.data().pointer_id();
            let script = format!("document.getElementById('gamepad-control-{label}')?.setPointerCapture({pointer_id});");
            spawn(async move { let _ = dioxus::document::eval(&script).await; });
            set_gamepad_control(&down_backend, state, pointer_id, control, true);
        },
        onpointerup: move |event| set_gamepad_control(&up_backend, state, event.data().pointer_id(), control, false),
        onpointercancel: move |event| set_gamepad_control(&cancel_backend, state, event.data().pointer_id(), control, false),
        "{label}"
    } }
}

#[component]
fn GamepadStick(
    label: String,
    button_action: ControlAction,
    axis_x: usize,
    axis_y: usize,
    backend: Rc<Backend>,
    state: Signal<GamepadInputState>,
) -> Element {
    let axes = state.read().axes;
    let cap_x = axes[axis_x] as f64 / 127.0 * 26.0;
    let cap_y = axes[axis_y] as f64 / 127.0 * 26.0;
    let down_backend = backend.clone();
    let move_backend = backend.clone();
    let up_state = state;
    let up_backend = backend.clone();
    let cancel_state = state;
    let cancel_backend = backend;
    let click_button = match button_action {
        ControlAction::GamepadButton(button) => Some(button),
        _ => None,
    };
    rsx! { button {
        id: "gamepad-stick-{axis_x}", class: "gamepad-stick", aria_label: "Gamepad stick {label}",
        onpointerdown: move |event| {
            event.prevent_default();
            let pointer_id = event.data().pointer_id();
            if let Some(button) = click_button {
                set_gamepad_control(&down_backend, state, pointer_id, GamepadControl::Button(button), true);
            }
            let backend = down_backend.clone();
            spawn(async move { begin_gamepad_stick(&backend, state, pointer_id, axis_x, axis_y, click_button).await; });
        },
        onpointermove: move |event| {
            let pointer_id = event.data().pointer_id();
            let Some(gesture) = state.read().sticks.get(&pointer_id).copied() else { return; };
            let point = event.data().client_coordinates();
            state.with_mut(|state| {
                let x = ((point.x - gesture.center_x) / gesture.radius).clamp(-1.0, 1.0);
                let y = ((point.y - gesture.center_y) / gesture.radius).clamp(-1.0, 1.0);
                let magnitude = x.hypot(y).max(1.0);
                state.axes[gesture.axis_x] = (x / magnitude * 127.0).round() as i8;
                state.axes[gesture.axis_y] = (y / magnitude * 127.0).round() as i8;
            });
            send_gamepad_report(&move_backend, &state);
        },
        onpointerup: move |event| end_gamepad_stick(&up_backend, up_state, event.data().pointer_id()),
        onpointercancel: move |event| end_gamepad_stick(&cancel_backend, cancel_state, event.data().pointer_id()),
        div { class: "stick-cap", style: "transform:translate({cap_x}px,{cap_y}px);" }
        span { "{label}" }
    } }
}

#[component]
fn GamepadFaceButtons(
    backend: Rc<Backend>,
    state: Signal<GamepadInputState>,
    controls: Vec<CustomControl>,
) -> Element {
    rsx! {
        div { class: "face-cluster",
            { let (label, action) = configured_control(&controls, "button:3", "Y", ControlAction::GamepadButton(3)); rsx! { GamepadButton { label, action, style_class: "face-button face-top", backend: backend.clone(), state } } }
            { let (label, action) = configured_control(&controls, "button:2", "X", ControlAction::GamepadButton(2)); rsx! { GamepadButton { label, action, style_class: "face-button face-left", backend: backend.clone(), state } } }
            { let (label, action) = configured_control(&controls, "button:1", "B", ControlAction::GamepadButton(1)); rsx! { GamepadButton { label, action, style_class: "face-button face-right", backend: backend.clone(), state } } }
            { let (label, action) = configured_control(&controls, "button:0", "A", ControlAction::GamepadButton(0)); rsx! { GamepadButton { label, action, style_class: "face-button face-bottom", backend: backend.clone(), state } } }
        }
    }
}

#[component]
fn GamepadDpad(
    style_class: String,
    backend: Rc<Backend>,
    state: Signal<GamepadInputState>,
    controls: Vec<CustomControl>,
) -> Element {
    rsx! {
        div { class: "gamepad-dpad {style_class}",
            { let (label, action) = configured_control(&controls, "hat:0", "D-pad up", ControlAction::GamepadHat(0)); rsx! { GamepadButton { label: "▲".to_string(), accessibility_label: Some(label), action, style_class: "dpad-up", backend: backend.clone(), state } } }
            { let (label, action) = configured_control(&controls, "hat:1", "D-pad right", ControlAction::GamepadHat(1)); rsx! { GamepadButton { label: "▶".to_string(), accessibility_label: Some(label), action, style_class: "dpad-right", backend: backend.clone(), state } } }
            { let (label, action) = configured_control(&controls, "hat:2", "D-pad down", ControlAction::GamepadHat(2)); rsx! { GamepadButton { label: "▼".to_string(), accessibility_label: Some(label), action, style_class: "dpad-down", backend: backend.clone(), state } } }
            { let (label, action) = configured_control(&controls, "hat:3", "D-pad left", ControlAction::GamepadHat(3)); rsx! { GamepadButton { label: "◀".to_string(), accessibility_label: Some(label), action, style_class: "dpad-left", backend: backend.clone(), state } } }
        }
    }
}

fn main() {
    dioxus::launch(OpenWASDApp);
}
