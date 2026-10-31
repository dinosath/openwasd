pub use makepad_widgets;

use makepad_widgets::*;
use openwasd_bluetooth::{
    BluetoothCommand, BluetoothDeviceInfo, BluetoothEvent, BluetoothService, BtleplugAdapter,
    ConnectionState, HidHostState, merge_devices,
};
use openwasd_core::custom_layout::{ActiveControls, ControlAction, CustomControl};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
mod ui_density;
use ui_density::UiDensity;

app_main!(App);

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
enum LayoutKind {
    Keyboard,
    Gamepad,
    Custom,
    #[default]
    Touchpad,
}

impl LayoutKind {
    fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "Full keyboard",
            Self::Gamepad => "Gamepad",
            Self::Custom => "Custom controls",
            Self::Touchpad => "Touchpad",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SavedLayout {
    name: String,
    kind: LayoutKind,
    #[serde(default)]
    controls: Vec<CustomControl>,
}

const CUSTOM_LIMIT: usize = 6;
const CUSTOM_OUTPUTS: &[(&str, ControlAction)] = &[
    ("Keyboard · Space", ControlAction::Keyboard(44)),
    ("Keyboard · Enter", ControlAction::Keyboard(40)),
    ("Keyboard · Escape", ControlAction::Keyboard(41)),
    ("Keyboard · W", ControlAction::Keyboard(26)),
    ("Keyboard · A", ControlAction::Keyboard(4)),
    ("Keyboard · S", ControlAction::Keyboard(22)),
    ("Keyboard · D", ControlAction::Keyboard(7)),
    ("Keyboard · B", ControlAction::Keyboard(5)),
    ("Keyboard · C", ControlAction::Keyboard(6)),
    ("Keyboard · E", ControlAction::Keyboard(8)),
    ("Keyboard · F", ControlAction::Keyboard(9)),
    ("Keyboard · G", ControlAction::Keyboard(10)),
    ("Keyboard · H", ControlAction::Keyboard(11)),
    ("Keyboard · I", ControlAction::Keyboard(12)),
    ("Keyboard · J", ControlAction::Keyboard(13)),
    ("Keyboard · K", ControlAction::Keyboard(14)),
    ("Keyboard · L", ControlAction::Keyboard(15)),
    ("Keyboard · M", ControlAction::Keyboard(16)),
    ("Keyboard · N", ControlAction::Keyboard(17)),
    ("Keyboard · O", ControlAction::Keyboard(18)),
    ("Keyboard · P", ControlAction::Keyboard(19)),
    ("Keyboard · Q", ControlAction::Keyboard(20)),
    ("Keyboard · R", ControlAction::Keyboard(21)),
    ("Keyboard · T", ControlAction::Keyboard(23)),
    ("Keyboard · U", ControlAction::Keyboard(24)),
    ("Keyboard · V", ControlAction::Keyboard(25)),
    ("Keyboard · X", ControlAction::Keyboard(27)),
    ("Keyboard · Y", ControlAction::Keyboard(28)),
    ("Keyboard · Z", ControlAction::Keyboard(29)),
    ("Keyboard · 1", ControlAction::Keyboard(30)),
    ("Keyboard · 2", ControlAction::Keyboard(31)),
    ("Keyboard · 3", ControlAction::Keyboard(32)),
    ("Keyboard · 4", ControlAction::Keyboard(33)),
    ("Keyboard · 5", ControlAction::Keyboard(34)),
    ("Keyboard · 6", ControlAction::Keyboard(35)),
    ("Keyboard · 7", ControlAction::Keyboard(36)),
    ("Keyboard · 8", ControlAction::Keyboard(37)),
    ("Keyboard · 9", ControlAction::Keyboard(38)),
    ("Keyboard · 0", ControlAction::Keyboard(39)),
    ("Keyboard · Backspace", ControlAction::Keyboard(42)),
    ("Keyboard · Tab", ControlAction::Keyboard(43)),
    ("Keyboard · Up", ControlAction::Keyboard(82)),
    ("Keyboard · Down", ControlAction::Keyboard(81)),
    ("Keyboard · Left", ControlAction::Keyboard(80)),
    ("Keyboard · Right", ControlAction::Keyboard(79)),
    ("Volume · Up", ControlAction::Keyboard(128)),
    ("Volume · Down", ControlAction::Keyboard(129)),
    ("Gamepad · A", ControlAction::GamepadButton(0)),
    ("Gamepad · B", ControlAction::GamepadButton(1)),
    ("Gamepad · X", ControlAction::GamepadButton(2)),
    ("Gamepad · Y", ControlAction::GamepadButton(3)),
    ("Gamepad · L1", ControlAction::GamepadButton(4)),
    ("Gamepad · R1", ControlAction::GamepadButton(5)),
    ("Gamepad · L2", ControlAction::GamepadButton(6)),
    ("Gamepad · R2", ControlAction::GamepadButton(7)),
    ("Gamepad · Select", ControlAction::GamepadButton(8)),
    ("Gamepad · Start", ControlAction::GamepadButton(9)),
    ("Gamepad · Home", ControlAction::GamepadButton(10)),
    ("Mouse · Left click", ControlAction::MouseButton(1)),
    ("Mouse · Right click", ControlAction::MouseButton(2)),
    ("Mouse · Middle click", ControlAction::MouseButton(4)),
    ("Bluetooth · Settings", ControlAction::BluetoothSettings),
];

fn custom_output_label(action: ControlAction) -> String {
    if let Some((label, _)) = CUSTOM_OUTPUTS
        .iter()
        .find(|(_, candidate)| *candidate == action)
    {
        return (*label).to_string();
    }
    match action {
        ControlAction::Keyboard(usage) => {
            let key = match usage {
                40 => "Enter",
                41 => "Escape",
                42 => "Backspace",
                43 => "Tab",
                44 => "Space",
                45 => "-",
                46 => "=",
                47 => "[",
                48 => "]",
                49 => "\\",
                51 => ";",
                52 => "'",
                53 => "`",
                54 => ",",
                55 => ".",
                56 => "/",
                57 => "Caps Lock",
                58..=69 => return format!("Keyboard · F{}", usage - 57),
                224 => "Left Ctrl",
                225 => "Left Shift",
                226 => "Left Alt",
                227 => "Left Meta",
                228 => "Right Ctrl",
                229 => "Right Shift",
                230 => "Right Alt",
                231 => "Right Meta",
                4..=29 => return format!("Keyboard · {}", (b'A' + usage - 4) as char),
                30..=38 => return format!("Keyboard · {}", (b'1' + usage - 30) as char),
                39 => "0",
                _ => "Key",
            };
            format!("Keyboard · {key}")
        }
        ControlAction::GamepadButton(button) => format!("Gamepad · Button {button}"),
        ControlAction::MouseButton(button) => format!("Mouse · Button {button}"),
        ControlAction::BluetoothSettings => "Bluetooth · Settings".into(),
    }
}

fn custom_control_output_label(control: &CustomControl) -> String {
    control
        .text_output
        .as_ref()
        .map(|text| format!("Text · {text}"))
        .unwrap_or_else(|| custom_output_label(control.action))
}

fn custom_control_icon(control: &CustomControl) -> &str {
    control
        .custom_icon
        .as_deref()
        .unwrap_or_else(|| match control.action {
            ControlAction::Keyboard(_) => "⌨",
            ControlAction::GamepadButton(_) => "🎮",
            ControlAction::MouseButton(_) => "🖱",
            ControlAction::BluetoothSettings => "⚙",
        })
}

fn custom_control_surface_label(control: &CustomControl) -> String {
    let icon = if control.show_icon {
        format!("{} ", custom_control_icon(control))
    } else {
        String::new()
    };
    let text = if control.show_text {
        control.label.as_str()
    } else {
        ""
    };
    format!("{icon}{text}").trim().to_string()
}

fn ascii_to_hid_key(character: char) -> Option<(u8, u8)> {
    let value = character as u8;
    if !character.is_ascii() {
        return None;
    }
    match character {
        'a'..='z' => Some((4 + (value - b'a'), 0)),
        'A'..='Z' => Some((4 + (value - b'A'), 2)),
        '1'..='9' => Some((30 + (value - b'1'), 0)),
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
}

fn custom_output_group(action: ControlAction) -> u8 {
    match action {
        ControlAction::Keyboard(128 | 129) => 1,
        ControlAction::Keyboard(_) => 0,
        ControlAction::GamepadButton(_) => 2,
        ControlAction::MouseButton(_) => 3,
        ControlAction::BluetoothSettings => 4,
    }
}

fn default_custom_controls() -> Vec<CustomControl> {
    vec![CustomControl {
        label: "clickme".into(),
        action: ControlAction::GamepadButton(0),
        text_output: None,
        hardware_key: None,
        show_text: true,
        show_icon: false,
        custom_icon: None,
    }]
}

fn saved_layouts_path() -> Option<PathBuf> {
    robius_directories::ProjectDirs::from("io", "openwasd", "OpenWASD")
        .map(|dirs| dirs.data_local_dir().join("layouts.json"))
}

fn load_saved_layouts() -> Vec<SavedLayout> {
    let Some(path) = saved_layouts_path() else {
        return Vec::new();
    };
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn persist_saved_layouts(layouts: &[SavedLayout]) -> Result<(), String> {
    let path =
        saved_layouts_path().ok_or_else(|| "App data directory is unavailable.".to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "Layout storage path is invalid.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let json = serde_json::to_vec_pretty(layouts).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| error.to_string())
}

script_mod! {
    use mod.prelude.widgets.*
    let state = mod.state
    let AppButton = Button {
        draw_bg.border_size: 0.0
        draw_bg.border_radius: 8.0
        draw_bg.color: #eef3f7
        draw_bg.color_hover: #dcebf0
        draw_bg.color_down: #c9e8ed
        draw_bg.color_focus: #e1f2f5
        draw_bg.color_disabled: #eef3f7
        draw_text.color: #x182b3c
        draw_text.color_hover: #x182b3c
        draw_text.color_down: #x182b3c
        draw_text.color_focus: #x182b3c
        draw_text.color_disabled: #x718597
    }
    let AppIconButton = AppButton {
        width: 40
        height: 40
        draw_bg.border_radius: 8.0
        draw_bg.color: #eef3f7
        draw_bg.color_hover: #dcebf0
        draw_bg.color_down: #c9e8ed
        draw_bg.color_focus: #e1f2f5
        draw_icon.color: #x00a7b8
        icon_walk: Walk{ width: 20, height: 20 }
    }
    let AppLayoutButton = AppButton {
        height: 56
        draw_bg.border_radius: 10.0
        draw_bg.color: #eef3f7
        draw_bg.color_hover: #dcebf0
        draw_bg.color_down: #c9e8ed
        draw_bg.color_focus: #e1f2f5
        draw_text.color: #x213246
        draw_text.text_style.font_size: 13
    }
    let AppPageTitle = Label {
        width: Fill
        draw_text.color: #x18263a
        draw_text.text_style.font_size: if state.is_narrow { 17 } else { 22 }
    }
    let AppPageSubtitle = Label {
        width: Fill
        draw_text.color: #x6b7c8c
        draw_text.text_style.font_size: if state.is_narrow { 12 } else { 15 }
    }
    let AppTopBar = View {
        width: Fill
        height: state.header_button_size
        flow: Right
        spacing: if state.is_narrow { 5 } else { 14 }
        align: Align{ y: 0.5 }
    }
    let AppTextInput = TextInput {
        draw_bg.color: #eef3f7
        draw_bg.color_empty: #eef3f7
        draw_bg.color_hover: #e5f3f5
        draw_bg.color_focus: #ffffff
        draw_bg.border_color: #d4e3e9
        draw_bg.border_color_empty: #d4e3e9
        draw_bg.border_color_focus: #x00a7b8
        draw_text.color: #x182b3c
        draw_text.color_empty: #x718597
        draw_text.color_empty_focus: #x718597
        draw_text.color_focus: #x182b3c
    }

    startup() do #(App::script_component(vm)) {
        ui: Root {
            on_startup: || {
                ui.main_view.render()
            }

            main_window := Window {
                window.inner_size: vec2(state.window_width, state.window_height)
                pass +: { clear_color: #xf5f8fa }

                body +: {
                    main_view := View {
                        width: Fill
                        height: Fill
                        flow: Overlay
                        show_bg: true
                        draw_bg +: { color: #xf5f8fa }

                        on_render: || {
                            content := View {
                                width: Fill
                                height: Fill
                                flow: Down
                                spacing: if state.current_screen == "layouts" && state.is_narrow { state.page_inset } else { state.page_gap }
                                margin: Inset{ left: 0 }
                                padding: Inset{ top: state.page_inset, left: state.page_inset, right: state.page_inset, bottom: if state.current_screen == "layout_surface" && state.is_narrow { 0 } else { state.page_bottom_inset } }
                                show_bg: true
                                draw_bg +: { color: #xf5f8fa }

                                page_header := AppTopBar {
                                    visible: true

                                    layout_settings_back := AppIconButton {
                                        width: state.header_button_size
                                        height: state.header_button_size
                                        visible: state.current_screen == "layout_settings"
                                        text: "←"
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 18
                                    }

                                    layout_create_back := AppIconButton {
                                        width: state.header_button_size
                                        height: state.header_button_size
                                        visible: state.current_screen == "layout_create"
                                        text: "←"
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 18
                                    }

                                    mobile_menu_toggle := AppIconButton {
                                        width: state.header_button_size
                                        height: state.header_button_size
                                        visible: (state.current_screen != "layout_settings" || !state.custom_button_focused) && state.current_screen != "layout_create"
                                        text: "☰"
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 17
                                    }

                                    page_title := AppPageTitle {
                                        width: Fit
                                        visible: state.current_screen != "layout_surface" && state.current_screen != "layout_settings" && state.current_screen != "layouts"
                                        text: if state.current_screen == "devices" { "Devices" } else if state.current_screen == "layout_create" { "New layout" } else if state.current_screen == "layout_surface" { if state.selected_layout_kind == "gamepad" { "Gamepad" } else { "Mouse / Keyboard" } } else if state.current_screen == "layout_settings" { "Layout settings" } else { "Dashboard" }
                                    }

                                    layouts_heading := View {
                                        width: Fit
                                        height: Fit
                                        visible: state.current_screen == "layouts"
                                        flow: Right
                                        spacing: 4
                                        align: Align{ y: 0.5 }

                                        layouts_title := AppPageTitle {
                                            width: Fit
                                            text: "Layouts"
                                        }

                                        layouts_subtitle := AppPageSubtitle {
                                            width: Fit
                                            text: "Build and save control surfaces for your devices."
                                            draw_text.text_style.font_size: if state.is_narrow { 10 } else { 12 }
                                        }
                                    }

                                    layout_settings_heading := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.current_screen == "layout_settings"
                                        flow: Down
                                        spacing: 0

                                        layout_settings_title := AppPageTitle {
                                            text: if state.custom_button_focused { "Edit button" } else { "Layout settings" }
                                        }

                                        layout_settings_subtitle := AppPageSubtitle {
                                            visible: state.selected_layout_kind == "custom"
                                            text: ""
                                        }
                                    }

                                    layout_surface_heading := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.current_screen == "layout_surface"
                                        flow: Down
                                        spacing: 0

                                        layout_surface_title := AppPageTitle {
                                            text: if state.custom_edit_mode { "Edit layout" } else if state.selected_layout_kind == "gamepad" { "Gamepad" } else { "Mouse / Keyboard" }
                                            draw_text.text_style.font_size: state.surface_title_size
                                        }

                                        layout_surface_device_status := AppPageSubtitle {
                                            text: "Waiting for a computer to connect"
                                            draw_text.text_style.font_size: state.surface_subtitle_size
                                        }
                                    }

                                    protocol_spacer := View {
                                        width: Fill
                                        height: 1
                                        visible: state.current_screen != "layout_surface"
                                    }

                                    protocol_actions := View {
                                        width: Fit
                                        height: Fit
                                        visible: state.current_screen != "layout_surface" && state.current_screen != "layout_settings" && state.current_screen != "layout_create"
                                        flow: Right
                                        spacing: if state.is_compact { 4 } else { 8 }

                                        layout_new := AppIconButton {
                                            width: state.header_button_size
                                            height: state.header_button_size
                                            visible: state.current_screen == "layouts"
                                            text: "+"
                                            draw_text.color: #x213246
                                            draw_text.text_style.font_size: 24
                                        }

                                        wifi_connect := AppIconButton {
                                            width: if state.is_narrow { 40 } else { 82 }
                                            height: state.header_button_size
                                            enabled: false
                                            text: if state.is_narrow { "" } else { "Wi-Fi" }
                                            icon_walk: Walk{ width: 16, height: 16 }
                                            draw_icon +: {
                                                color: #x8f9aaa
                                                svg: crate_resource("self:resources/wifi.svg")
                                            }
                                            draw_bg.color: #edf3f7
                                            draw_bg.color_disabled: #edf3f7
                                            draw_text.color: #x718597
                                            draw_text.color_disabled: #x718597
                                            animator +: { disabled: { default: @on } }
                                        }

                                        bluetooth_connect := AppIconButton {
                                            width: if state.is_narrow { 40 } else { 104 }
                                            height: state.header_button_size
                                            text: if state.is_narrow { "" } else { "Bluetooth" }
                                            icon_walk: Walk{ width: 16, height: 16 }
                                            draw_icon +: {
                                                color: #x8f9aaa
                                                svg: crate_resource("self:resources/bluetooth.svg")
                                            }
                                            draw_bg.color: #edf3f7
                                            draw_bg.color_hover: #e1f2f5
                                            draw_bg.color_down: #c9e9ee
                                            draw_text.color: #x213246
                                        }

                                        usb_connect := AppIconButton {
                                            width: if state.is_narrow { 40 } else { 72 }
                                            height: state.header_button_size
                                            enabled: false
                                            text: if state.is_narrow { "" } else { "USB" }
                                            icon_walk: Walk{ width: 16, height: 16 }
                                            draw_icon +: {
                                                color: #x8f9aaa
                                                svg: crate_resource("self:resources/usb.svg")
                                            }
                                            draw_bg.color: #edf3f7
                                            draw_bg.color_disabled: #edf3f7
                                            draw_text.color: #x718597
                                            draw_text.color_disabled: #x718597
                                            animator +: { disabled: { default: @on } }
                                        }
                                    }

                                    layout_toolbar := View {
                                        width: Fit
                                        height: Fit
                                        visible: state.current_screen == "layout_surface"
                                        flow: Right
                                        spacing: if state.is_compact { 4 } else { 8 }

                                        layout_keyboard_toggle := AppIconButton {
                                            width: state.header_button_size
                                            height: state.header_button_size
                                            text: ""
                                            icon_walk: Walk{ width: if state.header_button_size == 32 { 16 } else { 20 }, height: if state.header_button_size == 32 { 16 } else { 20 } }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: crate_resource("self:resources/keyboard.svg")
                                            }
                                            draw_text.color: #x213246
                                            draw_text.text_style.font_size: 20
                                        }

                                        layout_custom_add_menu_toggle := AppIconButton {
                                            width: state.header_button_size
                                            height: state.header_button_size
                                            visible: state.selected_layout_kind == "custom" && state.custom_edit_mode
                                            text: "+"
                                            draw_bg.color: #x00bfd0
                                            draw_bg.color_hover: #x20cddd
                                            draw_bg.color_down: #x44c7d5
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 22
                                        }

                                        layout_custom_edit_toggle := AppIconButton {
                                            width: state.header_button_size
                                            height: state.header_button_size
                                            visible: state.selected_layout_kind == "custom"
                                            text: ""
                                            icon_walk: Walk{ width: if state.header_button_size == 32 { 16 } else { 20 }, height: if state.header_button_size == 32 { 16 } else { 20 } }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: if state.custom_edit_mode { crate_resource("self:resources/save.svg") } else { crate_resource("self:resources/edit.svg") }
                                            }
                                        }

                                        layout_settings_button := AppIconButton {
                                            width: state.header_button_size
                                            height: state.header_button_size
                                            visible: true
                                            text: ""
                                            icon_walk: Walk{ width: if state.header_button_size == 32 { 16 } else { 20 }, height: if state.header_button_size == 32 { 16 } else { 20 } }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: crate_resource("self:resources/settings.svg")
                                            }
                                            draw_text.color: #x213246
                                            draw_text.text_style.font_size: 20
                                        }
                                    }

                                    layout_settings_delete := AppIconButton {
                                        width: state.header_button_size
                                        height: state.header_button_size
                                        visible: state.current_screen == "layout_settings" && state.selected_layout_kind == "custom" && state.custom_edit_mode && state.custom_button_focused
                                        text: ""
                                        icon_walk: Walk{ width: if state.header_button_size == 32 { 16 } else { 20 }, height: if state.header_button_size == 32 { 16 } else { 20 } }
                                        draw_icon +: {
                                            color: #xc05050
                                            svg: crate_resource("self:resources/delete.svg")
                                        }
                                        draw_bg.color: #xfce8e8
                                        draw_bg.color_hover: #xf8d8d8
                                        draw_bg.color_down: #xf0c4c4
                                    }

                                    layout_settings_save := AppButton {
                                        width: 74
                                        height: state.header_button_size
                                        visible: state.current_screen == "layout_settings" && state.selected_layout_kind == "custom" && state.custom_button_focused
                                        text: "Save"
                                        draw_bg.color: #x0000
                                        draw_bg.color_hover: #xe5f3f5
                                        draw_bg.color_down: #xc9e8ed
                                        draw_text.color: #x006aa1
                                        draw_text.text_style.font_size: 15
                                    }
                                }

                                custom_editor_groups := View {
                                    width: Fill height: Fit visible: false flow: Right spacing: 4
                                    custom_group_keyboard := AppLayoutButton { width: Fill height: 44 text: "Keyboard shortcut" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a draw_text.text_style.font_size: 12 }
                                    custom_group_function := AppLayoutButton { width: Fill height: 44 text: "Function" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a draw_text.text_style.font_size: 12 }
                                    custom_group_text := AppLayoutButton { width: Fill height: 44 text: "Text" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a draw_text.text_style.font_size: 12 }
                                    custom_group_hardware := AppLayoutButton { width: Fill height: 44 text: "Hardware buttons" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a draw_text.text_style.font_size: 10 }
                                    custom_group_settings := AppLayoutButton { width: Fill height: 44 text: "Settings" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a draw_text.text_style.font_size: 12 }
                                }

                                scroll_body := ScrollYView {
                                    width: Fill
                                    height: Fill
                                    scroll_bars: ScrollBars {
                                        show_scroll_x: false
                                        show_scroll_y: state.current_screen != "layout_surface"
                                        scroll_bar_y.drag_scrolling: true
                                    }
                                    flow: Down
                                    spacing: 22
                                    padding: 0
                                    show_bg: true
                                    draw_bg +: { color: #xf5f8fa }

                                page_description := Label {
                                    width: Fill
                                    visible: state.current_screen != "layout_surface" && state.current_screen != "layout_settings" && state.current_screen != "layouts" && state.current_screen != "layout_create"
                                    text: if state.current_screen == "devices" {
                                        "Manage your connected controllers."
                                    } else if state.current_screen == "layouts" {
                                        "Build and save control surfaces for your devices."
                                    } else {
                                        "Manage your devices and configurations."
                                    }
                                    draw_text.color: #x6b7c8c
                                    draw_text.text_style.font_size: 14
                                }

                                device_panel := RoundedView {
                                    width: Fill
                                    height: Fit
                                    visible: state.current_screen != "layouts" && state.current_screen != "layout_surface" && state.current_screen != "layout_settings" && state.current_screen != "layout_create"
                                    flow: Down
                                    spacing: 16
                                    padding: 22
                                    show_bg: true
                                    draw_bg +: { color: #xffffffff radius: 12.0 }

                                    usb_title := Label {
                                        width: Fill
                                        text: "USB devices"
                                        draw_text.color: #x18263a
                                        draw_text.text_style.font_size: 19
                                    }
                                    usb_status := Label {
                                        width: Fill height: Fit
                                        text: "Checking connected USB devices…"
                                        draw_text.color: #x576b7d
                                        draw_text.text_style.font_size: 13
                                    }
                                    usb_capabilities := Label {
                                        width: Fill height: Fit
                                        text: "OpenWASD uses Android USB accessory mode. Select a device to connect or disconnect."
                                        draw_text.color: #x66798b
                                        draw_text.text_style.font_size: 12
                                    }
                                    usb_companion_1 := AppButton { width: Fill height: 42 visible: false text: "" }
                                    usb_inspect_1 := AppButton { width: Fill height: 36 visible: false text: "View incoming input" }
                                    usb_companion_2 := AppButton { width: Fill height: 42 visible: false text: "" }
                                    usb_inspect_2 := AppButton { width: Fill height: 36 visible: false text: "View incoming input" }
                                    usb_companion_3 := AppButton { width: Fill height: 42 visible: false text: "" }
                                    usb_inspect_3 := AppButton { width: Fill height: 36 visible: false text: "View incoming input" }
                                    usb_companion_4 := AppButton { width: Fill height: 42 visible: false text: "" }
                                    usb_inspect_4 := AppButton { width: Fill height: 36 visible: false text: "View incoming input" }
                                    usb_bridge_status := Label { width: Fill height: Fit text: "" draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                    panel_title := Label {
                                        width: Fill
                                        text: "Bluetooth devices"
                                        draw_text.color: #x18263a
                                        draw_text.text_style.font_size: 19
                                    }

                                    panel_description := Label {
                                        width: Fill
                                        text: "Scan nearby BLE devices. Pair first in system settings if prompted."
                                        draw_text.color: #x66798b
                                        draw_text.text_style.font_size: 14
                                    }

                                    refresh_devices := AppButton {
                                        width: 168
                                        height: 42
                                        text: "Scan Bluetooth"
                                        draw_bg.color: #x00bfd0
                                        draw_bg.color_hover: #x20cddd
                                        draw_bg.color_down: #x44c7d5
                                        draw_text.color: #x182b3c
                                        draw_text.text_style.font_size: 14
                                    }

                                    bluetooth_status := Label {
                                        width: Fill
                                        text: "Bluetooth scan has not started."
                                        draw_text.color: #x66798b
                                        draw_text.text_style.font_size: 13
                                    }

                                    bluetooth_empty := Label {
                                        width: Fill
                                        text: "No devices listed. Scan to discover nearby devices."
                                        visible: true
                                        draw_text.color: #x7b8d9d
                                        draw_text.text_style.font_size: 13
                                    }

                                    bluetooth_device_1 := AppButton {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    bluetooth_device_2 := AppButton {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    bluetooth_device_3 := AppButton {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    bluetooth_device_4 := AppButton {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }

                                    selected_bluetooth_device := Label {
                                        width: Fill
                                        text: "Select a device to connect."
                                        draw_text.color: #x576b7d
                                        draw_text.text_style.font_size: 13
                                    }

                                    connect_device := AppButton {
                                        width: 160
                                        height: 42
                                        enabled: false
                                        text: "Connect"
                                        draw_bg.color: #x00bfd0
                                        draw_bg.color_hover: #x20cddd
                                        draw_bg.color_down: #x44c7d5
                                        draw_bg.color_disabled: #eef3f7
                                        draw_text.color: #x182b3c
                                        draw_text.color_disabled: #x7b8d9d
                                        animator +: { disabled: { default: @on } }
                                    }

                                    system_bluetooth_settings := AppButton {
                                        width: 210
                                        height: 38
                                        text: "Bluetooth settings"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x576b7d
                                        draw_text.text_style.font_size: 12
                                    }
                                }

                                device_detail_panel := RoundedView {
                                    width: Fill
                                    height: Fit
                                    visible: false
                                    flow: Down
                                    spacing: 16
                                    padding: 22
                                    show_bg: true
                                    draw_bg +: { color: #xffffffff radius: 12.0 }

                                    device_detail_back := AppButton {
                                        width: 142
                                        height: 38
                                        text: "←  All devices"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                    }

                                    device_detail_title := Label {
                                        width: Fill
                                        text: "Device settings"
                                        draw_text.color: #x18263a
                                        draw_text.text_style.font_size: 22
                                    }

                                    device_detail_connection := Label {
                                        width: Fill
                                        text: "Select a connected device to view its settings."
                                        draw_text.color: #x66798b
                                        draw_text.text_style.font_size: 14
                                    }

                                    device_tabs := View {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 8

                                        device_tab_overview := AppButton {
                                            width: 112
                                            height: 38
                                            text: "Overview"
                                            draw_bg.color: #x9ee5ed
                                            draw_bg.color_hover: #x7bd9e3
                                            draw_bg.color_down: #x44c7d5
                                            draw_text.color: #x182b3c
                                        }

                                        device_tab_diagnostics := AppButton {
                                            width: 126
                                            height: 38
                                            text: "Diagnostics"
                                            draw_bg.color: #eef3f7
                                            draw_bg.color_hover: #dcebf0
                                            draw_bg.color_down: #c9e8ed
                                            draw_text.color: #x213246
                                        }
                                    }

                                    device_overview := View {
                                        width: Fill
                                        height: Fit
                                        flow: Down
                                        spacing: 10

                                        device_overview_details := Label {
                                            width: Fill
                                            text: "Connection details will appear here."
                                            draw_text.color: #x576b7d
                                            draw_text.text_style.font_size: 14
                                        }

                                        device_settings_hint := Label {
                                            width: Fill
                                            text: "Use the computer's Bluetooth settings to pair or remove the device."
                                            draw_text.color: #x718597
                                            draw_text.text_style.font_size: 13
                                        }
                                    }

                                    device_diagnostics := View {
                                        width: Fill
                                        height: Fit
                                        visible: false
                                        flow: Down
                                        spacing: 12

                                        diagnostics_title := Label {
                                            width: Fill
                                            text: "Live input"
                                            draw_text.color: #x18263a
                                            draw_text.text_style.font_size: 18
                                        }

                                        diagnostics_hint := Label {
                                            width: Fill
                                            text: "Device-specific Bluetooth input inspection is not available yet."
                                            draw_text.color: #x66798b
                                            draw_text.text_style.font_size: 13
                                        }

                                        diagnostics_events := Label {
                                            width: Fill
                                            height: Fit
                                            text: "Waiting for mouse, keyboard, or touch input…"
                                            draw_text.color: #x176575
                                            draw_text.text_style.font_size: 13
                                        }

                                        diagnostics_clear := AppButton {
                                            width: 126
                                            height: 36
                                            text: "Clear events"
                                            draw_bg.color: #eef3f7
                                            draw_bg.color_hover: #dcebf0
                                            draw_bg.color_down: #c9e8ed
                                            draw_text.color: #x213246
                                        }
                                    }
                                }

                                layouts_panel := RoundedView {
                                    width: Fill
                                    height: Fit
                                    visible: state.current_screen == "layouts" || state.current_screen == "layout_create"
                                    flow: Down
                                    spacing: 18
                                    padding: Inset{ top: if state.is_narrow { 0 } else { state.page_gap }, left: 22, right: 22, bottom: 22 }
                                    show_bg: true
                                    draw_bg +: { color: #xffffffff radius: 12.0 }

                                    layout_create_form := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.current_screen == "layout_create"
                                        flow: Down
                                        spacing: 10
                                        padding: 16
                                        show_bg: true
                                        draw_bg +: { color: #xf0f5f8 }

                                        layout_name_input := AppTextInput {
                                            width: Fill
                                            height: 42
                                            empty_text: "Name this layout"
                                        }

                                        layout_create_actions := View {
                                            width: Fill
                                            height: Fit
                                            flow: Right
                                            spacing: 8

                                            layout_save := AppLayoutButton {
                                                width: 104
                                                height: 38
                                                text: "Save layout"
                                                draw_bg.color: #x00bfd0
                                                draw_bg.color_hover: #x20cddd
                                                draw_bg.color_down: #x44c7d5
                                                draw_text.color: #x182b3c
                                                draw_text.text_style.font_size: 12
                                            }

                                            layout_cancel := AppLayoutButton {
                                                width: 92
                                                height: 38
                                                text: "Cancel"
                                                draw_bg.color: #eef3f7
                                                draw_bg.color_hover: #dcebf0
                                                draw_bg.color_down: #c9e8ed
                                                draw_text.color: #x576b7d
                                                draw_text.text_style.font_size: 12
                                            }
                                        }

                                        layout_save_status := Label {
                                            width: Fill
                                            text: ""
                                            draw_text.color: #x6b7c8c
                                            draw_text.text_style.font_size: 12
                                        }
                                    }

                                    layout_choices := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.current_screen == "layouts"
                                        flow: Right
                                        spacing: 10

                                        layout_keyboard_select := AppLayoutButton {
                                            width: Fill
                                            height: 56
                                            text: "  Full keyboard"
                                            icon_walk: Walk{ width: 22, height: 22 }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: crate_resource("self:resources/keyboard.svg")
                                            }
                                            draw_bg.color: #eef3f7
                                            draw_bg.color_hover: #dcebf0
                                            draw_bg.color_down: #c9e8ed
                                            draw_text.color: #x213246
                                            draw_text.text_style.font_size: 13
                                        }

                                        layout_touchpad_select := AppLayoutButton {
                                            width: Fill
                                            height: 56
                                            text: "  Touchpad"
                                            icon_walk: Walk{ width: 22, height: 22 }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: crate_resource("self:resources/touchpad.svg")
                                            }
                                            draw_bg.color: #x9ee5ed
                                            draw_bg.color_hover: #x7bd9e3
                                            draw_bg.color_down: #x44c7d5
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                    }

                                    layout_custom_1 := AppLayoutButton {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }

                                    layout_gamepad_select := AppLayoutButton {
                                        width: Fill
                                        height: 56
                                        visible: state.current_screen == "layouts"
                                        text: "  Gamepad"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }

                                    layout_custom_create := AppLayoutButton {
                                        width: Fill
                                        height: 56
                                        visible: state.current_screen == "layouts"
                                        text: "  Custom controls"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_2 := AppLayoutButton {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_3 := AppLayoutButton {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_4 := AppLayoutButton {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }

                                    layout_preview_title := Label {
                                        width: Fill
                                        visible: false
                                        text: "Touchpad"
                                        draw_text.color: #x18263a
                                        draw_text.text_style.font_size: 18
                                    }

                                    layout_touchpad_preview := View {
                                        width: Fill
                                        height: if state.is_compact { 340 } else { 460 }
                                        visible: false
                                        flow: Overlay
                                        show_bg: true
                                        draw_bg.color: #e6f7f9

                                        touchpad_mouse_icon := Label {
                                            width: Fill
                                            height: Fill
                                            text: "◎"
                                            align: Align{ x: 0.5, y: 0.5 }
                                            draw_text.color: #x00a7b8
                                            draw_text.text_style.font_size: 40
                                        }
                                    }

                                    layout_mouse_buttons := View {
                                        width: Fill
                                        height: 52
                                        visible: false
                                        flow: Right
                                        spacing: 8

                                        mouse_left_button := AppLayoutButton {
                                            width: Fill
                                            height: 52
                                            text: "Left click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                        mouse_middle_button := AppLayoutButton {
                                            width: Fill
                                            height: 52
                                            text: "Middle click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                        mouse_right_button := AppLayoutButton {
                                            width: Fill
                                            height: 52
                                            text: "Right click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                    }

                                    layout_keyboard_preview := View {
                                        width: Fill
                                        height: Fit
                                        visible: false
                                        flow: Down
                                        spacing: 6
                                        padding: 10
                                        show_bg: true
                                        draw_bg.color: #eef3f7

                                        keyboard_function_row := View {
                                            width: Fill
                                            height: 34
                                            flow: Right
                                            spacing: 4
                                            key_esc := AppLayoutButton { width: Fill height: 34 text: "Esc" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f1 := AppLayoutButton { width: Fill height: 34 text: "F1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f2 := AppLayoutButton { width: Fill height: 34 text: "F2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f3 := AppLayoutButton { width: Fill height: 34 text: "F3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f4 := AppLayoutButton { width: Fill height: 34 text: "F4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f5 := AppLayoutButton { width: Fill height: 34 text: "F5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f6 := AppLayoutButton { width: Fill height: 34 text: "F6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f7 := AppLayoutButton { width: Fill height: 34 text: "F7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f8 := AppLayoutButton { width: Fill height: 34 text: "F8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f9 := AppLayoutButton { width: Fill height: 34 text: "F9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f10 := AppLayoutButton { width: Fill height: 34 text: "F10" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f11 := AppLayoutButton { width: Fill height: 34 text: "F11" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f12 := AppLayoutButton { width: Fill height: 34 text: "F12" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_number_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_num1 := AppLayoutButton { width: Fill height: 40 text: "`" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num2 := AppLayoutButton { width: Fill height: 40 text: "1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num3 := AppLayoutButton { width: Fill height: 40 text: "2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num4 := AppLayoutButton { width: Fill height: 40 text: "3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num5 := AppLayoutButton { width: Fill height: 40 text: "4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num6 := AppLayoutButton { width: Fill height: 40 text: "5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num7 := AppLayoutButton { width: Fill height: 40 text: "6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num8 := AppLayoutButton { width: Fill height: 40 text: "7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num9 := AppLayoutButton { width: Fill height: 40 text: "8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num10 := AppLayoutButton { width: Fill height: 40 text: "9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num11 := AppLayoutButton { width: Fill height: 40 text: "0" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num12 := AppLayoutButton { width: Fill height: 40 text: "-" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num13 := AppLayoutButton { width: Fill height: 40 text: "=" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_backspace := AppLayoutButton { width: Fill height: 40 text: "⌫" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 11 }
                                        }

                                        keyboard_qwerty_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_tab := AppLayoutButton { width: Fill height: 40 text: "Tab" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_q := AppLayoutButton { width: Fill height: 40 text: "Q" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_w := AppLayoutButton { width: Fill height: 40 text: "W" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_e := AppLayoutButton { width: Fill height: 40 text: "E" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_r := AppLayoutButton { width: Fill height: 40 text: "R" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_t := AppLayoutButton { width: Fill height: 40 text: "T" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_y := AppLayoutButton { width: Fill height: 40 text: "Y" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_u := AppLayoutButton { width: Fill height: 40 text: "U" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_i := AppLayoutButton { width: Fill height: 40 text: "I" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_o := AppLayoutButton { width: Fill height: 40 text: "O" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_p := AppLayoutButton { width: Fill height: 40 text: "P" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_bracket := AppLayoutButton { width: Fill height: 40 text: "[" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_bracket2 := AppLayoutButton { width: Fill height: 40 text: "]" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_enter1 := AppLayoutButton { width: Fill height: 40 text: "Enter" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_home_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_caps := AppLayoutButton { width: Fill height: 40 text: "Caps" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_a := AppLayoutButton { width: Fill height: 40 text: "A" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_s := AppLayoutButton { width: Fill height: 40 text: "S" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_d := AppLayoutButton { width: Fill height: 40 text: "D" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_f := AppLayoutButton { width: Fill height: 40 text: "F" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_g := AppLayoutButton { width: Fill height: 40 text: "G" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_h := AppLayoutButton { width: Fill height: 40 text: "H" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_j := AppLayoutButton { width: Fill height: 40 text: "J" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_k := AppLayoutButton { width: Fill height: 40 text: "K" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_l := AppLayoutButton { width: Fill height: 40 text: "L" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_semicolon := AppLayoutButton { width: Fill height: 40 text: ";" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_quote := AppLayoutButton { width: Fill height: 40 text: "'" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_enter2 := AppLayoutButton { width: Fill height: 40 text: "↵" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 12 }
                                        }

                                        keyboard_bottom_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_shift := AppLayoutButton { width: Fill height: 40 text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_z := AppLayoutButton { width: Fill height: 40 text: "Z" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_x := AppLayoutButton { width: Fill height: 40 text: "X" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_c := AppLayoutButton { width: Fill height: 40 text: "C" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_v := AppLayoutButton { width: Fill height: 40 text: "V" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_b := AppLayoutButton { width: Fill height: 40 text: "B" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_n := AppLayoutButton { width: Fill height: 40 text: "N" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_m := AppLayoutButton { width: Fill height: 40 text: "M" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_comma := AppLayoutButton { width: Fill height: 40 text: "," draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_period := AppLayoutButton { width: Fill height: 40 text: "." draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_slash := AppLayoutButton { width: Fill height: 40 text: "/" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_shift2 := AppLayoutButton { width: Fill height: 40 text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_modifier_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_ctrl := AppLayoutButton { width: Fill height: 40 text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_fn := AppLayoutButton { width: Fill height: 40 text: "Fn" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_alt := AppLayoutButton { width: Fill height: 40 text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_space := AppLayoutButton { width: Fill height: 40 text: "Space" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_alt2 := AppLayoutButton { width: Fill height: 40 text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_menu := AppLayoutButton { width: Fill height: 40 text: "Menu" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_ctrl2 := AppLayoutButton { width: Fill height: 40 text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_up := AppLayoutButton { width: Fill height: 40 text: "↑" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 13 }
                                            key_down := AppLayoutButton { width: Fill height: 40 text: "↓" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 13 }
                                        }
                                    }
                                }

                                layout_surface_panel := View {
                                    width: Fill
                                    height: if state.current_screen == "layout_surface" { Fill } else { Fit }
                                    visible: state.current_screen == "layout_surface" || state.custom_keyboard_picker
                                    flow: Down
                                    spacing: if state.is_compact { 4 } else { 8 }

                                    touchpad_surface := RoundedView {
                                        width: Fill
                                        height: Fill
                                        visible: state.selected_layout_kind == "touchpad"
                                        flow: Right
                                        spacing: 6
                                        padding: if state.is_compact { 4 } else { 8 }
                                        show_bg: true
                                        draw_bg +: {
                                            color: #xe6f7f9
                                            radius: if state.is_compact { 8.0 } else { 14.0 }
                                        }

                                        touchpad_center := View {
                                            width: Fill
                                            height: Fill
                                            align: Align{ x: 0.5, y: 0.5 }

                                            touchpad_mouse_icon := Icon {
                                                width: 32
                                                height: 32
                                                draw_icon +: {
                                                    color: #x00a7b8
                                                    svg: crate_resource("self:resources/mouse.svg")
                                                }
                                            }
                                        }

                                        touchpad_scroll_rail := View {
                                            width: 18
                                            height: Fill
                                            flow: Down
                                            spacing: 4

                                            scroll_up := Label {
                                                width: Fill
                                                height: 24
                                                text: "^"
                                                align: Align{ x: 0.5, y: 0.5 }
                                                draw_text.color: #x00a7b8
                                                draw_text.text_style.font_size: 16
                                            }
                                            scroll_track := View {
                                                width: 2
                                                height: Fill
                                                margin: Inset{ left: 8, right: 8 }
                                                show_bg: true
                                                draw_bg +: { color: #x7cd5df }
                                            }
                                            scroll_down := Label {
                                                width: Fill
                                                height: 24
                                                text: "v"
                                                align: Align{ x: 0.5, y: 0.5 }
                                                draw_text.color: #x00a7b8
                                                draw_text.text_style.font_size: 16
                                            }
                                        }
                                    }

                                    layout_surface_mouse_buttons := View {
                                        width: Fill
                                        height: if state.is_compact { 36 } else { 52 }
                                        visible: state.selected_layout_kind == "touchpad"
                                        flow: Right
                                        spacing: if state.is_compact { 4 } else { 8 }

                                        surface_left_click := AppLayoutButton {
                                            width: Fill
                                            height: state.surface_button_height
                                            text: "Left click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: if state.is_compact { 10 } else { 13 }
                                        }
                                        surface_middle_click := AppLayoutButton {
                                            width: Fill
                                            height: state.surface_button_height
                                            text: "Middle click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: if state.is_compact { 10 } else { 13 }
                                        }
                                        surface_right_click := AppLayoutButton {
                                            width: Fill
                                            height: state.surface_button_height
                                            text: "Right click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: if state.is_compact { 10 } else { 13 }
                                        }
                                    }

                                    layout_surface_gamepad := View {
                                        width: Fill
                                        height: Fill
                                        visible: state.selected_layout_kind == "gamepad"
                                        flow: Down
                                        spacing: 16
                                        padding: 20
                                        show_bg: true
                                        draw_bg +: { color: #xf0f5f8 }

                                        gamepad_shoulders := View {
                                            width: Fill height: 52 flow: Right spacing: 10
                                            gamepad_lb := AppLayoutButton { width: Fill height: 52 text: "LB" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                            gamepad_lt := AppLayoutButton { width: Fill height: 52 text: "LT" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                            gamepad_rb := AppLayoutButton { width: Fill height: 52 text: "RB" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                            gamepad_rt := AppLayoutButton { width: Fill height: 52 text: "RT" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                        }

                                        gamepad_controls := View {
                                            width: Fill height: Fill flow: Right spacing: 10 align: Align{ y: 0.5 }
                                            gamepad_dpad := View {
                                                width: Fill height: 200 flow: Down spacing: 6 align: Align{ x: 0.5, y: 0.5 }
                                                gamepad_dpad_up := AppLayoutButton { width: 66 height: 56 text: "↑" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                                gamepad_dpad_horizontal := View {
                                                    width: 204 height: 56 flow: Right spacing: 6
                                                    gamepad_dpad_left := AppLayoutButton { width: 66 height: 56 text: "←" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                                    View { width: Fill height: Fill }
                                                    gamepad_dpad_right := AppLayoutButton { width: 66 height: 56 text: "→" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                                }
                                                gamepad_dpad_down := AppLayoutButton { width: 66 height: 56 text: "↓" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                            }
                                            gamepad_center_controls := View {
                                                width: Fill height: 200 flow: Down spacing: 10 align: Align{ y: 0.5 }
                                                gamepad_menu_row := View {
                                                    width: Fill height: 48 flow: Right spacing: 8
                                                    gamepad_select := AppLayoutButton { width: Fill height: 48 text: "Select" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                                    gamepad_home := AppLayoutButton { width: Fill height: 48 text: "Home" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                                    gamepad_start := AppLayoutButton { width: Fill height: 48 text: "Start" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                                }
                                                gamepad_face_row := View {
                                                    width: Fill height: Fill flow: Down spacing: 4
                                                    gamepad_face_top := View {
                                                        width: Fill height: Fill flow: Right
                                                        View { width: Fill height: Fill }
                                                        gamepad_y := AppLayoutButton { width: 48 height: Fill text: "Y" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: Fill height: Fill }
                                                    }
                                                    gamepad_face_middle := View {
                                                        width: Fill height: Fill flow: Right
                                                        View { width: Fill height: Fill }
                                                        gamepad_x := AppLayoutButton { width: 48 height: Fill text: "X" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: 48 height: Fill }
                                                        gamepad_b := AppLayoutButton { width: 48 height: Fill text: "B" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: Fill height: Fill }
                                                    }
                                                    gamepad_face_bottom := View {
                                                        width: Fill height: Fill flow: Right
                                                        View { width: Fill height: Fill }
                                                        gamepad_a := AppLayoutButton { width: 48 height: Fill text: "A" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: Fill height: Fill }
                                                    }
                                                }
                                            }
                                        }

                                        gamepad_status := Label { width: Fill height: 24 text: "" draw_text.color: #x6b7c8c draw_text.text_style.font_size: 12 }
                                    }

                                    layout_surface_custom := View {
                                        width: Fill height: Fill visible: state.selected_layout_kind == "custom" flow: Down spacing: 12
                                        padding: 16 show_bg: true draw_bg +: { color: #xf5f8fa }
                                        custom_surface_hint := Label { width: Fill height: Fit visible: !state.custom_edit_mode text: "YOUR CONTROLS   ·   Hold a button to send input" draw_text.color: #x27818c draw_text.text_style.font_size: 11 }
                                        custom_surface_add_menu := RoundedView {
                                            width: Fill height: Fit visible: state.custom_edit_mode && state.custom_add_menu_open
                                            flow: Right spacing: 8 padding: 8
                                            draw_bg +: { color: #xe5e9ec border_radius: 8 }
                                            custom_add_button := AppLayoutButton { width: Fill height: 40 text: "Button" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                            custom_add_touchpad := AppLayoutButton { width: Fill height: 40 text: "Touchpad" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                            custom_add_speech := AppLayoutButton { width: Fill height: 40 text: "Speech input" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                            custom_add_keyboard_toggle := AppLayoutButton { width: Fill height: 40 text: "Keyboard toggle" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                        }
                                        custom_surface_edit_notice := RoundedView {
                                            width: Fill height: 40 visible: state.custom_edit_mode
                                            flow: Right spacing: 10 align: Align{y: 0.5}
                                            padding: 10 draw_bg +: { color: #xe5e9ec border_radius: 8 }
                                            custom_edit_notice_icon := AppLayoutButton { width: 28 height: 28 text: "i" draw_bg.color: #x74808a draw_bg.border_radius: 14 draw_text.color: #xffffff draw_text.text_style.font_size: 14 }
                                            custom_edit_notice_text := Label { width: Fill height: Fit text: "Press the button to edit it. Long press a button to drag and move it." draw_text.color: #x48535d draw_text.text_style.font_size: 11 }
                                        }
                                        custom_surface_edit_status := Label { width: Fill height: Fit visible: state.custom_edit_mode text: "" draw_text.color: #x6b7c8c draw_text.text_style.font_size: 11 }
                                        custom_surface_row_1 := View {
                                            width: Fill height: if state.custom_edit_mode { 64 } else { Fill } flow: Right spacing: 12
                                            custom_surface_1 := AppLayoutButton { width: Fill height: if state.custom_edit_mode { 56 } else { Fill } text: "" draw_bg.color: #e8f7f8 draw_bg.color_hover: #caeff2 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                            custom_surface_2 := AppLayoutButton { width: Fill height: if state.custom_edit_mode { 56 } else { Fill } text: "" draw_bg.color: #eef4f7 draw_bg.color_hover: #d6eef1 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                            custom_surface_3 := AppLayoutButton { width: Fill height: if state.custom_edit_mode { 56 } else { Fill } text: "" draw_bg.color: #e8f7f8 draw_bg.color_hover: #caeff2 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                        }
                                        custom_surface_row_2 := View {
                                            width: Fill height: if state.custom_edit_mode { 64 } else { Fill } flow: Right spacing: 12
                                            custom_surface_4 := AppLayoutButton { width: Fill height: if state.custom_edit_mode { 56 } else { Fill } text: "" draw_bg.color: #eef4f7 draw_bg.color_hover: #d6eef1 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                            custom_surface_5 := AppLayoutButton { width: Fill height: if state.custom_edit_mode { 56 } else { Fill } text: "" draw_bg.color: #e8f7f8 draw_bg.color_hover: #caeff2 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                            custom_surface_6 := AppLayoutButton { width: Fill height: if state.custom_edit_mode { 56 } else { Fill } text: "" draw_bg.color: #eef4f7 draw_bg.color_hover: #d6eef1 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                        }
                                    }

                                    layout_surface_keyboard := View {
                                        width: Fill
                                        height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { Fit }
                                        visible: state.selected_layout_kind == "keyboard" || state.custom_keyboard_picker
                                        flow: Down
                                        spacing: if state.is_narrow { 3 } else { 6 }
                                        padding: if state.is_narrow { 5 } else { 10 }
                                        show_bg: true
                                        draw_bg +: { color: #xf0f5f8 }

                                        landscape_keyboard_toolbar := View {
                                            width: Fill
                                            height: 42
                                            visible: false
                                            flow: Right
                                            spacing: 8
                                            align: Align{ y: 0.5 }

                                            landscape_menu_toggle := AppIconButton {
                                                width: 42
                                                height: 42
                                                text: "☰"
                                                draw_bg.color: #eef3f7
                                                draw_bg.color_hover: #dcebf0
                                                draw_text.color: #x213246
                                            }

                                            landscape_keyboard_title := Label {
                                                width: Fit
                                                text: "Mouse / Keyboard"
                                                draw_text.color: #x18263a
                                                draw_text.text_style.font_size: 22
                                            }

                                            landscape_toolbar_spacer := View { width: Fill height: 1 }

                                            landscape_keyboard_toggle := AppIconButton {
                                                width: 42
                                                height: 42
                                                text: ""
                                                icon_walk: Walk{ width: 20, height: 20 }
                                                draw_icon +: {
                                                    color: #x00a7b8
                                                    svg: crate_resource("self:resources/keyboard.svg")
                                                }
                                                draw_bg.color: #eef3f7
                                            }

                                            landscape_settings_button := AppIconButton {
                                                width: 42
                                                height: 42
                                                text: ""
                                                icon_walk: Walk{ width: 20, height: 20 }
                                                draw_icon +: {
                                                    color: #x00a7b8
                                                    svg: crate_resource("self:resources/settings.svg")
                                                }
                                                draw_bg.color: #eef3f7
                                            }
                                        }

                                        surface_keyboard_function := View {
                                            width: Fill
                                            height: if state.is_narrow && !state.custom_keyboard_picker { 0 } else { 34 }
                                            visible: !state.is_narrow || state.custom_keyboard_picker
                                            flow: Right
                                            spacing: if state.is_narrow { 2 } else { 4 }
                                            sk_esc := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "Esc" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f1 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f2 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f3 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f4 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f5 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f6 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f7 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f8 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f9 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f10 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F10" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f11 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F11" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f12 := AppLayoutButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F12" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_number := View {
                                            width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sn_1 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "`" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_2 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_3 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_4 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_5 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_6 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_7 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_8 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_9 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_10 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_11 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "0" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_backspace := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "⌫" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 11 }
                                        }

                                        surface_keyboard_symbols := View {
                                            width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            ss_minus := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "-" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_equal := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "=" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_open_bracket := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "[" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_close_bracket := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "]" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_backslash := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "\\" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_semicolon := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: ";" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_quote := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "'" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_comma := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "," draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_period := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "." draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_slash := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "/" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                        }

                                        surface_keyboard_top := View {
                                            width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            st_tab := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Tab" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            st_q := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Q" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_w := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "W" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_e := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "E" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_r := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "R" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_t := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "T" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_y := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Y" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_u := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "U" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_i := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "I" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_o := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "O" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_p := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "P" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_enter := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Enter" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_home := View {
                                            width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sh_caps := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Caps" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sh_a := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "A" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_s := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "S" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_d := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "D" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_f := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "F" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_g := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "G" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_h := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "H" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_j := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "J" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_k := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "K" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_l := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "L" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_enter := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Ent" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 12 }
                                        }

                                        surface_keyboard_bottom := View {
                                            width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sb_shift := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sb_z := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Z" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_x := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "X" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_c := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "C" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_v := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "V" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_b := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "B" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_n := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "N" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_m := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "M" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_shift2 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_modifiers := View {
                                            width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sm_ctrl := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_fn := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Fn" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_alt := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_space := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Space" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sm_alt2 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_ctrl2 := AppLayoutButton { width: Fill height: if state.is_narrow && !state.custom_keyboard_picker { Fill } else { 40 } text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }
                                    }
                                }

                                layout_settings_panel := RoundedView {
                                    width: Fill
                                    height: Fit
                                    visible: state.current_screen == "layout_settings"
                                    flow: Down
                                    spacing: 8
                                    padding: 12
                                    show_bg: true
                                    draw_bg +: { color: #xffffffff radius: 12.0 }

                                    layout_settings_name := Label {
                                        width: Fill
                                        text: if state.selected_layout_kind == "keyboard" { "Full keyboard settings" } else if state.selected_layout_kind == "gamepad" { "Gamepad settings" } else if state.selected_layout_kind == "custom" { "Custom controls settings" } else { "Touchpad settings" }
                                        draw_text.color: #x18263a
                                        draw_text.text_style.font_size: 16
                                    }

                                    layout_touchpad_settings := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.selected_layout_kind == "touchpad"
                                        flow: Down
                                        spacing: 12

                                        pointer_speed_label := Label { width: Fill text: "Pointer speed" draw_text.color: #x576b7d draw_text.text_style.font_size: 14 }
                                        pointer_speed_choices := View {
                                            width: Fill height: Fit flow: Right spacing: 8
                                            pointer_slow := AppButton { width: Fill height: 42 text: "Slow" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_text.color: #x213246 }
                                            pointer_standard := AppButton { width: Fill height: 42 text: "Standard" draw_bg.color: #x9ee5ed draw_bg.color_hover: #x7bd9e3 draw_text.color: #x182b3c }
                                            pointer_fast := AppButton { width: Fill height: 42 text: "Fast" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_text.color: #x213246 }
                                        }
                                        touchpad_tap_toggle := AppButton { width: Fill height: 42 text: "Tap to click   ·   On" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_text.color: #x213246 }
                                        touchpad_scroll_toggle := AppButton { width: Fill height: 42 text: "Natural scrolling   ·   Off" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_text.color: #x213246 }
                                    }

                                    layout_keyboard_settings := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.selected_layout_kind == "keyboard"
                                        flow: Down
                                        spacing: 12

                                        keyboard_layout_label := Label { width: Fill text: "Keyboard layout" draw_text.color: #x576b7d draw_text.text_style.font_size: 14 }
                                        keyboard_qwerty_choice := AppButton { width: Fill height: 42 text: "QWERTY   ·   English (US)" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_text.color: #x213246 }
                                        keyboard_repeat_toggle := AppButton { width: Fill height: 42 text: "Key repeat   ·   On" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_text.color: #x213246 }
                                    }

                                    layout_custom_settings := View {
                                        width: Fill height: Fit visible: state.selected_layout_kind == "custom" flow: Down spacing: 6
                                        custom_editor_intro := Label { width: Fill text: "Choose a control, then set its action and optional hardware button." draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                        custom_editor_columns := View {
                                            width: Fill height: Fit flow: if state.is_narrow { Down } else { Right } spacing: 14
                                            custom_editor_list := View {
                                                width: Fill height: Fit flow: Down spacing: 6
                                                custom_editor_row_1 := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    custom_edit_1 := AppButton { width: Fill height: 32 text: "" draw_bg.color: #e5f5f7 draw_text.color: #x182b3c }
                                                    custom_edit_2 := AppButton { width: Fill height: 32 text: "" draw_bg.color: #e5f5f7 draw_text.color: #x182b3c }
                                                    custom_edit_3 := AppButton { width: Fill height: 32 text: "" draw_bg.color: #e5f5f7 draw_text.color: #x182b3c }
                                                }
                                                custom_editor_row_2 := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    custom_edit_4 := AppButton { width: Fill height: 32 text: "" draw_bg.color: #e5f5f7 draw_text.color: #x182b3c }
                                                    custom_edit_5 := AppButton { width: Fill height: 32 text: "" draw_bg.color: #e5f5f7 draw_text.color: #x182b3c }
                                                    custom_edit_6 := AppButton { width: Fill height: 32 text: "" draw_bg.color: #e5f5f7 draw_text.color: #x182b3c }
                                                }
                                                custom_editor_manage := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    visible: !state.custom_edit_mode
                                                    custom_add := AppButton { width: Fill height: 32 text: "+ Add button" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                    custom_delete := AppButton { width: Fill height: 32 text: "Remove" draw_bg.color: #fce8e8 draw_text.color: #x182b3c }
                                                }
                                                custom_editor_order := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    visible: !state.custom_edit_mode
                                                    custom_move_left := AppButton { width: Fill height: 32 text: "Move left" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                    custom_move_right := AppButton { width: Fill height: 32 text: "Move right" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                }
                                            }
                                            custom_editor_details := View {
                                                width: Fill height: Fit flow: Down spacing: 6
                                                custom_function_panel := View {
                                                    width: Fill height: Fit visible: false flow: Down spacing: 6
                                                    custom_function_search := AppTextInput { width: Fill height: 40 empty_text: "Search function" }
                                                    custom_function_hint := Label { width: Fill height: Fit text: "Choose a supported OpenWASD action for this button." draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                                    custom_function_results := View {
                                                        width: Fill height: Fit flow: Down spacing: 4
                                                        custom_function_1 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_2 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_3 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_4 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_5 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_6 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_7 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_8 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_9 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_10 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_11 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_12 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_13 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_14 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_15 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_16 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                        custom_function_17 := AppLayoutButton { width: Fill height: 40 text: "" visible: false }
                                                    }
                                                }
                                                custom_text_panel := View {
                                                    width: Fill height: Fit visible: false flow: Down spacing: 6
                                                    custom_text_input := AppTextInput { width: Fill height: 44 empty_text: "Text to send" }
                                                    custom_text_hint := Label { width: Fill height: Fit text: "ASCII text is sent as US keyboard keystrokes." draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                                    custom_text_save := AppButton { width: Fill height: 40 text: "Set text" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                }
                                                custom_hardware_panel := View {
                                                    width: Fill height: Fit visible: false flow: Down spacing: 6
                                                    custom_hardware_hint := Label { width: Fill height: Fit text: "Listen for a physical keyboard or controller button on this device." draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                                    custom_editor_hardware := View {
                                                        width: Fill height: 40 flow: Right spacing: 6
                                                        custom_learn := AppButton { width: Fill height: 40 text: "Assign hardware button" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                        custom_unbind := AppButton { width: 70 height: 40 text: "Clear" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                    }
                                                }
                                                custom_editor_action := View {
                                                    width: Fill height: 40 flow: Right spacing: 6
                                                    custom_action_label := Label { width: Fill height: 40 text: "Output" align: Align{ x: 0.5, y: 0.5 } draw_text.color: #x164e5a draw_text.text_style.font_size: 14 }
                                                    custom_action_test := AppButton { width: 72 height: 40 text: "Test" draw_bg.color: #eef3f7 draw_text.color: #x182b3c }
                                                }
                                                custom_settings_panel := View {
                                                    width: Fill height: Fit visible: false flow: Down spacing: 6
                                                    custom_label_title := Label { width: Fill text: "Button name" draw_text.color: #x46535f draw_text.text_style.font_size: 12 }
                                                    custom_label_row := View {
                                                        width: Fill height: 44 flow: Right spacing: 6
                                                        custom_label_input := AppTextInput { width: Fill height: 44 empty_text: "Button name" }
                                                        custom_label_save := AppButton { width: 90 height: 44 text: "Set name" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                    }
                                                    custom_show_text_toggle := AppButton { width: Fill height: 40 text: "Show text" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                    custom_show_icon_toggle := AppButton { width: Fill height: 40 text: "Show icon" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                    custom_icon_source_label := Label { width: Fill text: "Icon source" draw_text.color: #x46535f draw_text.text_style.font_size: 12 }
                                                    custom_icon_default := AppButton { width: Fill height: 40 text: "Use default icon" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                    custom_icon_label := Label { width: Fill text: "Custom icon: emoji or symbol" draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                                    custom_icon_row := View {
                                                        width: Fill height: 44 flow: Right spacing: 6
                                                        custom_icon_input := AppTextInput { width: Fill height: 44 empty_text: "Enter icon" }
                                                        custom_icon_set := AppButton { width: 90 height: 44 text: "Set icon" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                    }
                                                }
                                                custom_editor_status := Label { width: Fill height: Fit text: "Changes are saved on this device." draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                            }
                                        }
                                    }
                                }
                                }
                            }

                            mobile_backdrop := AppButton {
                                width: Fill
                                height: Fill
                                visible: !state.sidebar_collapsed
                                text: ""
                                draw_bg.border_radius: 0.0
                                draw_bg.color: #0009
                            }

                            sidebar := SolidView {
                                width: if state.is_narrow { 304 } else { 320 }
                                height: Fill
                                visible: !state.sidebar_collapsed
                                flow: Down
                                spacing: 0
                                padding: 0
                                show_bg: true
                                draw_bg +: { color: #xffffffff }

                                sidebar_header := SolidView {
                                    width: Fill
                                    height: 76
                                    flow: Right
                                    spacing: 8
                                    padding: Inset{ left: 16 right: 8 }
                                    align: Align{ y: 0.5 }
                                    show_bg: true
                                    draw_bg +: { color: #x164e5a }

                                    sidebar_brand := View {
                                        width: Fill
                                        height: Fit
                                        flow: Down
                                        spacing: 2
                                        app_brand := Label {
                                            width: Fill
                                            text: "OpenWASD"
                                            draw_text.color: #xffffffff
                                            draw_text.text_style.font_size: 19
                                        }
                                        sidebar_tagline := Label {
                                            width: Fill
                                            text: "Input workspace"
                                            draw_text.color: #xd2eef0
                                            draw_text.text_style.font_size: 11
                                        }
                                    }

                                    sidebar_toggle := AppIconButton {
                                        width: 38
                                        height: 38
                                        text: "×"
                                        draw_bg.color: #x2b6872
                                        draw_bg.color_hover: #x3c7881
                                        draw_bg.color_down: #x0b3d47
                                        draw_text.color: #xffffffff
                                        draw_text.text_style.font_size: 20
                                    }
                                }

                                sidebar_body := ScrollYView {
                                    width: Fill
                                    height: Fill
                                    scroll_bars: ScrollBars {
                                        show_scroll_x: false
                                        show_scroll_y: true
                                        scroll_bar_y.drag_scrolling: true
                                    }
                                    flow: Down
                                    spacing: 0
                                    padding: Inset{ top: 8, bottom: 8, left: 10, right: 10 }

                                    sidebar_workspace_label := Label {
                                        width: Fill
                                        height: 34
                                        padding: Inset{ left: 18 }
                                        align: Align{ y: 0.5 }
                                        text: "General"
                                        draw_text.color: #x718597
                                        draw_text.text_style.font_size: 11
                                    }

                                    nav_dashboard := AppButton {
                                        width: Fill
                                        height: 48
                                        align: Align{ x: 0.0, y: 0.5 }
                                        padding: Inset{ left: 16, right: 12 }
                                        icon_walk: Walk{ width: 19, height: 19 }
                                        draw_icon +: { color: #x00a7b8 svg: crate_resource("self:resources/home.svg") }
                                        text: "Dashboard"
                                        draw_bg.color: if state.current_screen == "dashboard" { #xe3f6f7 } else { #x0000 }
                                        draw_bg.color_hover: #xeaf4f6
                                        draw_bg.color_down: #xd6eef0
                                        draw_text.color: #x182b3c
                                        draw_text.text_style.font_size: 14
                                    }

                                    nav_devices := AppButton {
                                        width: Fill
                                        height: 48
                                        align: Align{ x: 0.0, y: 0.5 }
                                        padding: Inset{ left: 16, right: 12 }
                                        icon_walk: Walk{ width: 19, height: 19 }
                                        draw_icon +: { color: #x00a7b8 svg: crate_resource("self:resources/usb.svg") }
                                        text: "Devices"
                                        draw_bg.color: if state.current_screen == "devices" { #xe3f6f7 } else { #x0000 }
                                        draw_bg.color_hover: #xeaf4f6
                                        draw_bg.color_down: #xd6eef0
                                        draw_text.color: #x182b3c
                                        draw_text.text_style.font_size: 14
                                    }

                                    sidebar_divider := SolidView {
                                        width: Fill
                                        height: 1
                                        margin: Inset{ top: 8, bottom: 8, left: 16, right: 16 }
                                        show_bg: true
                                        draw_bg +: { color: #xe3eaee }
                                    }

                                    sidebar_controls_label := Label {
                                        width: Fill
                                        height: 34
                                        padding: Inset{ left: 18 }
                                        align: Align{ y: 0.5 }
                                        text: "Customize"
                                        draw_text.color: #x718597
                                        draw_text.text_style.font_size: 11
                                    }

                                    nav_layouts := AppButton {
                                        width: Fill
                                        height: 48
                                        align: Align{ x: 0.0, y: 0.5 }
                                        padding: Inset{ left: 16, right: 12 }
                                        icon_walk: Walk{ width: 19, height: 19 }
                                        draw_icon +: { color: #x00a7b8 svg: crate_resource("self:resources/touchpad.svg") }
                                        text: "Layouts"
                                        draw_bg.color: if state.current_screen == "layouts" { #xe3f6f7 } else { #x0000 }
                                        draw_bg.color_hover: #xeaf4f6
                                        draw_bg.color_down: #xd6eef0
                                        draw_text.color: #x182b3c
                                        draw_text.text_style.font_size: 14
                                    }
                                }

                                sidebar_footer := Label {
                                    width: Fill
                                    height: 52
                                    padding: Inset{ left: 18 }
                                    align: Align{ y: 0.5 }
                                    text: "Local-first input tools  •  v0.1"
                                    draw_text.color: #x718597
                                    draw_text.text_style.font_size: 11
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    startup_screenshot: Option<PathBuf>,
    #[rust]
    startup_screenshot_timer: Option<Timer>,
    #[rust]
    bluetooth_runtime: Option<tokio::runtime::Runtime>,
    #[rust]
    bluetooth: Option<BluetoothService>,
    #[rust]
    bluetooth_devices: Vec<BluetoothDeviceInfo>,
    #[rust]
    bluetooth_selected_id: Option<String>,
    #[rust]
    bluetooth_timer: Option<Timer>,
    #[rust]
    bluetooth_refresh_timer: Option<Timer>,
    #[rust]
    layout_refresh_timer: Option<Timer>,
    #[rust]
    custom_drag_suppress_timer: Option<Timer>,
    #[cfg(target_os = "linux")]
    #[rust]
    usb_host: Option<openwasd_usb::bridge::Host>,
    #[cfg(target_os = "linux")]
    #[rust]
    usb_companions: Vec<openwasd_usb::UsbDevice>,
    #[rust]
    selected_usb_id: Option<String>,
    #[cfg(target_os = "android")]
    #[rust]
    usb_client: Option<openwasd_usb::bridge::Client>,
    #[rust]
    hid_start_pending: bool,
    #[rust]
    hid_start_retries: u8,
    #[rust]
    hid_ready: bool,
    #[rust]
    hid_host: HidHostState,
    #[rust]
    hid_host_state_seen: bool,
    #[rust]
    device_detail_open: bool,
    #[rust]
    device_diagnostics_open: bool,
    #[rust]
    diagnostic_events: Vec<String>,
    #[rust]
    saved_layouts: Vec<SavedLayout>,
    #[rust]
    selected_layout_kind: LayoutKind,
    #[rust]
    selected_custom_layout: Option<usize>,
    #[rust]
    create_layout_open: bool,
    #[rust]
    layout_status: String,
    #[rust]
    custom_selected_control: usize,
    #[rust]
    custom_edit_mode: bool,
    #[rust]
    custom_button_focused: bool,
    #[rust]
    custom_editor_tab: u8,
    #[rust]
    custom_function_filter: String,
    #[rust]
    custom_add_menu_open: bool,
    #[rust]
    custom_drag_candidate: Option<(u64, usize, (f64, f64), f64)>,
    #[rust]
    custom_drag_active: bool,
    #[rust]
    custom_drag_suppress_click: bool,
    #[rust]
    custom_learning: bool,
    #[rust]
    custom_status: String,
    #[rust]
    active_controls: ActiveControls,
    #[rust]
    pointer_speed: u8,
    #[rust]
    tap_to_click: bool,
    #[rust]
    natural_scrolling: bool,
    #[rust]
    keyboard_repeat: bool,
    #[rust]
    keyboard_layout_index: u8,
    #[rust]
    keyboard_modifiers: u8,
    #[rust]
    touchpad_last: Option<(f64, f64)>,
    #[rust]
    touchpad_touch_uid: Option<u64>,
    #[rust]
    touchpad_start_point: Option<(f64, f64)>,
    #[rust]
    touchpad_started_at: Option<f64>,
    #[rust]
    layout_surface_active: bool,
}

impl ScriptHook for App {
    fn on_after_new(&mut self, _vm: &mut ScriptVm) {
        self.startup_screenshot =
            std::env::var_os("OPENWASD_SCREENSHOT_ON_STARTUP").map(PathBuf::from);
        self.startup_screenshot_timer = None;
        self.bluetooth_devices = Vec::new();
        self.bluetooth_selected_id = None;
        self.bluetooth_timer = None;
        self.bluetooth_refresh_timer = None;
        self.layout_refresh_timer = None;
        self.custom_drag_suppress_timer = None;
        self.selected_usb_id = None;
        #[cfg(target_os = "linux")]
        {
            self.usb_host = Some(Default::default());
            self.usb_companions.clear();
        }
        #[cfg(target_os = "android")]
        {
            self.usb_client = Some(openwasd_usb::bridge::Client::start());
        }
        self.hid_start_pending = false;
        self.hid_start_retries = 0;
        self.hid_ready = false;
        self.hid_host = HidHostState::default();
        self.hid_host_state_seen = false;
        self.device_detail_open = false;
        self.device_diagnostics_open = false;
        self.diagnostic_events = Vec::new();
        self.saved_layouts = load_saved_layouts();
        self.selected_layout_kind = match std::env::var("OPENWASD_START_LAYOUT").as_deref() {
            Ok("keyboard") => LayoutKind::Keyboard,
            Ok("gamepad") => LayoutKind::Gamepad,
            _ => LayoutKind::Touchpad,
        };
        self.selected_custom_layout = None;
        self.create_layout_open = false;
        self.layout_status.clear();
        self.custom_selected_control = 0;
        self.custom_edit_mode = false;
        self.custom_button_focused = false;
        self.custom_editor_tab = 0;
        self.custom_function_filter.clear();
        self.custom_editor_tab = 0;
        self.custom_function_filter = String::new();
        self.custom_add_menu_open = false;
        self.custom_drag_candidate = None;
        self.custom_drag_active = false;
        self.custom_drag_suppress_click = false;
        self.custom_learning = false;
        self.custom_status = "Changes are saved on this device.".into();
        self.active_controls = ActiveControls::default();
        self.pointer_speed = 1;
        self.tap_to_click = true;
        self.natural_scrolling = false;
        self.keyboard_repeat = true;
        self.keyboard_layout_index = 0;
        self.keyboard_modifiers = 0;
        self.touchpad_last = None;
        self.touchpad_touch_uid = None;
        self.touchpad_start_point = None;
        self.touchpad_started_at = None;
        self.layout_surface_active = matches!(
            std::env::var("OPENWASD_START_SCREEN").as_deref(),
            Ok("layout_surface")
        );
        log!("[OpenWASD][Bluetooth] starting Tokio runtime and BtleplugAdapter");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Bluetooth runtime should initialize");
        let bluetooth = {
            let _guard = runtime.enter();
            BluetoothService::spawn(BtleplugAdapter::new())
        };
        self.bluetooth_runtime = Some(runtime);
        self.bluetooth = Some(bluetooth);
    }
}

impl MatchEvent for App {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        #[cfg(target_os = "linux")]
        for (index, id) in [
            live_id!(usb_inspect_1),
            live_id!(usb_inspect_2),
            live_id!(usb_inspect_3),
            live_id!(usb_inspect_4),
        ]
        .into_iter()
        .enumerate()
        {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if let Some(device) = self.usb_companions.get(index) {
                    self.selected_usb_id = Some(device.id.clone());
                    self.bluetooth_selected_id = None;
                    self.device_detail_open = true;
                    self.device_diagnostics_open = true;
                    self.ui
                        .widget(cx, ids!(device_panel))
                        .set_visible(cx, false);
                    self.ui
                        .widget(cx, ids!(device_detail_panel))
                        .set_visible(cx, true);
                    self.ui
                        .view(cx, ids!(scroll_body))
                        .set_scroll_pos(cx, dvec2(0.0, 0.0));
                    self.render_device_detail(cx);
                }
            }
        }
        #[cfg(target_os = "linux")]
        for (index, id) in [
            live_id!(usb_companion_1),
            live_id!(usb_companion_2),
            live_id!(usb_companion_3),
            live_id!(usb_companion_4),
        ]
        .into_iter()
        .enumerate()
        {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if let Some(device) = self.usb_companions.get(index) {
                    if let Some(host) = self.usb_host.as_mut() {
                        match host.status(&device.id) {
                            Some(status)
                                if status.starts_with("USB connection failed:")
                                    || status == "Disconnected" =>
                            {
                                host.disconnect(&device.id);
                                host.connect(&device.id, &device.name);
                            }
                            Some(_) => host.disconnect(&device.id),
                            None => host.connect(&device.id, &device.name),
                        }
                    }
                }
                self.render_usb_devices(cx);
            }
        }
        if self.ui.button(cx, ids!(bluetooth_connect)).clicked(actions)
            || self.ui.button(cx, ids!(refresh_devices)).clicked(actions)
        {
            self.start_bluetooth_scan(cx);
        }

        if self.ui.button(cx, ids!(layout_new)).clicked(actions) {
            self.create_layout_open = true;
            self.layout_status.clear();
            self.ui
                .text_input(cx, ids!(layout_name_input))
                .set_text(cx, "");
            script_eval!(cx, {
                mod.state.current_screen = "layout_create";
                mod.state.sidebar_collapsed = true;
                ui.main_view.render()
            });
            self.render_layouts(cx);
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
        }

        if self
            .ui
            .button(cx, ids!(layout_keyboard_select))
            .clicked(actions)
        {
            self.select_layout(LayoutKind::Keyboard, None, "Full keyboard", cx);
        }
        if self
            .ui
            .button(cx, ids!(layout_touchpad_select))
            .clicked(actions)
        {
            self.select_layout(LayoutKind::Touchpad, None, "Touchpad", cx);
        }
        if self
            .ui
            .button(cx, ids!(layout_gamepad_select))
            .clicked(actions)
        {
            self.select_layout(LayoutKind::Gamepad, None, "Gamepad", cx);
        }
        if self
            .ui
            .button(cx, ids!(layout_custom_create))
            .clicked(actions)
        {
            self.create_layout_open = true;
            self.layout_status.clear();
            self.ui
                .text_input(cx, ids!(layout_name_input))
                .set_text(cx, "");
            script_eval!(cx, {
                mod.state.current_screen = "layout_create";
                mod.state.sidebar_collapsed = true;
                ui.main_view.render()
            });
            self.render_layouts(cx);
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
        }

        for (index, id) in [
            live_id!(layout_custom_1),
            live_id!(layout_custom_2),
            live_id!(layout_custom_3),
            live_id!(layout_custom_4),
        ]
        .into_iter()
        .enumerate()
        {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if let Some(layout) = self.saved_layouts.get(index).cloned() {
                    self.select_layout(layout.kind, Some(index), &layout.name, cx);
                }
            }
        }

        if self
            .ui
            .button(cx, ids!(layout_create_back))
            .clicked(actions)
            || self.ui.button(cx, ids!(layout_cancel)).clicked(actions)
        {
            self.create_layout_open = false;
            script_eval!(cx, {
                mod.state.current_screen = "layouts";
                ui.main_view.render()
            });
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(layout_save)).clicked(actions) {
            let name = self
                .ui
                .text_input(cx, ids!(layout_name_input))
                .text()
                .trim()
                .to_string();
            if name.is_empty() {
                self.layout_status = "Enter a name before saving.".into();
            } else if self.saved_layouts.len() >= 4 {
                self.layout_status = "You can save up to four custom layouts for now.".into();
            } else {
                let layout = SavedLayout {
                    name: name.clone(),
                    kind: LayoutKind::Custom,
                    controls: default_custom_controls(),
                };
                self.saved_layouts.push(layout);
                match persist_saved_layouts(&self.saved_layouts) {
                    Ok(()) => {
                        let index = self.saved_layouts.len() - 1;
                        self.layout_status = format!("Saved {name}.");
                        self.create_layout_open = false;
                        self.select_layout(LayoutKind::Custom, Some(index), &name, cx);
                    }
                    Err(error) => {
                        self.saved_layouts.pop();
                        self.layout_status = format!("Could not save layout: {error}");
                    }
                }
            }
            self.render_layouts(cx);
        }

        if self
            .ui
            .button(cx, ids!(layout_keyboard_toggle))
            .clicked(actions)
            || self
                .ui
                .button(cx, ids!(landscape_keyboard_toggle))
                .clicked(actions)
        {
            let kind = if self.selected_layout_kind == LayoutKind::Keyboard {
                LayoutKind::Touchpad
            } else {
                LayoutKind::Keyboard
            };
            self.select_layout(kind, None, kind.label(), cx);
        }

        for (id, button_mask) in [
            (live_id!(surface_left_click), 1),
            (live_id!(surface_middle_click), 4),
            (live_id!(surface_right_click), 2),
        ] {
            if self.ui.button(cx, &[id]).clicked(actions) {
                self.send_mouse_click(button_mask);
            }
        }
        for (id, button) in [
            (live_id!(gamepad_a), 0),
            (live_id!(gamepad_b), 1),
            (live_id!(gamepad_x), 2),
            (live_id!(gamepad_y), 3),
            (live_id!(gamepad_lb), 4),
            (live_id!(gamepad_rb), 5),
            (live_id!(gamepad_lt), 6),
            (live_id!(gamepad_rt), 7),
            (live_id!(gamepad_select), 8),
            (live_id!(gamepad_start), 9),
            (live_id!(gamepad_home), 10),
        ] {
            if self.ui.button(cx, &[id]).clicked(actions) {
                self.send_bluetooth_command(BluetoothCommand::HidGamepadPress(button));
            }
        }
        for (id, direction) in [
            (live_id!(gamepad_dpad_up), 0),
            (live_id!(gamepad_dpad_right), 2),
            (live_id!(gamepad_dpad_down), 4),
            (live_id!(gamepad_dpad_left), 6),
        ] {
            if self.ui.button(cx, &[id]).clicked(actions) {
                self.send_bluetooth_command(BluetoothCommand::HidGamepadHat(direction));
            }
        }
        self.handle_custom_actions(cx, actions);
        self.handle_keyboard_actions(cx, actions);

        if self
            .ui
            .button(cx, ids!(layout_settings_button))
            .clicked(actions)
            || self
                .ui
                .button(cx, ids!(landscape_settings_button))
                .clicked(actions)
        {
            self.open_custom_settings(cx);
        }

        if self
            .ui
            .button(cx, ids!(layout_settings_back))
            .clicked(actions)
            || self
                .ui
                .button(cx, ids!(layout_settings_save))
                .clicked(actions)
        {
            if self.custom_button_focused {
                self.save_custom_changes();
            }
            self.layout_surface_active = true;
            self.custom_learning = false;
            self.custom_button_focused = false;
            #[cfg(target_os = "android")]
            self.set_control_fullscreen(cx, true);
            #[cfg(target_os = "android")]
            self.request_hid_start();
            let custom_edit_mode = self.custom_edit_mode;
            script_eval!(cx, {
                mod.state.custom_edit_mode = #(custom_edit_mode);
                mod.state.current_screen = "layout_surface";
                mod.state.custom_button_focused = false;
                ui.main_view.render()
            });
            self.schedule_layout_refresh(cx);
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
            self.render_layouts(cx);
        }

        if self
            .ui
            .button(cx, ids!(layout_settings_delete))
            .clicked(actions)
        {
            self.remove_custom_control(cx);
        }

        if self.ui.button(cx, ids!(pointer_slow)).clicked(actions) {
            self.pointer_speed = 0;
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(pointer_standard)).clicked(actions) {
            self.pointer_speed = 1;
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(pointer_fast)).clicked(actions) {
            self.pointer_speed = 2;
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(touchpad_tap_toggle))
            .clicked(actions)
        {
            self.tap_to_click = !self.tap_to_click;
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(touchpad_scroll_toggle))
            .clicked(actions)
        {
            self.natural_scrolling = !self.natural_scrolling;
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(keyboard_qwerty_choice))
            .clicked(actions)
        {
            self.keyboard_layout_index = (self.keyboard_layout_index + 1) % 3;
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(keyboard_repeat_toggle))
            .clicked(actions)
        {
            self.keyboard_repeat = !self.keyboard_repeat;
            self.render_layouts(cx);
        }

        for (index, id) in [
            live_id!(bluetooth_device_1),
            live_id!(bluetooth_device_2),
            live_id!(bluetooth_device_3),
            live_id!(bluetooth_device_4),
        ]
        .into_iter()
        .enumerate()
        {
            if self.ui.button(cx, &[id]).clicked(actions) {
                self.select_bluetooth_device(cx, index);
            }
        }

        if self.ui.button(cx, ids!(connect_device)).clicked(actions) {
            self.connect_selected_bluetooth_device(cx);
        }

        if self
            .ui
            .button(cx, ids!(device_detail_back))
            .clicked(actions)
        {
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            self.ui.widget(cx, ids!(device_panel)).set_visible(cx, true);
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.selected_usb_id = None;
        }

        if self
            .ui
            .button(cx, ids!(device_tab_overview))
            .clicked(actions)
        {
            self.device_diagnostics_open = false;
            self.render_device_detail(cx);
        }

        if self
            .ui
            .button(cx, ids!(device_tab_diagnostics))
            .clicked(actions)
        {
            self.device_diagnostics_open = true;
            self.render_device_detail(cx);
        }

        if self.ui.button(cx, ids!(diagnostics_clear)).clicked(actions) {
            self.diagnostic_events.clear();
            #[cfg(target_os = "linux")]
            if let (Some(host), Some(serial)) = (&self.usb_host, &self.selected_usb_id) {
                host.clear_events(serial);
            }
            self.render_diagnostics(cx);
        }

        if self
            .ui
            .button(cx, ids!(system_bluetooth_settings))
            .clicked(actions)
        {
            open_system_bluetooth_settings();
            self.ui
                .label(cx, ids!(bluetooth_status))
                .set_text(cx, "Opened system Bluetooth settings.");
        }

        if self
            .ui
            .button(cx, ids!(mobile_menu_toggle))
            .clicked(actions)
            || self
                .ui
                .button(cx, ids!(landscape_menu_toggle))
                .clicked(actions)
        {
            script_eval!(cx, {
                mod.state.sidebar_collapsed = false;
                ui.main_view.render()
            });
        }

        if self.ui.button(cx, ids!(sidebar_toggle)).clicked(actions) {
            script_eval!(cx, {
                mod.state.sidebar_collapsed = !mod.state.sidebar_collapsed;
                ui.main_view.render()
            });
        }

        if self.ui.button(cx, ids!(mobile_backdrop)).clicked(actions) {
            script_eval!(cx, {
                mod.state.sidebar_collapsed = true;
                ui.main_view.render()
            });
        }

        if self.ui.button(cx, ids!(nav_dashboard)).clicked(actions) {
            self.release_custom_controls();
            self.custom_learning = false;
            self.custom_button_focused = false;
            self.custom_add_menu_open = false;
            self.layout_surface_active = false;
            self.hid_start_pending = false;
            #[cfg(target_os = "android")]
            self.set_control_fullscreen(cx, false);
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            script_eval!(cx, {
                mod.state.current_screen = "dashboard";
                mod.state.sidebar_collapsed = true;
                mod.state.custom_button_focused = false;
                mod.state.custom_add_menu_open = false;
                ui.main_view.render()
            });
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
        }

        if self.ui.button(cx, ids!(nav_devices)).clicked(actions) {
            self.release_custom_controls();
            self.custom_learning = false;
            self.custom_button_focused = false;
            self.custom_add_menu_open = false;
            self.layout_surface_active = false;
            self.hid_start_pending = false;
            #[cfg(target_os = "android")]
            self.set_control_fullscreen(cx, false);
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            script_eval!(cx, {
                mod.state.current_screen = "devices";
                mod.state.sidebar_collapsed = true;
                mod.state.custom_button_focused = false;
                mod.state.custom_add_menu_open = false;
                ui.main_view.render()
            });
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
            self.render_usb_devices(cx);
            self.render_bluetooth_devices(cx);
        }

        if self.ui.button(cx, ids!(nav_layouts)).clicked(actions) {
            self.release_custom_controls();
            self.custom_learning = false;
            self.custom_button_focused = false;
            self.custom_add_menu_open = false;
            self.layout_surface_active = false;
            self.hid_start_pending = false;
            #[cfg(target_os = "android")]
            self.set_control_fullscreen(cx, false);
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            script_eval!(cx, {
                mod.state.current_screen = "layouts";
                mod.state.sidebar_collapsed = true;
                mod.state.custom_button_focused = false;
                mod.state.custom_add_menu_open = false;
                ui.main_view.render()
            });
            self.render_layouts(cx);
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        crate::makepad_widgets::script_mod(vm);

        if !vm.is_reload() {
            let (window_width, window_height) = initial_window_size();
            let density = UiDensity::for_window(window_width, cfg!(target_os = "android"));
            let initial_screen = match std::env::var("OPENWASD_START_SCREEN").as_deref() {
                Ok("devices") => "devices",
                Ok("layouts") => "layouts",
                Ok("layout_surface") => "layout_surface",
                Ok("layout_settings") => "layout_settings",
                _ => "dashboard",
            };
            let initial_layout_kind = match std::env::var("OPENWASD_START_LAYOUT").as_deref() {
                Ok("keyboard") => "keyboard",
                _ => "touchpad",
            };
            script_eval!(vm, {
                mod.state = {
                    sidebar_collapsed: true,
                    is_compact: #(density.compact),
                    is_narrow: #(density.narrow),
                    page_inset: #(density.page_inset),
                    page_bottom_inset: #(density.page_bottom_inset),
                    page_gap: #(density.page_gap),
                    header_button_size: #(density.header_button_size),
                    surface_title_size: #(density.surface_title_size),
                    surface_subtitle_size: #(density.surface_subtitle_size),
                    surface_button_height: #(density.surface_button_height),
                    current_screen: #(initial_screen),
                    selected_layout_kind: #(initial_layout_kind),
                    custom_edit_mode: false,
                                    custom_button_focused: false,
                                    custom_keyboard_picker: false,
                                    custom_add_menu_open: false,
                    window_width: #(window_width),
                    window_height: #(window_height)
                };
                true
            });
        }

        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.handle_touchpad_input(cx, event);
        self.handle_custom_drag(cx, event);
        self.handle_custom_key(cx, event);
        match event {
            Event::Pause | Event::Background | Event::WindowLostFocus(_) | Event::Shutdown => {
                self.release_custom_controls();
                self.custom_learning = false;
                #[cfg(target_os = "linux")]
                if matches!(event, Event::Shutdown) {
                    self.usb_host = None;
                }
            }
            Event::Startup => {
                log!("[OpenWASD][Bluetooth] app startup; starting connected-device refresh timer");
                self.bluetooth_timer = Some(cx.start_interval(0.2));
                self.bluetooth_refresh_timer = Some(cx.start_interval(3.0));
                self.refresh_system_bluetooth_devices();
                self.render_usb_devices(cx);
                #[cfg(target_os = "android")]
                if self.layout_surface_active {
                    self.set_control_fullscreen(cx, true);
                    self.request_hid_start();
                }
                self.render_layouts(cx);
                if self.startup_screenshot.is_some() {
                    self.startup_screenshot_timer = Some(cx.start_timeout(0.25));
                }
            }
            #[cfg(target_os = "android")]
            Event::Resume if self.layout_surface_active => {
                self.set_control_fullscreen(cx, true);
                self.request_hid_start();
            }
            Event::Timer(_)
                if self
                    .layout_refresh_timer
                    .as_ref()
                    .is_some_and(|timer| timer.is_event(event).is_some()) =>
            {
                self.layout_refresh_timer = None;
                self.render_layouts(cx);
            }
            Event::Timer(_)
                if self
                    .custom_drag_suppress_timer
                    .as_ref()
                    .is_some_and(|timer| timer.is_event(event).is_some()) =>
            {
                self.custom_drag_suppress_timer = None;
                self.custom_drag_suppress_click = false;
            }
            Event::Timer(_)
                if self
                    .startup_screenshot_timer
                    .as_ref()
                    .is_some_and(|timer| timer.is_event(event).is_some()) =>
            {
                self.startup_screenshot_timer = None;
                if let Some(path) = self.startup_screenshot.take() {
                    request_screenshot(cx, path);
                }
            }
            Event::KeyDown(key) if key.key_code == KeyCode::F12 && !key.is_repeat => {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                request_screenshot(
                    cx,
                    PathBuf::from("screenshots").join(format!("openwasd-{timestamp}.png")),
                );
            }
            _ => {}
        }

        if self
            .bluetooth_timer
            .as_ref()
            .is_some_and(|timer| timer.is_event(event).is_some())
        {
            if self.device_diagnostics_open && self.selected_usb_id.is_some() {
                self.render_diagnostics(cx);
            }
            while let Some(event) = self
                .bluetooth
                .as_ref()
                .and_then(BluetoothService::try_next_event)
            {
                self.apply_bluetooth_event(cx, event);
            }
        }

        if self
            .bluetooth_refresh_timer
            .as_ref()
            .is_some_and(|timer| timer.is_event(event).is_some())
        {
            #[cfg(target_os = "android")]
            if self.hid_start_pending && self.layout_surface_active {
                if self.hid_start_retries < 20 {
                    self.hid_start_retries += 1;
                    self.send_bluetooth_command(BluetoothCommand::RetryStartHid);
                } else {
                    self.hid_start_pending = false;
                    self.ui.label(cx, ids!(bluetooth_status)).set_text(
                        cx,
                        "Bluetooth permission is still needed. Grant Nearby devices access, then reopen the layout.",
                    );
                }
            }
            self.refresh_system_bluetooth_devices();
            self.render_usb_devices(cx);
            #[cfg(target_os = "android")]
            if self.layout_surface_active {
                self.render_layouts(cx);
            }
        }

        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

impl App {
    #[cfg(target_os = "android")]
    fn set_control_fullscreen(&self, cx: &mut Cx, enabled: bool) {
        let window = self.ui.window(cx, ids!(main_window));
        if enabled {
            window.fullscreen(cx);
        } else {
            window.disable_fullscreen(cx);
        }
    }

    fn render_usb_devices(&mut self, cx: &mut Cx) {
        #[cfg(target_os = "android")]
        {
            let connected = self
                .usb_client
                .as_ref()
                .is_some_and(|client| client.is_connected());
            self.ui
                .label(cx, ids!(usb_title))
                .set_text(cx, "USB companion");
            self.ui.label(cx, ids!(usb_status)).set_text(
                cx,
                if connected {
                    "Connected to a computer · gamepad, keyboard and mouse ready"
                } else {
                    "Waiting for a computer to connect"
                },
            );
            self.ui.label(cx, ids!(usb_capabilities)).set_text(
                cx,
                "Connect by USB. Select this device on the Linux computer's Devices page.",
            );
            return;
        }
        #[cfg(not(target_os = "android"))]
        {
            let inventory = openwasd_usb::connected_devices();
            let status = match &inventory {
                openwasd_usb::Inventory::Available(devices) if devices.is_empty() => {
                    "No USB devices attached.".to_owned()
                }
                openwasd_usb::Inventory::Available(devices) => devices
                    .iter()
                    .map(|device| {
                        format!(
                            "●  {}  ·  USB attached  ·  {:04x}:{:04x}",
                            device.name, device.vendor_id, device.product_id
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                openwasd_usb::Inventory::Unavailable(reason) => reason.clone(),
            };
            self.ui.label(cx, ids!(usb_status)).set_text(cx, &status);
            #[cfg(target_os = "linux")]
            {
                self.usb_companions = match &inventory {
                    openwasd_usb::Inventory::Available(devices) => devices.clone(),
                    openwasd_usb::Inventory::Unavailable(_) => Vec::new(),
                };
                self.ui.label(cx, ids!(usb_bridge_status)).set_text(
                    cx,
                    "USB input uses Android Open Accessory mode; device support and USB permissions vary.",
                );
                for (index, id) in [
                    live_id!(usb_companion_1),
                    live_id!(usb_companion_2),
                    live_id!(usb_companion_3),
                    live_id!(usb_companion_4),
                ]
                .into_iter()
                .enumerate()
                {
                    let inspect = [
                        live_id!(usb_inspect_1),
                        live_id!(usb_inspect_2),
                        live_id!(usb_inspect_3),
                        live_id!(usb_inspect_4),
                    ][index];
                    if let Some(device) = self.usb_companions.get(index) {
                        let state = self
                            .usb_host
                            .as_ref()
                            .and_then(|host| host.status(&device.id))
                            .unwrap_or_else(|| "Tap to connect".into());
                        self.ui
                            .button(cx, &[id])
                            .set_text(cx, &format!("{} · USB · {}", device.name, state));
                        self.ui.widget(cx, &[id]).set_visible(cx, true);
                        self.ui.widget(cx, &[inspect]).set_visible(
                            cx,
                            self.usb_host
                                .as_ref()
                                .and_then(|host| host.status(&device.id))
                                .is_some(),
                        );
                    } else {
                        self.ui.widget(cx, &[id]).set_visible(cx, false);
                        self.ui.widget(cx, &[inspect]).set_visible(cx, false);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "android")]
    fn request_hid_start(&mut self) {
        self.hid_start_pending = true;
        self.hid_start_retries = 0;
        self.send_bluetooth_command(BluetoothCommand::StartHid);
    }

    fn send_bluetooth_command(&mut self, command: BluetoothCommand) {
        #[cfg(target_os = "android")]
        if let Some(client) = self
            .usb_client
            .as_ref()
            .filter(|client| client.is_connected())
        {
            use openwasd_usb::bridge::GamepadMessage;
            let message = match &command {
                BluetoothCommand::HidGamepad(report) => Some(GamepadMessage::Report(*report)),
                BluetoothCommand::HidGamepadPress(button) => Some(GamepadMessage::Press(*button)),
                BluetoothCommand::HidGamepadHat(hat) => Some(GamepadMessage::Hat(*hat)),
                BluetoothCommand::HidKeyboard(report) => Some(GamepadMessage::Keyboard(*report)),
                BluetoothCommand::HidKey(report) => Some(GamepadMessage::Key(*report)),
                BluetoothCommand::HidMouse {
                    buttons,
                    dx,
                    dy,
                    wheel,
                } => Some(GamepadMessage::Mouse {
                    buttons: *buttons as u8,
                    dx: *dx,
                    dy: *dy,
                    wheel: *wheel,
                }),
                BluetoothCommand::HidClick(buttons) => Some(GamepadMessage::Click(*buttons as u8)),
                _ => None,
            };
            if let Some(message) = message {
                if !client.send(message) {
                    log!("[OpenWASD][USB] gamepad queue failed; closing session");
                }
                return;
            }
        }
        #[cfg(target_os = "android")]
        if matches!(
            &command,
            BluetoothCommand::HidMouse { .. }
                | BluetoothCommand::HidKeyboard(_)
                | BluetoothCommand::HidGamepad(_)
                | BluetoothCommand::HidClick(_)
                | BluetoothCommand::HidKey(_)
                | BluetoothCommand::HidGamepadPress(_)
                | BluetoothCommand::HidGamepadHat(_)
        ) && !self.hid_ready
        {
            log_hid_input_command("suppressed: Android HID profile is not ready", &command);
            if !self.hid_start_pending {
                self.request_hid_start();
            }
            return;
        }
        #[cfg(target_os = "android")]
        if matches!(
            &command,
            BluetoothCommand::HidMouse { .. }
                | BluetoothCommand::HidKeyboard(_)
                | BluetoothCommand::HidGamepad(_)
                | BluetoothCommand::HidClick(_)
                | BluetoothCommand::HidKey(_)
                | BluetoothCommand::HidGamepadPress(_)
                | BluetoothCommand::HidGamepadHat(_)
        ) && match &command {
            BluetoothCommand::HidMouse { .. } | BluetoothCommand::HidClick(_) => {
                !self.hid_host.mouse_ready()
            }
            BluetoothCommand::HidGamepad(_)
            | BluetoothCommand::HidGamepadPress(_)
            | BluetoothCommand::HidGamepadHat(_) => !self.hid_host.gamepad_ready(),
            _ => !self.hid_host.keyboard_ready(),
        } {
            log_hid_input_command(
                &format!(
                    "suppressed: host has no matching HID report subscription; host={:?} report_mask=0x{:02x}",
                    self.hid_host.name, self.hid_host.report_mask
                ),
                &command,
            );
            return;
        }
        if let Some(sender) = self
            .bluetooth
            .as_ref()
            .map(BluetoothService::command_sender)
        {
            if let Err(error) = sender.try_send(command.clone()) {
                log!("[OpenWASD][HID] input command could not be queued: {error}");
            } else {
                log_hid_input_command(
                    &format!("input queued for host {:?}", self.hid_host.name),
                    &command,
                );
            }
        }
    }

    fn send_mouse_click(&mut self, button: i8) {
        self.send_bluetooth_command(BluetoothCommand::HidClick(button));
    }

    fn send_keyboard_press(&mut self, usage: u8, modifier: u8) {
        let mut report = [0; 8];
        report[0] = modifier;
        if usage != 0 {
            report[2] = usage;
        }
        self.send_bluetooth_command(BluetoothCommand::HidKey(report));
    }

    fn selected_custom_control(&self, index: usize) -> Option<CustomControl> {
        self.selected_custom_layout
            .and_then(|layout| self.saved_layouts.get(layout))
            .filter(|layout| layout.kind == LayoutKind::Custom)
            .and_then(|layout| layout.controls.get(index))
            .cloned()
    }

    fn save_custom_changes(&mut self) {
        self.custom_status = match persist_saved_layouts(&self.saved_layouts) {
            Ok(()) => "Saved on this device.".into(),
            Err(error) => format!("Could not save: {error}"),
        };
    }

    fn load_custom_label(&mut self, cx: &mut Cx) {
        let control = self.selected_custom_control(self.custom_selected_control);
        let label = control
            .as_ref()
            .map(|control| control.label.as_str())
            .unwrap_or("");
        self.ui
            .text_input(cx, ids!(custom_label_input))
            .set_text(cx, label);
        self.ui.text_input(cx, ids!(custom_text_input)).set_text(
            cx,
            control
                .as_ref()
                .and_then(|control| control.text_output.as_deref())
                .unwrap_or(""),
        );
        self.ui.text_input(cx, ids!(custom_icon_input)).set_text(
            cx,
            control
                .as_ref()
                .and_then(|control| control.custom_icon.as_deref())
                .unwrap_or(""),
        );
        self.ui
            .text_input(cx, ids!(custom_function_search))
            .set_text(cx, "");
        self.custom_function_filter.clear();
    }

    fn filtered_custom_functions(&self) -> Vec<ControlAction> {
        let query = self.custom_function_filter.trim().to_ascii_lowercase();
        CUSTOM_OUTPUTS
            .iter()
            .filter(|(label, action)| {
                custom_output_group(*action) != 0 && label.to_ascii_lowercase().contains(&query)
            })
            .map(|(_, action)| *action)
            .collect()
    }

    fn render_custom_function_picker(&mut self, cx: &mut Cx) {
        let actions = self.filtered_custom_functions();
        let ids = [
            live_id!(custom_function_1),
            live_id!(custom_function_2),
            live_id!(custom_function_3),
            live_id!(custom_function_4),
            live_id!(custom_function_5),
            live_id!(custom_function_6),
            live_id!(custom_function_7),
            live_id!(custom_function_8),
            live_id!(custom_function_9),
            live_id!(custom_function_10),
            live_id!(custom_function_11),
            live_id!(custom_function_12),
            live_id!(custom_function_13),
            live_id!(custom_function_14),
            live_id!(custom_function_15),
            live_id!(custom_function_16),
            live_id!(custom_function_17),
        ];
        for (index, id) in ids.into_iter().enumerate() {
            let action = actions.get(index).copied();
            self.ui.widget(cx, &[id]).set_visible(cx, action.is_some());
            let label = action.map(custom_output_label).unwrap_or_default();
            self.ui.button(cx, &[id]).set_text(cx, &label);
        }
    }

    fn set_custom_keyboard_action(&mut self, cx: &mut Cx, usage: u8) {
        if let Some(control) = self
            .selected_custom_layout
            .and_then(|index| self.saved_layouts.get_mut(index))
            .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
        {
            control.action = ControlAction::Keyboard(usage);
            control.text_output = None;
            self.custom_status = "Keyboard shortcut saved.".into();
            self.save_custom_changes();
        }
        self.render_layouts(cx);
    }

    fn send_text_output(&mut self, text: &str) {
        let mut unsupported = false;
        for character in text.chars() {
            if let Some((usage, modifier)) = ascii_to_hid_key(character) {
                let mut report = [0; 8];
                report[0] = modifier;
                report[2] = usage;
                self.send_bluetooth_command(BluetoothCommand::HidKeyboard(report));
                self.send_bluetooth_command(BluetoothCommand::HidKeyboard([0; 8]));
            } else {
                unsupported = true;
            }
        }
        if unsupported {
            self.custom_status =
                "Some characters need a US keyboard mapping and were skipped.".into();
        }
    }

    fn custom_report(&mut self, action: ControlAction) {
        let reports = self.active_controls.reports();
        match action {
            ControlAction::Keyboard(_) => {
                self.send_bluetooth_command(BluetoothCommand::HidKeyboard(reports.keyboard))
            }
            ControlAction::GamepadButton(_) => {
                self.send_bluetooth_command(BluetoothCommand::HidGamepad(reports.gamepad))
            }
            ControlAction::MouseButton(_) => {
                self.send_bluetooth_command(BluetoothCommand::HidMouse {
                    buttons: reports.mouse_buttons as i8,
                    dx: 0,
                    dy: 0,
                    wheel: 0,
                })
            }
            ControlAction::BluetoothSettings => {}
        }
    }

    fn press_custom_control(&mut self, source: String, action: ControlAction) {
        if action == ControlAction::BluetoothSettings {
            open_system_bluetooth_settings();
            return;
        }
        self.active_controls.press(source, action);
        self.custom_report(action);
    }

    fn release_custom_control(&mut self, source: &str, action: ControlAction) {
        self.active_controls.release(source);
        self.custom_report(action);
    }

    fn release_custom_controls(&mut self) {
        if self.active_controls.is_empty() {
            return;
        }
        self.active_controls.clear();
        self.custom_report(ControlAction::Keyboard(0));
        self.custom_report(ControlAction::GamepadButton(0));
        self.custom_report(ControlAction::MouseButton(0));
    }

    fn add_custom_control(&mut self, cx: &mut Cx, duplicate: bool) {
        let Some(layout_index) = self.selected_custom_layout else {
            return;
        };
        if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
            if layout.controls.len() >= CUSTOM_LIMIT {
                self.custom_status =
                    format!("This layout can contain up to {CUSTOM_LIMIT} buttons.");
            } else {
                let next_index = layout.controls.len();
                let control = if duplicate {
                    layout
                        .controls
                        .get(self.custom_selected_control)
                        .cloned()
                        .map(|mut control| {
                            control.label = format!("{} copy", control.label);
                            control.hardware_key = None;
                            control
                        })
                        .unwrap_or(CustomControl {
                            label: format!("Button {}", next_index + 1),
                            action: ControlAction::Keyboard(44),
                            text_output: None,
                            hardware_key: None,
                            show_text: true,
                            show_icon: false,
                            custom_icon: None,
                        })
                } else {
                    CustomControl {
                        label: format!("Button {}", next_index + 1),
                        action: ControlAction::Keyboard(44),
                        text_output: None,
                        hardware_key: None,
                        show_text: true,
                        show_icon: false,
                        custom_icon: None,
                    }
                };
                layout.controls.push(control);
                self.custom_selected_control = next_index;
                self.custom_button_focused = true;
                self.custom_status = "Button added. Select it to edit its label or action.".into();
                self.save_custom_changes();
                self.load_custom_label(cx);
            }
        }
        self.render_layouts(cx);
    }

    fn remove_custom_control(&mut self, cx: &mut Cx) {
        let Some(layout_index) = self.selected_custom_layout else {
            return;
        };
        if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
            if self.custom_selected_control < layout.controls.len() {
                layout.controls.remove(self.custom_selected_control);
                self.custom_selected_control = self
                    .custom_selected_control
                    .min(layout.controls.len().saturating_sub(1));
                self.custom_learning = false;
                self.custom_status = "Selected button removed.".into();
                self.save_custom_changes();
                self.load_custom_label(cx);
            } else {
                self.custom_status = "Select a button to remove it.".into();
            }
        }
        self.render_layouts(cx);
    }

    fn move_custom_control(&mut self, cx: &mut Cx, move_left: bool) {
        let Some(layout_index) = self.selected_custom_layout else {
            return;
        };
        if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
            let from = self.custom_selected_control;
            let to = if move_left {
                from.checked_sub(1)
            } else {
                from.checked_add(1)
            };
            if let Some(to) =
                to.filter(|to| *to < layout.controls.len() && from < layout.controls.len())
            {
                layout.controls.swap(from, to);
                self.custom_selected_control = to;
                self.save_custom_changes();
                self.custom_status = "Button order updated.".into();
                self.load_custom_label(cx);
            }
        }
        self.render_layouts(cx);
    }

    fn open_custom_settings(&mut self, cx: &mut Cx) {
        self.release_custom_controls();
        self.custom_learning = false;
        self.layout_surface_active = false;
        #[cfg(target_os = "android")]
        self.set_control_fullscreen(cx, false);
        let custom_edit_mode = self.custom_edit_mode;
        script_eval!(cx, {
            mod.state.custom_edit_mode = #(custom_edit_mode);
            mod.state.current_screen = "layout_settings";
            ui.main_view.render()
        });
        self.schedule_layout_refresh(cx);
        self.ui
            .view(cx, ids!(scroll_body))
            .set_scroll_pos(cx, dvec2(0.0, 0.0));
        self.render_layouts(cx);
    }

    fn schedule_layout_refresh(&mut self, cx: &mut Cx) {
        self.layout_refresh_timer = Some(cx.start_timeout(0.05));
    }

    fn close_custom_add_menu(&mut self, cx: &mut Cx) {
        self.custom_add_menu_open = false;
        script_eval!(cx, {
            mod.state.custom_add_menu_open = false;
            ui.main_view.render()
        });
    }

    fn handle_custom_key(&mut self, cx: &mut Cx, event: &Event) {
        let (name, down) = match event {
            Event::KeyDown(key) if !key.is_repeat => (format!("{:?}", key.key_code), true),
            Event::KeyUp(key) => (format!("{:?}", key.key_code), false),
            _ => return,
        };
        if name == "Unknown" {
            return;
        }
        if self.custom_learning && down {
            if let Some(layout_index) = self.selected_custom_layout {
                if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
                    for control in &mut layout.controls {
                        if control.hardware_key.as_deref() == Some(&name) {
                            control.hardware_key = None;
                        }
                    }
                    if let Some(control) = layout.controls.get_mut(self.custom_selected_control) {
                        control.hardware_key = Some(name.clone());
                        self.custom_learning = false;
                        self.save_custom_changes();
                        self.custom_status = format!("Assigned {name}. {}", self.custom_status);
                        self.render_layouts(cx);
                    }
                }
            }
            return;
        }
        if !self.layout_surface_active || self.selected_layout_kind != LayoutKind::Custom {
            return;
        }
        let control = self
            .selected_custom_layout
            .and_then(|index| self.saved_layouts.get(index))
            .and_then(|layout| {
                layout
                    .controls
                    .iter()
                    .find(|control| control.hardware_key.as_deref() == Some(&name))
            })
            .cloned();
        if let Some(control) = control {
            let source = format!("hardware:{name}");
            if let Some(text) = control.text_output.as_deref() {
                if down {
                    self.send_text_output(text);
                }
            } else if down {
                self.press_custom_control(source, control.action);
            } else {
                self.release_custom_control(&source, control.action);
            }
        }
    }

    fn handle_custom_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.layout_surface_active
            && self.selected_layout_kind == LayoutKind::Custom
            && self
                .ui
                .button(cx, ids!(layout_custom_edit_toggle))
                .clicked(actions)
        {
            self.release_custom_controls();
            self.custom_edit_mode = !self.custom_edit_mode;
            self.custom_button_focused = false;
            self.custom_add_menu_open = false;
            self.custom_status = if self.custom_edit_mode {
                String::new()
            } else {
                "Layout saved on this device.".into()
            };
            let edit_mode = self.custom_edit_mode;
            script_eval!(cx, {
                mod.state.custom_edit_mode = #(edit_mode);
                mod.state.custom_button_focused = false;
                mod.state.custom_add_menu_open = false;
                ui.main_view.render()
            });
            self.schedule_layout_refresh(cx);
            self.render_layouts(cx);
        }
        if self.layout_surface_active
            && self.selected_layout_kind == LayoutKind::Custom
            && self.custom_edit_mode
            && self
                .ui
                .button(cx, ids!(layout_custom_add_menu_toggle))
                .clicked(actions)
        {
            self.custom_add_menu_open = !self.custom_add_menu_open;
            let menu_open = self.custom_add_menu_open;
            script_eval!(cx, {
                mod.state.custom_add_menu_open = #(menu_open);
                ui.main_view.render()
            });
            self.render_layouts(cx);
        }
        let surface_ids = [
            live_id!(custom_surface_1),
            live_id!(custom_surface_2),
            live_id!(custom_surface_3),
            live_id!(custom_surface_4),
            live_id!(custom_surface_5),
            live_id!(custom_surface_6),
        ];
        if self.layout_surface_active && self.selected_layout_kind == LayoutKind::Custom {
            for (index, id) in surface_ids.into_iter().enumerate() {
                if let Some(control) = self.selected_custom_control(index) {
                    let button = self.ui.button(cx, &[id]);
                    if self.custom_edit_mode {
                        if button.clicked(actions) {
                            if self.custom_drag_suppress_click {
                                self.custom_drag_suppress_click = false;
                                self.custom_drag_suppress_timer = None;
                            } else {
                                self.custom_selected_control = index;
                                self.custom_button_focused = true;
                                self.load_custom_label(cx);
                                self.open_custom_settings(cx);
                            }
                        }
                        continue;
                    }
                    let source = format!("touch:{index}");
                    if let Some(text) = control.text_output.as_deref() {
                        if button.pressed(actions) {
                            self.send_text_output(text);
                        }
                    } else if control.action == ControlAction::BluetoothSettings {
                        if button.clicked(actions) {
                            self.press_custom_control(source, control.action);
                        }
                    } else {
                        if button.pressed(actions) {
                            self.press_custom_control(source.clone(), control.action);
                        }
                        if button.clicked(actions) || button.released(actions) {
                            self.release_custom_control(&source, control.action);
                        }
                    }
                }
            }

            if self.ui.button(cx, ids!(custom_add_button)).clicked(actions) {
                self.close_custom_add_menu(cx);
                self.add_custom_control(cx, false);
            }
            if self
                .ui
                .button(cx, ids!(custom_add_touchpad))
                .clicked(actions)
            {
                self.close_custom_add_menu(cx);
                self.custom_status =
                    "Touchpad components are not supported in custom layouts yet.".into();
                self.render_layouts(cx);
            }
            if self.ui.button(cx, ids!(custom_add_speech)).clicked(actions) {
                self.close_custom_add_menu(cx);
                self.custom_status = "Speech input is not supported in custom layouts yet.".into();
                self.render_layouts(cx);
            }
            if self
                .ui
                .button(cx, ids!(custom_add_keyboard_toggle))
                .clicked(actions)
            {
                self.close_custom_add_menu(cx);
                self.custom_status =
                    "Keyboard toggle controls are not supported in custom layouts yet.".into();
                self.render_layouts(cx);
            }
        }
        if self.layout_surface_active || self.selected_layout_kind != LayoutKind::Custom {
            return;
        }
        let Some(layout_index) = self.selected_custom_layout else {
            return;
        };
        let edit_ids = [
            live_id!(custom_edit_1),
            live_id!(custom_edit_2),
            live_id!(custom_edit_3),
            live_id!(custom_edit_4),
            live_id!(custom_edit_5),
            live_id!(custom_edit_6),
        ];
        for (index, id) in edit_ids.into_iter().enumerate() {
            if self.ui.button(cx, &[id]).clicked(actions)
                && self.selected_custom_control(index).is_some()
            {
                self.custom_selected_control = index;
                self.custom_button_focused = true;
                self.custom_learning = false;
                self.load_custom_label(cx);
                self.render_layouts(cx);
            }
        }
        if self.ui.button(cx, ids!(custom_add)).clicked(actions) {
            self.add_custom_control(cx, false);
        }
        if self.ui.button(cx, ids!(custom_delete)).clicked(actions) {
            self.remove_custom_control(cx);
        }
        let move_left = self.ui.button(cx, ids!(custom_move_left)).clicked(actions);
        let move_right = self.ui.button(cx, ids!(custom_move_right)).clicked(actions);
        if move_left || move_right {
            self.move_custom_control(cx, move_left);
        }
        if self.ui.button(cx, ids!(custom_label_save)).clicked(actions) {
            let label = self
                .ui
                .text_input(cx, ids!(custom_label_input))
                .text()
                .trim()
                .to_string();
            if label.is_empty() {
                self.custom_status = "Enter a button label.".into();
            } else if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.label = label.chars().take(24).collect();
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(custom_show_text_toggle))
            .clicked(actions)
        {
            if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.show_text = !control.show_text;
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(custom_show_icon_toggle))
            .clicked(actions)
        {
            if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.show_icon = !control.show_icon;
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(custom_icon_default))
            .clicked(actions)
        {
            if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.custom_icon = None;
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(custom_icon_set)).clicked(actions) {
            let icon = self
                .ui
                .text_input(cx, ids!(custom_icon_input))
                .text()
                .trim()
                .chars()
                .take(4)
                .collect::<String>();
            if icon.is_empty() {
                self.custom_status = "Enter an emoji or symbol for the icon.".into();
            } else if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.custom_icon = Some(icon);
                self.custom_status = "Custom icon saved.".into();
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        for (id, tab) in [
            (live_id!(custom_group_keyboard), 0),
            (live_id!(custom_group_function), 1),
            (live_id!(custom_group_text), 2),
            (live_id!(custom_group_hardware), 3),
            (live_id!(custom_group_settings), 4),
        ] {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if tab == 3 {
                    cx.set_key_focus(Area::Empty);
                    cx.hide_text_ime();
                }
                self.custom_editor_tab = tab;
                self.render_layouts(cx);
            }
        }
        if let Some(filter) = self
            .ui
            .text_input(cx, ids!(custom_function_search))
            .changed(actions)
        {
            self.custom_function_filter = filter;
            self.render_custom_function_picker(cx);
        }
        for (index, id) in [
            live_id!(custom_function_1),
            live_id!(custom_function_2),
            live_id!(custom_function_3),
            live_id!(custom_function_4),
            live_id!(custom_function_5),
            live_id!(custom_function_6),
            live_id!(custom_function_7),
            live_id!(custom_function_8),
            live_id!(custom_function_9),
            live_id!(custom_function_10),
            live_id!(custom_function_11),
            live_id!(custom_function_12),
            live_id!(custom_function_13),
            live_id!(custom_function_14),
            live_id!(custom_function_15),
            live_id!(custom_function_16),
            live_id!(custom_function_17),
        ]
        .into_iter()
        .enumerate()
        {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if let Some(action) = self.filtered_custom_functions().get(index).copied()
                    && let Some(control) = self
                        .saved_layouts
                        .get_mut(layout_index)
                        .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
                {
                    control.action = action;
                    control.text_output = None;
                    self.save_custom_changes();
                }
                self.render_layouts(cx);
            }
        }
        if self.ui.button(cx, ids!(custom_text_save)).clicked(actions) {
            let text = self
                .ui
                .text_input(cx, ids!(custom_text_input))
                .text()
                .to_string();
            if text.is_empty() {
                self.custom_status = "Enter text to send.".into();
            } else if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.text_output = Some(text.chars().take(120).collect());
                self.custom_status =
                    "Text output saved. Unsupported characters are skipped.".into();
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(custom_action_test))
            .clicked(actions)
        {
            if let Some(control) = self.selected_custom_control(self.custom_selected_control) {
                if let Some(text) = control.text_output.as_deref() {
                    self.send_text_output(text);
                } else if control.action != ControlAction::BluetoothSettings {
                    self.press_custom_control("editor-test".into(), control.action);
                    self.release_custom_control("editor-test", control.action);
                }
            }
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(custom_learn)).clicked(actions)
            && self
                .selected_custom_control(self.custom_selected_control)
                .is_some()
        {
            cx.set_key_focus(Area::Empty);
            cx.hide_text_ime();
            self.custom_learning = true;
            self.custom_status =
                "Listening. Press a physical keyboard or controller button.".into();
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(custom_unbind)).clicked(actions) {
            if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                control.hardware_key = None;
                self.custom_learning = false;
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
    }

    fn handle_keyboard_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let editing_custom_shortcut = !self.layout_surface_active
            && self.selected_layout_kind == LayoutKind::Custom
            && self.custom_button_focused
            && self.custom_editor_tab == 0;
        for (id, modifier) in [
            (live_id!(key_shift), 2),
            (live_id!(key_shift2), 2),
            (live_id!(sb_shift), 2),
            (live_id!(sb_shift2), 2),
            (live_id!(key_ctrl), 1),
            (live_id!(key_ctrl2), 1),
            (live_id!(sm_ctrl), 1),
            (live_id!(sm_ctrl2), 1),
            (live_id!(key_alt), 4),
            (live_id!(key_alt2), 4),
            (live_id!(sm_alt), 4),
            (live_id!(sm_alt2), 4),
        ] {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if editing_custom_shortcut {
                    let usage = match modifier {
                        1 => 224,
                        2 => 225,
                        4 => 226,
                        _ => return,
                    };
                    self.set_custom_keyboard_action(cx, usage);
                    return;
                }
                self.keyboard_modifiers ^= modifier;
                let mut report = [0; 8];
                report[0] = self.keyboard_modifiers;
                self.send_bluetooth_command(BluetoothCommand::HidKeyboard(report));
            }
        }
        let keys: &[(LiveId, u8, u8)] = &[
            (live_id!(key_esc), 41, 0),
            (live_id!(key_backspace), 42, 0),
            (live_id!(key_tab), 43, 0),
            (live_id!(key_enter1), 40, 0),
            (live_id!(key_enter2), 40, 0),
            (live_id!(key_space), 44, 0),
            (live_id!(key_caps), 57, 0),
            (live_id!(key_up), 82, 0),
            (live_id!(key_down), 81, 0),
            (live_id!(key_comma), 54, 0),
            (live_id!(key_period), 55, 0),
            (live_id!(key_slash), 56, 0),
            (live_id!(key_semicolon), 51, 0),
            (live_id!(key_quote), 52, 0),
            (live_id!(key_bracket), 47, 0),
            (live_id!(key_bracket2), 48, 0),
            (live_id!(key_num1), 53, 0),
            (live_id!(key_num2), 31, 0),
            (live_id!(key_num3), 32, 0),
            (live_id!(key_num4), 33, 0),
            (live_id!(key_num5), 34, 0),
            (live_id!(key_num6), 35, 0),
            (live_id!(key_num7), 36, 0),
            (live_id!(key_num8), 37, 0),
            (live_id!(key_num9), 38, 0),
            (live_id!(key_num10), 39, 0),
            (live_id!(key_num11), 30, 0),
            (live_id!(key_num12), 45, 0),
            (live_id!(key_num13), 46, 0),
            (live_id!(key_a), 4, 0),
            (live_id!(key_b), 5, 0),
            (live_id!(key_c), 6, 0),
            (live_id!(key_d), 7, 0),
            (live_id!(key_e), 8, 0),
            (live_id!(key_f), 9, 0),
            (live_id!(key_g), 10, 0),
            (live_id!(key_h), 11, 0),
            (live_id!(key_i), 12, 0),
            (live_id!(key_j), 13, 0),
            (live_id!(key_k), 14, 0),
            (live_id!(key_l), 15, 0),
            (live_id!(key_m), 16, 0),
            (live_id!(key_n), 17, 0),
            (live_id!(key_o), 18, 0),
            (live_id!(key_p), 19, 0),
            (live_id!(key_q), 20, 0),
            (live_id!(key_r), 21, 0),
            (live_id!(key_s), 22, 0),
            (live_id!(key_t), 23, 0),
            (live_id!(key_u), 24, 0),
            (live_id!(key_v), 25, 0),
            (live_id!(key_w), 26, 0),
            (live_id!(key_x), 27, 0),
            (live_id!(key_y), 28, 0),
            (live_id!(key_z), 29, 0),
            (live_id!(sk_f1), 58, 0),
            (live_id!(sk_f2), 59, 0),
            (live_id!(sk_f3), 60, 0),
            (live_id!(sk_f4), 61, 0),
            (live_id!(sk_f5), 62, 0),
            (live_id!(sk_f6), 63, 0),
            (live_id!(sk_f7), 64, 0),
            (live_id!(sk_f8), 65, 0),
            (live_id!(sk_f9), 66, 0),
            (live_id!(sk_f10), 67, 0),
            (live_id!(sk_f11), 68, 0),
            (live_id!(sk_f12), 69, 0),
            // Keys in the full-screen keyboard use their own widget IDs.
            // Keep them wired to the same HID usages as the compact preview.
            (live_id!(st_q), 20, 0),
            (live_id!(st_w), 26, 0),
            (live_id!(st_e), 8, 0),
            (live_id!(st_r), 21, 0),
            (live_id!(st_t), 23, 0),
            (live_id!(st_y), 28, 0),
            (live_id!(st_u), 24, 0),
            (live_id!(st_i), 12, 0),
            (live_id!(st_o), 18, 0),
            (live_id!(st_p), 19, 0),
            (live_id!(st_tab), 43, 0),
            (live_id!(st_enter), 40, 0),
            (live_id!(sh_a), 4, 0),
            (live_id!(sh_s), 22, 0),
            (live_id!(sh_d), 7, 0),
            (live_id!(sh_f), 9, 0),
            (live_id!(sh_g), 10, 0),
            (live_id!(sh_h), 11, 0),
            (live_id!(sh_j), 13, 0),
            (live_id!(sh_k), 14, 0),
            (live_id!(sh_l), 15, 0),
            (live_id!(sh_caps), 57, 0),
            (live_id!(sh_enter), 40, 0),
            (live_id!(sb_z), 29, 0),
            (live_id!(sb_x), 27, 0),
            (live_id!(sb_c), 6, 0),
            (live_id!(sb_v), 25, 0),
            (live_id!(sb_b), 5, 0),
            (live_id!(sb_n), 17, 0),
            (live_id!(sb_m), 16, 0),
            (live_id!(sn_1), 53, 0),
            (live_id!(sn_2), 30, 0),
            (live_id!(sn_3), 31, 0),
            (live_id!(sn_4), 32, 0),
            (live_id!(sn_5), 33, 0),
            (live_id!(sn_6), 34, 0),
            (live_id!(sn_7), 35, 0),
            (live_id!(sn_8), 36, 0),
            (live_id!(sn_9), 37, 0),
            (live_id!(sn_10), 38, 0),
            (live_id!(sn_11), 39, 0),
            (live_id!(sn_backspace), 42, 0),
            (live_id!(ss_minus), 45, 0),
            (live_id!(ss_equal), 46, 0),
            (live_id!(ss_open_bracket), 47, 0),
            (live_id!(ss_close_bracket), 48, 0),
            (live_id!(ss_backslash), 49, 0),
            (live_id!(ss_semicolon), 51, 0),
            (live_id!(ss_quote), 52, 0),
            (live_id!(ss_comma), 54, 0),
            (live_id!(ss_period), 55, 0),
            (live_id!(ss_slash), 56, 0),
            (live_id!(sm_space), 44, 0),
            (live_id!(key_menu), 101, 0),
        ];
        for (id, usage, modifier) in keys {
            if self.ui.button(cx, &[*id]).clicked(actions) {
                if editing_custom_shortcut {
                    self.set_custom_keyboard_action(cx, *usage);
                    return;
                }
                self.send_keyboard_press(*usage, self.keyboard_modifiers | *modifier);
                self.keyboard_modifiers = 0;
            }
        }
    }

    fn handle_touchpad_input(&mut self, cx: &mut Cx, event: &Event) {
        if !self.layout_surface_active || self.selected_layout_kind != LayoutKind::Touchpad {
            return;
        }
        let area = self.ui.widget(cx, ids!(touchpad_surface)).area();
        if !area.is_valid(cx) {
            self.touchpad_last = None;
            self.touchpad_touch_uid = None;
            self.touchpad_start_point = None;
            self.touchpad_started_at = None;
            return;
        }
        let rect = area.rect(cx);
        let speed = match self.pointer_speed {
            0 => 0.5,
            2 => 1.75,
            _ => 1.0,
        };
        match event {
            Event::TouchUpdate(update) => {
                for touch in &update.touches {
                    let point = (touch.abs.x, touch.abs.y);
                    match touch.state {
                        makepad_widgets::makepad_platform::event::TouchState::Start
                            if rect.contains(touch.abs) =>
                        {
                            self.touchpad_touch_uid = Some(touch.uid);
                            self.touchpad_last = Some(point);
                            self.touchpad_start_point = Some(point);
                            self.touchpad_started_at = Some(touch.time);
                        }
                        makepad_widgets::makepad_platform::event::TouchState::Move
                            if self.touchpad_touch_uid == Some(touch.uid) =>
                        {
                            self.emit_pointer_delta(point, speed)
                        }
                        makepad_widgets::makepad_platform::event::TouchState::Stop
                            if self.touchpad_touch_uid == Some(touch.uid) =>
                        {
                            let start = self.touchpad_start_point.take();
                            let started_at = self.touchpad_started_at.take();
                            if self.tap_to_click
                                && start.is_some_and(|start| {
                                    (point.0 - start.0).hypot(point.1 - start.1) < 12.0
                                })
                                && started_at
                                    .is_some_and(|started_at| touch.time - started_at < 0.28)
                            {
                                self.send_mouse_click(1);
                            }
                            self.touchpad_touch_uid = None;
                            self.touchpad_last = None;
                        }
                        _ => {}
                    }
                }
            }
            Event::MouseMove(mouse) if rect.contains(mouse.abs) => {
                self.emit_pointer_delta((mouse.abs.x, mouse.abs.y), speed)
            }
            Event::MouseDown(mouse) if rect.contains(mouse.abs) => {
                self.touchpad_last = Some((mouse.abs.x, mouse.abs.y))
            }
            Event::MouseUp(_) => self.touchpad_last = None,
            _ => {}
        }
    }

    fn handle_custom_drag(&mut self, cx: &mut Cx, event: &Event) {
        if !self.layout_surface_active
            || self.selected_layout_kind != LayoutKind::Custom
            || !self.custom_edit_mode
        {
            self.custom_drag_candidate = None;
            self.custom_drag_active = false;
            return;
        }
        let ids = [
            live_id!(custom_surface_1),
            live_id!(custom_surface_2),
            live_id!(custom_surface_3),
            live_id!(custom_surface_4),
            live_id!(custom_surface_5),
            live_id!(custom_surface_6),
        ];
        let Event::TouchUpdate(update) = event else {
            return;
        };
        for touch in &update.touches {
            let point = (touch.abs.x, touch.abs.y);
            match touch.state {
                makepad_widgets::makepad_platform::event::TouchState::Start => {
                    self.custom_drag_active = false;
                    self.custom_drag_candidate = ids.iter().enumerate().find_map(|(index, id)| {
                        if self.selected_custom_control(index).is_none() {
                            return None;
                        }
                        let area = self.ui.widget(cx, &[*id]).area();
                        (area.is_valid(cx) && area.rect(cx).contains(touch.abs))
                            .then_some((touch.uid, index, point, touch.time))
                    });
                }
                makepad_widgets::makepad_platform::event::TouchState::Move => {
                    let Some((uid, from, start, started_at)) = self.custom_drag_candidate else {
                        continue;
                    };
                    if uid != touch.uid {
                        continue;
                    }
                    if touch.time - started_at >= 0.35
                        && (point.0 - start.0).hypot(point.1 - start.1) >= 12.0
                    {
                        self.custom_drag_active = true;
                    }
                    if !self.custom_drag_active {
                        continue;
                    }
                    let target = ids.iter().enumerate().find_map(|(index, id)| {
                        if self.selected_custom_control(index).is_none() {
                            return None;
                        }
                        let area = self.ui.widget(cx, &[*id]).area();
                        (area.is_valid(cx) && area.rect(cx).contains(touch.abs)).then_some(index)
                    });
                    let Some(target) = target else {
                        continue;
                    };
                    if target == from {
                        continue;
                    }
                    let Some(layout_index) = self.selected_custom_layout else {
                        continue;
                    };
                    let changed = if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
                        if from < layout.controls.len() && target < layout.controls.len() {
                            layout.controls.swap(from, target);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if changed {
                        self.custom_drag_suppress_click = true;
                        self.custom_drag_suppress_timer = Some(cx.start_timeout(0.5));
                        self.custom_selected_control = target;
                        self.custom_drag_candidate = Some((uid, target, start, started_at));
                        self.custom_status = "Button moved.".into();
                        self.save_custom_changes();
                        self.load_custom_label(cx);
                        self.render_layouts(cx);
                    }
                }
                makepad_widgets::makepad_platform::event::TouchState::Stop => {
                    if self
                        .custom_drag_candidate
                        .is_some_and(|(uid, _, _, _)| uid == touch.uid)
                    {
                        self.custom_drag_candidate = None;
                        self.custom_drag_active = false;
                    }
                }
                _ => {}
            }
        }
    }

    fn emit_pointer_delta(&mut self, point: (f64, f64), speed: f64) {
        if let Some(last) = self.touchpad_last {
            let dx = ((point.0 - last.0) * speed).round().clamp(-127.0, 127.0) as i8;
            let dy = ((point.1 - last.1) * speed).round().clamp(-127.0, 127.0) as i8;
            if dx != 0 || dy != 0 {
                self.send_bluetooth_command(BluetoothCommand::HidMouse {
                    buttons: 0,
                    dx,
                    dy,
                    wheel: 0,
                });
            }
        }
        self.touchpad_last = Some(point);
    }

    fn select_layout(
        &mut self,
        kind: LayoutKind,
        custom_index: Option<usize>,
        name: &str,
        cx: &mut Cx,
    ) {
        self.release_custom_controls();
        self.custom_edit_mode = false;
        self.custom_button_focused = false;
        self.custom_add_menu_open = false;
        self.custom_learning = false;
        self.custom_selected_control = 0;
        self.selected_layout_kind = kind;
        self.selected_custom_layout = custom_index;
        self.layout_surface_active = true;
        #[cfg(target_os = "android")]
        self.request_hid_start();
        let kind_name = match kind {
            LayoutKind::Keyboard => "keyboard",
            LayoutKind::Gamepad => "gamepad",
            LayoutKind::Touchpad => "touchpad",
            LayoutKind::Custom => "custom",
        };
        script_eval!(cx, {
            mod.state.current_screen = "layout_surface";
            mod.state.selected_layout_kind = #(kind_name);
            mod.state.custom_edit_mode = false;
            mod.state.custom_button_focused = false;
            mod.state.custom_add_menu_open = false;
            mod.state.sidebar_collapsed = true;
            ui.main_view.render()
        });
        self.schedule_layout_refresh(cx);
        #[cfg(target_os = "android")]
        self.set_control_fullscreen(cx, true);
        self.ui
            .view(cx, ids!(scroll_body))
            .set_scroll_pos(cx, dvec2(0.0, 0.0));
        self.ui
            .label(cx, ids!(layout_settings_name))
            .set_text(cx, &format!("{name} settings"));
        self.render_layouts(cx);
    }

    fn render_layouts(&mut self, cx: &mut Cx) {
        self.ui
            .widget(cx, ids!(layout_create_form))
            .set_visible(cx, self.create_layout_open);
        let is_keyboard = self.selected_layout_kind == LayoutKind::Keyboard;
        let is_touchpad = self.selected_layout_kind == LayoutKind::Touchpad;
        let is_custom = self.selected_layout_kind == LayoutKind::Custom;
        let control_count = self
            .selected_custom_layout
            .and_then(|index| self.saved_layouts.get(index))
            .map(|layout| layout.controls.len())
            .unwrap_or(0);
        let (window_width, _) = initial_window_size();
        let density = UiDensity::for_window(window_width, cfg!(target_os = "android"));
        if self.layout_surface_active {
            self.ui.widget(cx, ids!(page_header)).set_visible(cx, true);
            self.ui
                .widget(cx, ids!(layout_surface_heading))
                .set_visible(cx, true);
            self.ui
                .widget(cx, ids!(layout_toolbar))
                .set_visible(cx, true);
        }
        self.ui
            .widget(cx, ids!(layout_custom_edit_toggle))
            .set_visible(cx, self.layout_surface_active && is_custom);
        self.ui.widget(cx, ids!(mobile_menu_toggle)).set_visible(
            cx,
            self.layout_surface_active
                || !is_custom
                || !self.custom_edit_mode
                || !self.custom_button_focused,
        );
        self.ui
            .widget(cx, ids!(layout_custom_add_menu_toggle))
            .set_visible(
                cx,
                self.layout_surface_active && is_custom && self.custom_edit_mode,
            );
        self.ui
            .widget(cx, ids!(layout_settings_button))
            .set_visible(cx, self.layout_surface_active);
        self.ui
            .widget(cx, ids!(layout_settings_subtitle))
            .set_visible(cx, is_custom);
        let selected_control = self.selected_custom_control(self.custom_selected_control);
        let custom_editor_active = is_custom && self.custom_edit_mode && self.custom_button_focused;
        let custom_keyboard_picker = custom_editor_active && self.custom_editor_tab == 0;
        script_eval!(cx, {
            mod.state.custom_keyboard_picker = #(custom_keyboard_picker);
            ui.main_view.render()
        });
        self.ui
            .widget(cx, ids!(layout_surface_panel))
            .set_visible(cx, self.layout_surface_active || custom_keyboard_picker);
        self.ui
            .widget(cx, ids!(layout_settings_delete))
            .set_visible(cx, !self.layout_surface_active && custom_editor_active);
        self.ui
            .widget(cx, ids!(layout_settings_save))
            .set_visible(cx, !self.layout_surface_active && custom_editor_active);
        self.ui
            .widget(cx, ids!(custom_editor_manage))
            .set_visible(cx, !self.custom_edit_mode);
        self.ui
            .widget(cx, ids!(custom_editor_order))
            .set_visible(cx, !self.custom_edit_mode);
        self.ui
            .widget(cx, ids!(custom_surface_add_menu))
            .set_visible(
                cx,
                is_custom && self.custom_edit_mode && self.custom_add_menu_open,
            );
        self.ui
            .widget(cx, ids!(custom_surface_edit_status))
            .set_visible(cx, is_custom && self.custom_edit_mode);
        self.ui
            .widget(cx, ids!(custom_surface_row_2))
            .set_visible(cx, is_custom && control_count > 3);
        self.ui
            .widget(cx, ids!(custom_editor_row_2))
            .set_visible(cx, is_custom && control_count > 3);
        self.ui
            .widget(cx, ids!(custom_editor_intro))
            .set_visible(cx, !self.custom_button_focused);
        self.ui
            .widget(cx, ids!(custom_editor_list))
            .set_visible(cx, !self.custom_button_focused);
        self.ui
            .widget(cx, ids!(custom_editor_groups))
            .set_visible(cx, custom_editor_active);
        self.ui
            .widget(cx, ids!(custom_function_panel))
            .set_visible(cx, custom_editor_active && self.custom_editor_tab == 1);
        self.ui
            .widget(cx, ids!(custom_text_panel))
            .set_visible(cx, custom_editor_active && self.custom_editor_tab == 2);
        self.ui
            .widget(cx, ids!(custom_hardware_panel))
            .set_visible(cx, custom_editor_active && self.custom_editor_tab == 3);
        self.ui
            .widget(cx, ids!(custom_settings_panel))
            .set_visible(cx, custom_editor_active && self.custom_editor_tab == 4);
        self.ui
            .widget(cx, ids!(custom_editor_action))
            .set_visible(cx, custom_editor_active);
        if let Some(control) = selected_control.as_ref() {
            self.ui.button(cx, ids!(custom_show_text_toggle)).set_text(
                cx,
                &format!(
                    "Show text  ·  {}",
                    if control.show_text { "On" } else { "Off" }
                ),
            );
            self.ui.button(cx, ids!(custom_show_icon_toggle)).set_text(
                cx,
                &format!(
                    "Show icon  ·  {}",
                    if control.show_icon { "On" } else { "Off" }
                ),
            );
            self.ui.button(cx, ids!(custom_icon_default)).set_text(
                cx,
                &if control.custom_icon.is_none() {
                    format!("✓ Default icon  ·  {}", custom_control_icon(control))
                } else {
                    "Use default icon".to_string()
                },
            );
        }
        self.render_custom_function_picker(cx);
        self.ui
            .widget(cx, ids!(touchpad_surface))
            .set_visible(cx, is_touchpad);
        self.ui
            .widget(cx, ids!(layout_surface_mouse_buttons))
            .set_visible(cx, is_touchpad);
        self.ui
            .widget(cx, ids!(layout_surface_keyboard))
            .set_visible(cx, is_keyboard || custom_keyboard_picker);
        self.ui
            .widget(cx, ids!(surface_keyboard_function))
            .set_visible(
                cx,
                !density.narrow || (self.custom_button_focused && self.custom_editor_tab == 0),
            );
        self.ui
            .widget(cx, ids!(layout_surface_gamepad))
            .set_visible(cx, self.selected_layout_kind == LayoutKind::Gamepad);
        self.ui
            .widget(cx, ids!(layout_surface_custom))
            .set_visible(cx, is_custom && self.layout_surface_active);
        self.ui
            .widget(cx, ids!(layout_touchpad_settings))
            .set_visible(cx, is_touchpad);
        self.ui
            .widget(cx, ids!(layout_keyboard_settings))
            .set_visible(cx, is_keyboard);
        self.ui
            .widget(cx, ids!(layout_custom_settings))
            .set_visible(cx, is_custom);
        let preview_name = self
            .selected_custom_layout
            .and_then(|index| self.saved_layouts.get(index))
            .map(|layout| layout.name.as_str())
            .unwrap_or(self.selected_layout_kind.label());
        let surface_title = if is_custom && self.custom_edit_mode {
            if self.custom_button_focused {
                "Edit button"
            } else {
                "Edit layout"
            }
        } else {
            preview_name
        };
        self.ui
            .label(cx, ids!(layout_surface_title))
            .set_text(cx, surface_title);
        let settings_title = if is_custom && self.custom_edit_mode && self.custom_button_focused {
            "Edit button"
        } else {
            "Layout settings"
        };
        self.ui
            .label(cx, ids!(layout_settings_title))
            .set_text(cx, settings_title);
        let edit_subtitle = preview_name;
        self.ui
            .label(cx, ids!(layout_settings_subtitle))
            .set_text(cx, edit_subtitle);
        self.ui
            .widget(cx, ids!(layout_settings_name))
            .set_visible(cx, !self.custom_button_focused);
        #[cfg(target_os = "android")]
        let usb_gamepad = self
            .usb_client
            .as_ref()
            .is_some_and(|client| client.is_connected());
        #[cfg(not(target_os = "android"))]
        let usb_gamepad = false;
        let device_status = if usb_gamepad {
            "USB connected · gamepad, keyboard and mouse ready".to_string()
        } else if let Some(status) = self.hid_host.label() {
            status
        } else if self.hid_start_pending {
            "Preparing Bluetooth HID…".to_string()
        } else if self.hid_ready {
            "Bluetooth HID is ready; waiting for the computer connection".to_string()
        } else if cfg!(target_os = "android") {
            "Starting Bluetooth HID…".to_string()
        } else {
            "Open this layout on Android to connect a computer".to_string()
        };
        self.ui
            .label(cx, ids!(layout_surface_device_status))
            .set_text(
                cx,
                if is_custom && self.custom_edit_mode {
                    edit_subtitle
                } else {
                    &device_status
                },
            );
        self.ui
            .label(cx, ids!(custom_surface_edit_status))
            .set_text(cx, &self.custom_status);
        self.ui.label(cx, ids!(gamepad_status)).set_text(
            cx,
            if usb_gamepad {
                "USB gamepad connected."
            } else if self.hid_host.gamepad_ready() {
                "Bluetooth gamepad connected."
            } else if self.hid_ready {
                "Connect a Bluetooth host that enables the gamepad report."
            } else {
                "Start Bluetooth HID on Android to send gamepad presses."
            },
        );
        self.ui
            .label(cx, ids!(layout_preview_title))
            .set_text(cx, preview_name);
        self.ui
            .label(cx, ids!(layout_settings_name))
            .set_text(cx, &format!("{} settings", preview_name));
        self.ui
            .label(cx, ids!(layout_save_status))
            .set_text(cx, &self.layout_status);
        let surface_ids = [
            live_id!(custom_surface_1),
            live_id!(custom_surface_2),
            live_id!(custom_surface_3),
            live_id!(custom_surface_4),
            live_id!(custom_surface_5),
            live_id!(custom_surface_6),
        ];
        let edit_ids = [
            live_id!(custom_edit_1),
            live_id!(custom_edit_2),
            live_id!(custom_edit_3),
            live_id!(custom_edit_4),
            live_id!(custom_edit_5),
            live_id!(custom_edit_6),
        ];
        for (index, (surface_id, edit_id)) in surface_ids.into_iter().zip(edit_ids).enumerate() {
            let control = self.selected_custom_control(index);
            self.ui
                .widget(cx, &[surface_id])
                .set_visible(cx, is_custom && control.is_some());
            self.ui
                .widget(cx, &[edit_id])
                .set_visible(cx, is_custom && control.is_some());
            if let Some(control) = control {
                self.ui.button(cx, &[surface_id]).set_text(
                    cx,
                    &if self.custom_edit_mode && index == self.custom_selected_control {
                        format!("● {}", custom_control_surface_label(&control))
                    } else {
                        custom_control_surface_label(&control)
                    },
                );
                self.ui.button(cx, &[edit_id]).set_text(
                    cx,
                    &format!(
                        "{}{}",
                        if index == self.custom_selected_control {
                            "● "
                        } else {
                            ""
                        },
                        custom_control_surface_label(&control)
                    ),
                );
            }
        }
        let action_summary = selected_control
            .as_ref()
            .map(custom_control_output_label)
            .unwrap_or_else(|| "Select an output action".into());
        self.ui
            .label(cx, ids!(custom_action_label))
            .set_text(cx, &action_summary);
        for (id, tab, label) in [
            (live_id!(custom_group_keyboard), 0, "Keyboard shortcut"),
            (live_id!(custom_group_function), 1, "Function"),
            (live_id!(custom_group_text), 2, "Text"),
            (live_id!(custom_group_hardware), 3, "Hardware buttons"),
            (live_id!(custom_group_settings), 4, "Settings"),
        ] {
            self.ui.button(cx, &[id]).set_text(
                cx,
                &if self.custom_editor_tab == tab {
                    format!("{label}  ✓")
                } else {
                    label.to_string()
                },
            );
        }
        self.ui.button(cx, ids!(custom_learn)).set_text(
            cx,
            &if self.custom_learning {
                "Press a hardware button…".to_string()
            } else if let Some(key) = selected_control
                .as_ref()
                .and_then(|control| control.hardware_key.as_ref())
            {
                format!("Hardware: {key}  ·  Change")
            } else {
                "Assign hardware button".to_string()
            },
        );
        self.ui
            .label(cx, ids!(custom_editor_status))
            .set_text(cx, &self.custom_status);
        for (index, id, label) in [
            (0, live_id!(pointer_slow), "Slow"),
            (1, live_id!(pointer_standard), "Standard"),
            (2, live_id!(pointer_fast), "Fast"),
        ] {
            let text = if self.pointer_speed == index {
                format!("{label}  ✓")
            } else {
                label.to_string()
            };
            self.ui.button(cx, &[id]).set_text(cx, &text);
        }
        self.ui.button(cx, ids!(touchpad_tap_toggle)).set_text(
            cx,
            &format!(
                "Tap to click   ·   {}",
                if self.tap_to_click { "On" } else { "Off" }
            ),
        );
        self.ui.button(cx, ids!(touchpad_scroll_toggle)).set_text(
            cx,
            &format!(
                "Natural scrolling   ·   {}",
                if self.natural_scrolling { "On" } else { "Off" }
            ),
        );
        let keyboard_layout = match self.keyboard_layout_index {
            1 => "AZERTY   ·   French",
            2 => "QWERTZ   ·   German",
            _ => "QWERTY   ·   English (US)",
        };
        self.ui
            .button(cx, ids!(keyboard_qwerty_choice))
            .set_text(cx, keyboard_layout);
        self.ui.button(cx, ids!(keyboard_repeat_toggle)).set_text(
            cx,
            &format!(
                "Key repeat   ·   {}",
                if self.keyboard_repeat { "On" } else { "Off" }
            ),
        );

        for (index, id) in [
            live_id!(layout_custom_1),
            live_id!(layout_custom_2),
            live_id!(layout_custom_3),
            live_id!(layout_custom_4),
        ]
        .into_iter()
        .enumerate()
        {
            let widget = self.ui.button(cx, &[id]);
            if let Some(layout) = self.saved_layouts.get(index) {
                widget.set_text(
                    cx,
                    &format!("{}   ·   {}", layout.name, layout.kind.label()),
                );
                self.ui
                    .widget(cx, &[id])
                    .set_visible(cx, !self.create_layout_open);
            } else {
                self.ui.widget(cx, &[id]).set_visible(cx, false);
            }
        }
    }

    fn refresh_system_bluetooth_devices(&self) {
        if let Some(service) = &self.bluetooth {
            let result = service
                .command_sender()
                .try_send(BluetoothCommand::RefreshConnected);
            log!(
                "[OpenWASD][Bluetooth] queued connected-device refresh: {}",
                result.is_ok()
            );
            if let Err(error) = result {
                log!("[OpenWASD][Bluetooth] refresh queue error: {error}");
            }
        } else {
            log!("[OpenWASD][Bluetooth] refresh requested before service initialization");
        }
    }

    fn start_bluetooth_scan(&self, cx: &mut Cx) {
        let Some(service) = &self.bluetooth else {
            self.ui
                .label(cx, ids!(bluetooth_status))
                .set_text(cx, "Bluetooth is unavailable.");
            return;
        };
        if service
            .command_sender()
            .try_send(BluetoothCommand::Scan)
            .is_err()
        {
            log!("[OpenWASD][Bluetooth] BLE scan request could not be queued");
            self.ui
                .label(cx, ids!(bluetooth_status))
                .set_text(cx, "Bluetooth scanner is busy. Try again shortly.");
        } else {
            log!("[OpenWASD][Bluetooth] BLE scan request queued");
            self.ui
                .label(cx, ids!(bluetooth_status))
                .set_text(cx, "Scanning for nearby Bluetooth LE devices…");
        }
    }

    fn select_bluetooth_device(&mut self, cx: &mut Cx, index: usize) {
        let Some(device) = self.bluetooth_devices.get(index) else {
            return;
        };
        self.bluetooth_selected_id = Some(device.id.clone());
        self.selected_usb_id = None;
        self.device_detail_open = true;
        self.device_diagnostics_open = false;
        self.diagnostic_events.clear();
        self.ui
            .widget(cx, ids!(device_panel))
            .set_visible(cx, false);
        self.ui
            .widget(cx, ids!(device_detail_panel))
            .set_visible(cx, true);
        self.render_device_detail(cx);
    }

    fn render_device_detail(&self, cx: &mut Cx) {
        #[cfg(target_os = "linux")]
        if let Some(device_id) = &self.selected_usb_id {
            let name = self
                .usb_companions
                .iter()
                .find(|device| &device.id == device_id)
                .map(|device| device.name.as_str())
                .unwrap_or("USB device");
            let status = self
                .usb_host
                .as_ref()
                .and_then(|host| host.status(device_id))
                .unwrap_or_else(|| "Disconnected".into());
            self.ui
                .label(cx, ids!(device_detail_title))
                .set_text(cx, &format!("{name} input"));
            self.ui
                .label(cx, ids!(device_detail_connection))
                .set_text(cx, &format!("USB · {status}"));
            self.ui.label(cx, ids!(device_overview_details)).set_text(
                cx,
                &format!("Device: {name}\nTransport: USB accessory\nState: {status}"),
            );
            self.ui
                .label(cx, ids!(device_settings_hint))
                .set_text(cx, "Use the Devices page to disconnect this USB device.");
            self.ui
                .label(cx, ids!(diagnostics_hint))
                .set_text(cx, "Input received from this USB companion, newest first.");
        }
        let selected = self.bluetooth_selected_id.as_ref().and_then(|id| {
            self.bluetooth_devices
                .iter()
                .find(|device| &device.id == id)
        });
        if let Some(device) = selected {
            let name = device.name.as_deref().unwrap_or("Unnamed device");
            self.ui
                .label(cx, ids!(device_detail_title))
                .set_text(cx, &format!("{name} settings"));
            self.ui.label(cx, ids!(device_detail_connection)).set_text(
                cx,
                &format!(
                    "{}  ·  {}  ·  {}",
                    state_label(&device.state),
                    device.transport.label(),
                    device.kind.label()
                ),
            );
            self.ui.label(cx, ids!(device_overview_details)).set_text(
                cx,
                &format!(
                    "Device: {name}\nType: {}\nTransport: {}\nState: {}",
                    device.kind.label(),
                    device.transport.label(),
                    state_label(&device.state)
                ),
            );
        }
        self.ui
            .widget(cx, ids!(device_overview))
            .set_visible(cx, !self.device_diagnostics_open);
        self.ui
            .widget(cx, ids!(device_diagnostics))
            .set_visible(cx, self.device_diagnostics_open);
        self.render_diagnostics(cx);
    }

    fn render_diagnostics(&self, cx: &mut Cx) {
        #[cfg(target_os = "linux")]
        let usb_events = self
            .selected_usb_id
            .as_ref()
            .and_then(|serial| self.usb_host.as_ref().map(|host| host.events(serial)));
        #[cfg(not(target_os = "linux"))]
        let usb_events: Option<Vec<String>> = None;
        let events = usb_events.as_ref().unwrap_or(&self.diagnostic_events);
        let text = if events.is_empty() {
            if self.selected_usb_id.is_some() {
                "Waiting for input from this USB companion…".to_string()
            } else {
                "Device-specific Bluetooth input inspection is not available yet.".to_string()
            }
        } else {
            events.join("\n")
        };
        self.ui
            .label(cx, ids!(diagnostics_events))
            .set_text(cx, &text);
    }

    fn render_selected_bluetooth_device(&self, cx: &mut Cx) {
        let selected = self.bluetooth_selected_id.as_ref().and_then(|id| {
            self.bluetooth_devices
                .iter()
                .find(|device| &device.id == id)
        });
        if let Some(device) = selected {
            let name = device.name.as_deref().unwrap_or("Unnamed device");
            let signal = device
                .rssi_dbm
                .map(|rssi| format!("{rssi} dBm"))
                .unwrap_or_else(|| "signal unavailable".into());
            self.ui.label(cx, ids!(selected_bluetooth_device)).set_text(
                cx,
                &format!("{name}  ·  {}  ·  {signal}", state_label(&device.state)),
            );
            let system_managed = device.id.starts_with("system:");
            self.ui.button(cx, ids!(connect_device)).set_text(
                cx,
                if system_managed {
                    "Manage in Settings"
                } else if matches!(device.state, ConnectionState::Connected) {
                    "Disconnect"
                } else {
                    "Connect"
                },
            );
            self.ui
                .button(cx, ids!(connect_device))
                .set_disabled(cx, false);
        } else {
            self.ui
                .label(cx, ids!(selected_bluetooth_device))
                .set_text(cx, "Select a device to connect.");
            self.ui
                .button(cx, ids!(connect_device))
                .set_text(cx, "Connect");
            self.ui
                .button(cx, ids!(connect_device))
                .set_disabled(cx, true);
        }
    }

    fn connect_selected_bluetooth_device(&mut self, cx: &mut Cx) {
        let Some(id) = self.bluetooth_selected_id.clone() else {
            return;
        };
        if id.starts_with("system:") {
            open_system_bluetooth_settings();
            self.ui.label(cx, ids!(bluetooth_status)).set_text(
                cx,
                "This device is managed by the system Bluetooth settings.",
            );
            return;
        }
        let connected = self
            .bluetooth_devices
            .iter()
            .find(|device| device.id == id)
            .is_some_and(|device| matches!(device.state, ConnectionState::Connected));
        let command = if connected {
            BluetoothCommand::Disconnect(id)
        } else {
            BluetoothCommand::Connect(id)
        };
        let Some(service) = &self.bluetooth else {
            return;
        };
        if service.command_sender().try_send(command).is_err() {
            self.ui
                .label(cx, ids!(bluetooth_status))
                .set_text(cx, "Bluetooth is busy. Try again shortly.");
        } else {
            self.ui.label(cx, ids!(bluetooth_status)).set_text(
                cx,
                "Connection request sent. Pair in system settings first if prompted.",
            );
        }
    }

    fn render_bluetooth_devices(&self, cx: &mut Cx) {
        let row_ids = [
            live_id!(bluetooth_device_1),
            live_id!(bluetooth_device_2),
            live_id!(bluetooth_device_3),
            live_id!(bluetooth_device_4),
        ];
        for (index, id) in row_ids.into_iter().enumerate() {
            let Some(device) = self.bluetooth_devices.get(index) else {
                self.ui.widget(cx, &[id]).set_visible(cx, false);
                continue;
            };
            let name = device.name.as_deref().unwrap_or("Unnamed device");
            let state = state_label(&device.state);
            let signal = device
                .rssi_dbm
                .map(|rssi| format!("{rssi} dBm"))
                .unwrap_or_else(|| "signal n/a".into());
            self.ui.button(cx, &[id]).set_text(
                cx,
                &format!(
                    "{}   {name}  ·  {state}  ·  {signal}",
                    if matches!(device.state, ConnectionState::Connected) {
                        "●"
                    } else {
                        "○"
                    }
                ),
            );
            self.ui.widget(cx, &[id]).set_visible(cx, true);
        }
        self.ui
            .widget(cx, ids!(bluetooth_empty))
            .set_visible(cx, self.bluetooth_devices.is_empty());
        self.render_selected_bluetooth_device(cx);
        if self.device_detail_open {
            self.render_device_detail(cx);
        }
    }

    fn apply_bluetooth_event(&mut self, cx: &mut Cx, event: BluetoothEvent) {
        match event {
            BluetoothEvent::ScanStarted => {
                log!("[OpenWASD][Bluetooth] scan started");
                self.ui
                    .label(cx, ids!(bluetooth_status))
                    .set_text(cx, "Scanning for nearby Bluetooth LE devices…");
            }
            BluetoothEvent::Devices(devices) => {
                log!(
                    "[OpenWASD][Bluetooth] UI received scan result with {} device(s)",
                    devices.len()
                );
                for device in &devices {
                    log!(
                        "[OpenWASD][Bluetooth] scan device name={:?} state={:?} kind={:?}",
                        device.name,
                        device.state,
                        device.kind
                    );
                }
                self.bluetooth_devices = devices;
                self.render_bluetooth_devices(cx);
                let connected_name = self
                    .bluetooth_selected_id
                    .as_ref()
                    .and_then(|id| {
                        self.bluetooth_devices
                            .iter()
                            .find(|device| &device.id == id)
                    })
                    .filter(|device| matches!(device.state, ConnectionState::Connected))
                    .and_then(|device| device.name.as_deref());
                let status = connected_name
                    .map(|name| format!("Connected to {name}."))
                    .unwrap_or_else(|| {
                        format!(
                            "Found {} nearby Bluetooth LE device(s).",
                            self.bluetooth_devices.len()
                        )
                    });
                self.ui
                    .label(cx, ids!(bluetooth_status))
                    .set_text(cx, &status);
            }
            BluetoothEvent::SystemDevices(mut system_devices) => {
                let connected = system_devices
                    .iter()
                    .filter(|device| device.state == ConnectionState::Connected)
                    .count();
                let paired = system_devices
                    .iter()
                    .filter(|device| device.state == ConnectionState::Paired)
                    .count();
                log!(
                    "[OpenWASD][Bluetooth] system refresh: {connected} connected, {paired} paired ({} total)",
                    system_devices.len(),
                );
                for device in &system_devices {
                    log!(
                        "[OpenWASD][Bluetooth] system device id={:?} name={:?} state={:?} kind={:?} rssi_dbm={:?} services={:?}",
                        device.id,
                        device.name,
                        device.state,
                        device.kind,
                        device.rssi_dbm,
                        device.services,
                    );
                }
                self.bluetooth_devices
                    .retain(|device| !device.id.starts_with("system:"));
                merge_devices(
                    &mut self.bluetooth_devices,
                    system_devices.drain(..).collect(),
                );
                self.bluetooth_devices
                    .sort_by_key(|device| match &device.state {
                        ConnectionState::Connected => 0,
                        ConnectionState::Connecting => 1,
                        ConnectionState::Paired => 2,
                        ConnectionState::Discovered => 3,
                        ConnectionState::Pairing => 4,
                        ConnectionState::Reconnecting { .. } => 5,
                        ConnectionState::Disconnected => 6,
                        ConnectionState::Failed { .. } => 7,
                    });
                self.render_bluetooth_devices(cx);
                self.render_layouts(cx);
                if let Some(device) = self
                    .bluetooth_devices
                    .iter()
                    .find(|device| matches!(device.state, ConnectionState::Connected))
                {
                    let name = device.name.as_deref().unwrap_or("Bluetooth device");
                    self.ui
                        .label(cx, ids!(bluetooth_status))
                        .set_text(cx, &format!("Connected to {name}."));
                }
            }
            BluetoothEvent::ScanFinished => {
                let status = if self.bluetooth_devices.is_empty() {
                    "No Bluetooth LE devices found. Check that the device is advertising."
                } else {
                    "Scan complete. Select a device, then connect."
                };
                self.ui
                    .label(cx, ids!(bluetooth_status))
                    .set_text(cx, status);
            }
            BluetoothEvent::HidReady => {
                self.hid_ready = true;
                self.hid_start_pending = false;
                self.hid_start_retries = 0;
                self.render_layouts(cx);
                self.ui.label(cx, ids!(bluetooth_status)).set_text(
                    cx,
                    "Bluetooth HID service started. Connect to this device from the computer's Bluetooth settings to enable input.",
                );
            }
            BluetoothEvent::DeviceUpdated(device) => {
                if let Some(existing) = self
                    .bluetooth_devices
                    .iter_mut()
                    .find(|existing| existing.id == device.id)
                {
                    *existing = device;
                } else {
                    self.bluetooth_devices.push(device);
                }
                self.render_bluetooth_devices(cx);
            }
            BluetoothEvent::Error { device_id, message } => {
                log!("[OpenWASD][Bluetooth] adapter error for {device_id:?}: {message}");
                if message.contains("permissions requested") {
                    self.hid_start_pending = true;
                } else if message.contains("BLE HID peripheral")
                    || message.contains("advertis")
                    || message.contains("failed to resolve Java class")
                    || message.contains("class not found")
                {
                    self.hid_ready = false;
                    self.hid_start_pending = false;
                }
                self.ui
                    .label(cx, ids!(bluetooth_status))
                    .set_text(cx, &message);
                self.render_layouts(cx);
            }
            BluetoothEvent::HidHost(host) => {
                if self.hid_host.name != host.name {
                    self.release_custom_controls();
                }
                if !self.hid_host_state_seen || self.hid_host != host {
                    log!(
                        "[OpenWASD][HID] host state: name={:?} report_mask=0x{:02x} mouse_ready={} keyboard_ready={} gamepad_ready={}",
                        host.name,
                        host.report_mask,
                        host.mouse_ready(),
                        host.keyboard_ready(),
                        host.gamepad_ready(),
                    );
                    self.hid_host_state_seen = true;
                }
                self.hid_host = host;
                self.render_layouts(cx);
            }
        }
    }
}

fn log_hid_input_command(stage: &str, command: &BluetoothCommand) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static MOUSE_INPUTS: AtomicUsize = AtomicUsize::new(0);

    if let BluetoothCommand::HidMouse { .. } = command {
        let count = MOUSE_INPUTS.fetch_add(1, Ordering::Relaxed) + 1;
        if count != 1 && count % 25 != 0 {
            return;
        }
    }

    log!("[OpenWASD][HID] {stage}: {command:?}");
}

fn state_label(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Discovered => "Discovered",
        ConnectionState::Pairing => "Pairing",
        ConnectionState::Paired => "Paired",
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Connected => "Connected",
        ConnectionState::Reconnecting { .. } => "Reconnecting",
        ConnectionState::Disconnected => "Disconnected",
        ConnectionState::Failed { .. } => "Failed",
    }
}

fn open_system_bluetooth_settings() {
    #[cfg(target_os = "android")]
    if let Err(error) = openwasd_bluetooth::open_android_bluetooth_settings() {
        log!("[OpenWASD][Bluetooth] could not open system settings: {error}");
    }
    #[cfg(target_os = "macos")]
    let _ = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.BluetoothSettings")
        .spawn();
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd")
        .args(["/C", "start", "ms-settings:bluetooth"])
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg("settings://bluetooth").spawn();
}

fn request_screenshot(cx: &mut Cx, path: PathBuf) {
    cx.capture_next_frame_to_file(path.clone());
    log!("Screenshot requested: {}", path.display());
}

fn initial_window_size() -> (u32, u32) {
    let Some(size) = std::env::var("OPENWASD_WINDOW_SIZE").ok() else {
        return (1100, 720);
    };
    let Some((width, height)) = size.split_once('x') else {
        return (1100, 720);
    };
    let (Ok(width), Ok(height)) = (width.parse::<u32>(), height.parse::<u32>()) else {
        return (1100, 720);
    };
    if width < 320 || height < 320 {
        return (1100, 720);
    }
    (width, height)
}
