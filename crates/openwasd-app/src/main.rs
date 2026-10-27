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

fn custom_output_label(action: ControlAction) -> &'static str {
    CUSTOM_OUTPUTS
        .iter()
        .find(|(_, candidate)| *candidate == action)
        .map(|(label, _)| *label)
        .unwrap_or("Unknown output")
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
    vec![
        CustomControl {
            label: "Action A".into(),
            action: ControlAction::GamepadButton(0),
            hardware_key: None,
        },
        CustomControl {
            label: "Space".into(),
            action: ControlAction::Keyboard(44),
            hardware_key: None,
        },
    ]
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
                                spacing: 12
                                margin: Inset{ left: 0 }
                                padding: Inset{ top: if state.is_compact { 14 } else { 24 }, left: if state.is_compact { 14 } else { 24 }, right: if state.is_compact { 14 } else { 24 }, bottom: if state.is_compact { 30 } else { 40 } }
                                show_bg: true
                                draw_bg +: { color: #xf5f8fa }

                                page_header := View {
                                    width: Fill
                                    height: Fit
                                    visible: !(state.current_screen == "layout_surface" && state.selected_layout_kind == "keyboard" && state.is_compact && !state.is_narrow)
                                    flow: Right
                                    spacing: if state.is_narrow { 8 } else { 14 }
                                    align: Align{ y: 0.5 }

                                    mobile_menu_toggle := AppButton {
                                        width: 42
                                        height: 42
                                        visible: true
                                        text: "☰"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 17
                                    }

                                    page_title := Label {
                                        width: Fit
                                        visible: state.current_screen != "layout_surface"
                                        text: if state.current_screen == "devices" { "Devices" } else if state.current_screen == "layouts" { "Layouts" } else if state.current_screen == "layout_surface" { if state.selected_layout_kind == "gamepad" { "Gamepad" } else { "Mouse / Keyboard" } } else if state.current_screen == "layout_settings" { "Layout settings" } else { "Dashboard" }
                                        draw_text.color: #x18263a
                                        draw_text.text_style.font_size: if state.is_narrow { 18 } else { 22 }
                                    }

                                    layout_surface_heading := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.current_screen == "layout_surface"
                                        flow: Down
                                        spacing: 2

                                        layout_surface_title := Label {
                                            width: Fill
                                            text: if state.selected_layout_kind == "gamepad" { "Gamepad" } else { "Mouse / Keyboard" }
                                            draw_text.color: #x18263a
                                            draw_text.text_style.font_size: if state.is_narrow { 19 } else { 25 }
                                        }

                                        layout_surface_device_status := Label {
                                            width: Fill
                                            text: "Waiting for a computer to connect"
                                            draw_text.color: #x6b7c8c
                                            draw_text.text_style.font_size: if state.is_narrow { 11 } else { 13 }
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
                                        visible: state.current_screen != "layout_surface" && state.current_screen != "layout_settings"
                                        flow: Right
                                        spacing: 8

                                        wifi_connect := AppButton {
                                            width: if state.is_narrow { 40 } else { 82 }
                                            height: 40
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

                                        bluetooth_connect := AppButton {
                                            width: if state.is_narrow { 40 } else { 104 }
                                            height: 40
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

                                        usb_connect := AppButton {
                                            width: if state.is_narrow { 40 } else { 72 }
                                            height: 40
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
                                        spacing: 8

                                        layout_keyboard_toggle := AppButton {
                                            width: 42
                                            height: 42
                                            text: ""
                                            icon_walk: Walk{ width: 20, height: 20 }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: crate_resource("self:resources/keyboard.svg")
                                            }
                                            draw_bg.color: #eef3f7
                                            draw_bg.color_hover: #dcebf0
                                            draw_bg.color_down: #c9e8ed
                                            draw_text.color: #x213246
                                            draw_text.text_style.font_size: 20
                                        }

                                        layout_settings_button := AppButton {
                                            width: 42
                                            height: 42
                                            text: ""
                                            icon_walk: Walk{ width: 20, height: 20 }
                                            draw_icon +: {
                                                color: #x00a7b8
                                                svg: crate_resource("self:resources/settings.svg")
                                            }
                                            draw_bg.color: #eef3f7
                                            draw_bg.color_hover: #dcebf0
                                            draw_bg.color_down: #c9e8ed
                                            draw_text.color: #x213246
                                            draw_text.text_style.font_size: 20
                                        }
                                    }

                                    layout_settings_back := AppButton {
                                        width: 42
                                        height: 42
                                        visible: state.current_screen == "layout_settings"
                                        text: "←"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 18
                                    }
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
                                    visible: state.current_screen != "layout_surface" && state.current_screen != "layout_settings"
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
                                    visible: state.current_screen != "layouts" && state.current_screen != "layout_surface" && state.current_screen != "layout_settings"
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
                                    visible: state.current_screen == "layouts"
                                    flow: Down
                                    spacing: 18
                                    padding: 22
                                    show_bg: true
                                    draw_bg +: { color: #xffffffff radius: 12.0 }

                                    layouts_heading := View {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 12
                                        align: Align{ y: 0.5 }

                                        layouts_heading_text := Label {
                                            width: Fill
                                            text: "Your layouts"
                                            draw_text.color: #x18263a
                                            draw_text.text_style.font_size: 20
                                        }

                                        layout_new := AppButton {
                                            width: 142
                                            height: 40
                                            text: "+  New layout"
                                            draw_bg.color: #x00bfd0
                                            draw_bg.color_hover: #x20cddd
                                            draw_bg.color_down: #x44c7d5
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                    }

                                    layout_create_form := View {
                                        width: Fill
                                        height: Fit
                                        visible: false
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

                                        layout_create_type := View {
                                            width: Fill
                                            height: Fit
                                            flow: Right
                                            spacing: 8

                                            layout_create_keyboard := AppButton {
                                                width: Fill
                                                height: 40
                                                text: "Full keyboard"
                                                draw_bg.color: #eef3f7
                                                draw_bg.color_hover: #x7bd9e3
                                                draw_bg.color_down: #x44c7d5
                                                draw_text.color: #x182b3c
                                                draw_text.text_style.font_size: 12
                                            }

                                            layout_create_touchpad := AppButton {
                                                width: Fill
                                                height: 40
                                                text: "Touchpad"
                                                draw_bg.color: #eef3f7
                                                draw_bg.color_hover: #dcebf0
                                                draw_bg.color_down: #c9e8ed
                                                draw_text.color: #x213246
                                                draw_text.text_style.font_size: 12
                                            }
                                            layout_create_gamepad := AppButton {
                                                width: Fill
                                                height: 40
                                                text: "Gamepad"
                                                draw_bg.color: #eef3f7
                                                draw_bg.color_hover: #dcebf0
                                                draw_bg.color_down: #c9e8ed
                                                draw_text.color: #x213246
                                                draw_text.text_style.font_size: 12
                                            }
                                            layout_create_custom := AppButton {
                                                width: Fill height: 40 text: "Custom controls"
                                                draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0
                                                draw_bg.color_down: #x9ee5ed draw_text.color: #x213246
                                                draw_text.text_style.font_size: 12
                                            }
                                        }

                                        layout_create_actions := View {
                                            width: Fill
                                            height: Fit
                                            flow: Right
                                            spacing: 8

                                            layout_save := AppButton {
                                                width: 104
                                                height: 38
                                                text: "Save layout"
                                                draw_bg.color: #x00bfd0
                                                draw_bg.color_hover: #x20cddd
                                                draw_bg.color_down: #x44c7d5
                                                draw_text.color: #x182b3c
                                                draw_text.text_style.font_size: 12
                                            }

                                            layout_cancel := AppButton {
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
                                            text: "Choose a name and a control surface."
                                            draw_text.color: #x6b7c8c
                                            draw_text.text_style.font_size: 12
                                        }
                                    }

                                    layout_choices := View {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 10

                                        layout_keyboard_select := AppButton {
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

                                        layout_touchpad_select := AppButton {
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

                                    layout_custom_1 := AppButton {
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

                                    layout_gamepad_select := AppButton {
                                        width: Fill
                                        height: 56
                                        text: "  Gamepad"
                                        draw_bg.color: #eef3f7
                                        draw_bg.color_hover: #dcebf0
                                        draw_bg.color_down: #c9e8ed
                                        draw_text.color: #x213246
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_2 := AppButton {
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
                                    layout_custom_3 := AppButton {
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
                                    layout_custom_4 := AppButton {
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

                                        mouse_left_button := AppButton {
                                            width: Fill
                                            height: 52
                                            text: "Left click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                        mouse_middle_button := AppButton {
                                            width: Fill
                                            height: 52
                                            text: "Middle click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                        mouse_right_button := AppButton {
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
                                            key_esc := AppButton { width: Fill height: 34 text: "Esc" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f1 := AppButton { width: Fill height: 34 text: "F1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f2 := AppButton { width: Fill height: 34 text: "F2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f3 := AppButton { width: Fill height: 34 text: "F3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f4 := AppButton { width: Fill height: 34 text: "F4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f5 := AppButton { width: Fill height: 34 text: "F5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f6 := AppButton { width: Fill height: 34 text: "F6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f7 := AppButton { width: Fill height: 34 text: "F7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f8 := AppButton { width: Fill height: 34 text: "F8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f9 := AppButton { width: Fill height: 34 text: "F9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f10 := AppButton { width: Fill height: 34 text: "F10" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f11 := AppButton { width: Fill height: 34 text: "F11" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            key_f12 := AppButton { width: Fill height: 34 text: "F12" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_number_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_num1 := AppButton { width: Fill height: 40 text: "`" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num2 := AppButton { width: Fill height: 40 text: "1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num3 := AppButton { width: Fill height: 40 text: "2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num4 := AppButton { width: Fill height: 40 text: "3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num5 := AppButton { width: Fill height: 40 text: "4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num6 := AppButton { width: Fill height: 40 text: "5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num7 := AppButton { width: Fill height: 40 text: "6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num8 := AppButton { width: Fill height: 40 text: "7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num9 := AppButton { width: Fill height: 40 text: "8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num10 := AppButton { width: Fill height: 40 text: "9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num11 := AppButton { width: Fill height: 40 text: "0" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num12 := AppButton { width: Fill height: 40 text: "-" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_num13 := AppButton { width: Fill height: 40 text: "=" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_backspace := AppButton { width: Fill height: 40 text: "⌫" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 11 }
                                        }

                                        keyboard_qwerty_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_tab := AppButton { width: Fill height: 40 text: "Tab" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_q := AppButton { width: Fill height: 40 text: "Q" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_w := AppButton { width: Fill height: 40 text: "W" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_e := AppButton { width: Fill height: 40 text: "E" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_r := AppButton { width: Fill height: 40 text: "R" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_t := AppButton { width: Fill height: 40 text: "T" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_y := AppButton { width: Fill height: 40 text: "Y" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_u := AppButton { width: Fill height: 40 text: "U" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_i := AppButton { width: Fill height: 40 text: "I" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_o := AppButton { width: Fill height: 40 text: "O" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_p := AppButton { width: Fill height: 40 text: "P" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_bracket := AppButton { width: Fill height: 40 text: "[" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_bracket2 := AppButton { width: Fill height: 40 text: "]" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_enter1 := AppButton { width: Fill height: 40 text: "Enter" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_home_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_caps := AppButton { width: Fill height: 40 text: "Caps" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_a := AppButton { width: Fill height: 40 text: "A" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_s := AppButton { width: Fill height: 40 text: "S" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_d := AppButton { width: Fill height: 40 text: "D" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_f := AppButton { width: Fill height: 40 text: "F" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_g := AppButton { width: Fill height: 40 text: "G" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_h := AppButton { width: Fill height: 40 text: "H" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_j := AppButton { width: Fill height: 40 text: "J" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_k := AppButton { width: Fill height: 40 text: "K" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_l := AppButton { width: Fill height: 40 text: "L" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_semicolon := AppButton { width: Fill height: 40 text: ";" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_quote := AppButton { width: Fill height: 40 text: "'" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_enter2 := AppButton { width: Fill height: 40 text: "↵" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 12 }
                                        }

                                        keyboard_bottom_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_shift := AppButton { width: Fill height: 40 text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_z := AppButton { width: Fill height: 40 text: "Z" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_x := AppButton { width: Fill height: 40 text: "X" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_c := AppButton { width: Fill height: 40 text: "C" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_v := AppButton { width: Fill height: 40 text: "V" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_b := AppButton { width: Fill height: 40 text: "B" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_n := AppButton { width: Fill height: 40 text: "N" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_m := AppButton { width: Fill height: 40 text: "M" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_comma := AppButton { width: Fill height: 40 text: "," draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_period := AppButton { width: Fill height: 40 text: "." draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_slash := AppButton { width: Fill height: 40 text: "/" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            key_shift2 := AppButton { width: Fill height: 40 text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_modifier_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_ctrl := AppButton { width: Fill height: 40 text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_fn := AppButton { width: Fill height: 40 text: "Fn" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_alt := AppButton { width: Fill height: 40 text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_space := AppButton { width: Fill height: 40 text: "Space" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            key_alt2 := AppButton { width: Fill height: 40 text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_menu := AppButton { width: Fill height: 40 text: "Menu" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_ctrl2 := AppButton { width: Fill height: 40 text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            key_up := AppButton { width: Fill height: 40 text: "↑" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 13 }
                                            key_down := AppButton { width: Fill height: 40 text: "↓" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 13 }
                                        }
                                    }
                                }

                                layout_surface_panel := View {
                                    width: Fill
                                    height: if state.current_screen == "layout_surface" { Fill } else { Fit }
                                    visible: state.current_screen == "layout_surface"
                                    flow: Down
                                    spacing: 8

                                    touchpad_surface := RoundedView {
                                        width: Fill
                                        height: Fill
                                        visible: state.selected_layout_kind == "touchpad"
                                        flow: Right
                                        spacing: 6
                                        padding: 8
                                        show_bg: true
                                        draw_bg +: {
                                            color: #xe6f7f9
                                            radius: 14.0
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
                                        height: 52
                                        visible: state.selected_layout_kind == "touchpad"
                                        flow: Right
                                        spacing: 8

                                        surface_left_click := AppButton {
                                            width: Fill
                                            height: 52
                                            text: "Left click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                        surface_middle_click := AppButton {
                                            width: Fill
                                            height: 52
                                            text: "Middle click"
                                            draw_bg.color: #b7eaf0
                                            draw_bg.color_hover: #x90dce5
                                            draw_bg.color_down: #x63cbd6
                                            draw_text.color: #x182b3c
                                            draw_text.text_style.font_size: 13
                                        }
                                        surface_right_click := AppButton {
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
                                            gamepad_lb := AppButton { width: Fill height: 52 text: "LB" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                            gamepad_lt := AppButton { width: Fill height: 52 text: "LT" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                            gamepad_rb := AppButton { width: Fill height: 52 text: "RB" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                            gamepad_rt := AppButton { width: Fill height: 52 text: "RT" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 15 }
                                        }

                                        gamepad_controls := View {
                                            width: Fill height: Fill flow: Right spacing: 10 align: Align{ y: 0.5 }
                                            gamepad_dpad := View {
                                                width: Fill height: 200 flow: Down spacing: 6 align: Align{ x: 0.5, y: 0.5 }
                                                gamepad_dpad_up := AppButton { width: 66 height: 56 text: "↑" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                                gamepad_dpad_horizontal := View {
                                                    width: 204 height: 56 flow: Right spacing: 6
                                                    gamepad_dpad_left := AppButton { width: 66 height: 56 text: "←" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                                    View { width: Fill height: Fill }
                                                    gamepad_dpad_right := AppButton { width: 66 height: 56 text: "→" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                                }
                                                gamepad_dpad_down := AppButton { width: 66 height: 56 text: "↓" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 20 }
                                            }
                                            gamepad_center_controls := View {
                                                width: Fill height: 200 flow: Down spacing: 10 align: Align{ y: 0.5 }
                                                gamepad_menu_row := View {
                                                    width: Fill height: 48 flow: Right spacing: 8
                                                    gamepad_select := AppButton { width: Fill height: 48 text: "Select" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                                    gamepad_home := AppButton { width: Fill height: 48 text: "Home" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                                    gamepad_start := AppButton { width: Fill height: 48 text: "Start" draw_bg.color: #eef3f7 draw_bg.color_hover: #dcebf0 draw_bg.color_down: #x9ee5ed draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                                }
                                                gamepad_face_row := View {
                                                    width: Fill height: Fill flow: Down spacing: 4
                                                    gamepad_face_top := View {
                                                        width: Fill height: Fill flow: Right
                                                        View { width: Fill height: Fill }
                                                        gamepad_y := AppButton { width: 48 height: Fill text: "Y" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: Fill height: Fill }
                                                    }
                                                    gamepad_face_middle := View {
                                                        width: Fill height: Fill flow: Right
                                                        View { width: Fill height: Fill }
                                                        gamepad_x := AppButton { width: 48 height: Fill text: "X" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: 48 height: Fill }
                                                        gamepad_b := AppButton { width: 48 height: Fill text: "B" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: Fill height: Fill }
                                                    }
                                                    gamepad_face_bottom := View {
                                                        width: Fill height: Fill flow: Right
                                                        View { width: Fill height: Fill }
                                                        gamepad_a := AppButton { width: 48 height: Fill text: "A" draw_bg.border_radius: 24 draw_bg.color: #dff5f7 draw_bg.color_hover: #b8eaf0 draw_bg.color_down: #x8edce4 draw_text.color: #x008e9f draw_text.text_style.font_size: 16 }
                                                        View { width: Fill height: Fill }
                                                    }
                                                }
                                            }
                                        }

                                        gamepad_status := Label { width: Fill height: 24 text: "Tap a control to send a Bluetooth gamepad press." draw_text.color: #x6b7c8c draw_text.text_style.font_size: 12 }
                                    }

                                    layout_surface_custom := View {
                                        width: Fill height: Fill visible: state.selected_layout_kind == "custom" flow: Down spacing: 12
                                        padding: 16 show_bg: true draw_bg +: { color: #xf5f8fa }
                                        custom_surface_hint := Label { width: Fill height: Fit text: "YOUR CONTROLS   ·   Hold a button to send input" draw_text.color: #x27818c draw_text.text_style.font_size: 11 }
                                        custom_surface_row_1 := View {
                                            width: Fill height: Fill flow: Right spacing: 12
                                            custom_surface_1 := AppButton { width: Fill height: Fill text: "" draw_bg.color: #e8f7f8 draw_bg.color_hover: #caeff2 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 18 }
                                            custom_surface_2 := AppButton { width: Fill height: Fill text: "" draw_bg.color: #eef4f7 draw_bg.color_hover: #d6eef1 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 18 }
                                            custom_surface_3 := AppButton { width: Fill height: Fill text: "" draw_bg.color: #e8f7f8 draw_bg.color_hover: #caeff2 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 18 }
                                        }
                                        custom_surface_row_2 := View {
                                            width: Fill height: Fill flow: Right spacing: 12
                                            custom_surface_4 := AppButton { width: Fill height: Fill text: "" draw_bg.color: #eef4f7 draw_bg.color_hover: #d6eef1 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 18 }
                                            custom_surface_5 := AppButton { width: Fill height: Fill text: "" draw_bg.color: #e8f7f8 draw_bg.color_hover: #caeff2 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 18 }
                                            custom_surface_6 := AppButton { width: Fill height: Fill text: "" draw_bg.color: #eef4f7 draw_bg.color_hover: #d6eef1 draw_bg.color_down: #a0dfe5 draw_text.color: #x182b3c draw_text.text_style.font_size: 18 }
                                        }
                                    }

                                    layout_surface_keyboard := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.selected_layout_kind == "keyboard"
                                        flow: Down
                                        spacing: if state.is_narrow { 3 } else { 6 }
                                        padding: if state.is_narrow { 5 } else { 10 }
                                        show_bg: true
                                        draw_bg +: { color: #xf0f5f8 }

                                        landscape_keyboard_toolbar := View {
                                            width: Fill
                                            height: 42
                                            visible: state.is_compact && !state.is_narrow
                                            flow: Right
                                            spacing: 8
                                            align: Align{ y: 0.5 }

                                            landscape_menu_toggle := AppButton {
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

                                            landscape_keyboard_toggle := AppButton {
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

                                            landscape_settings_button := AppButton {
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
                                            height: if state.is_narrow { 26 } else { 34 }
                                            visible: !state.is_narrow
                                            flow: Right
                                            spacing: if state.is_narrow { 2 } else { 4 }
                                            sk_esc := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "Esc" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f1 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f2 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f3 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f4 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f5 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f6 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f7 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f8 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f9 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f10 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F10" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f11 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F11" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                            sk_f12 := AppButton { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F12" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_number := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sn_1 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "`" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_2 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "1" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_3 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "2" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_4 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "3" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_5 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "4" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_6 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "5" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_7 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "6" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_8 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "7" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_9 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "8" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_10 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "9" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_11 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "0" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            sn_backspace := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "⌫" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 11 }
                                        }

                                        surface_keyboard_symbols := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            ss_minus := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "-" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_equal := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "=" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_open_bracket := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "[" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_close_bracket := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "]" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_backslash := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "\\" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_semicolon := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: ";" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_quote := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "'" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_comma := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "," draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_period := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "." draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                            ss_slash := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "/" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 11 }
                                        }

                                        surface_keyboard_top := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            st_tab := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Tab" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            st_q := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Q" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_w := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "W" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_e := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "E" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_r := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "R" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_t := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "T" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_y := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Y" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_u := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "U" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_i := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "I" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_o := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "O" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_p := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "P" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            st_enter := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Enter" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_home := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sh_caps := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Caps" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sh_a := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "A" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_s := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "S" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_d := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "D" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_f := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "F" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_g := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "G" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_h := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "H" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_j := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "J" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_k := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "K" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_l := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "L" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sh_enter := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Ent" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 12 }
                                        }

                                        surface_keyboard_bottom := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sb_shift := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sb_z := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Z" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_x := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "X" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_c := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "C" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_v := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "V" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_b := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "B" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_n := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "N" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_m := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "M" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sb_shift2 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Shift" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_modifiers := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sm_ctrl := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_fn := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Fn" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_alt := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_space := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Space" draw_bg.color: #eef3f7 draw_text.color: #x213246 draw_text.text_style.font_size: 12 }
                                            sm_alt2 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Alt" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
                                            sm_ctrl2 := AppButton { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Ctrl" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c draw_text.text_style.font_size: 10 }
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
                                        text: "Touchpad settings"
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
                                                    custom_add := AppButton { width: Fill height: 32 text: "+ Add button" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                    custom_delete := AppButton { width: Fill height: 32 text: "Remove" draw_bg.color: #fce8e8 draw_text.color: #x182b3c }
                                                }
                                                custom_editor_order := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    custom_move_left := AppButton { width: Fill height: 32 text: "Move left" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                    custom_move_right := AppButton { width: Fill height: 32 text: "Move right" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                }
                                            }
                                            custom_editor_details := View {
                                                width: Fill height: Fit flow: Down spacing: 6
                                                custom_editor_name := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    custom_label_input := AppTextInput { width: Fill height: 32 empty_text: "Button label" }
                                                    custom_label_save := AppButton { width: 90 height: 32 text: "Set label" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                }
                                                custom_editor_groups := View {
                                                    width: Fill height: 32 flow: Right spacing: 4
                                                    custom_group_keyboard := AppButton { width: Fill height: 32 text: "Keys" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a }
                                                    custom_group_volume := AppButton { width: Fill height: 32 text: "Volume" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a }
                                                    custom_group_gamepad := AppButton { width: Fill height: 32 text: "Pad" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a }
                                                    custom_group_mouse := AppButton { width: Fill height: 32 text: "Mouse" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a }
                                                    custom_group_bluetooth := AppButton { width: Fill height: 32 text: "BT" draw_bg.color: #e5f3f5 draw_text.color: #x164e5a }
                                                }
                                                custom_editor_action := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    custom_action_previous := AppButton { width: 44 height: 32 text: "‹" draw_bg.color: #eef3f7 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                                    custom_action_label := Label { width: Fill height: 32 text: "Output" align: Align{ x: 0.5, y: 0.5 } draw_text.color: #x164e5a draw_text.text_style.font_size: 14 }
                                                    custom_action_next := AppButton { width: 44 height: 32 text: "›" draw_bg.color: #eef3f7 draw_text.color: #x182b3c draw_text.text_style.font_size: 16 }
                                                }
                                                custom_editor_hardware := View {
                                                    width: Fill height: 32 flow: Right spacing: 6
                                                    custom_learn := AppButton { width: Fill height: 32 text: "Assign hardware button" draw_bg.color: #x9ee5ed draw_text.color: #x182b3c }
                                                    custom_unbind := AppButton { width: 70 height: 32 text: "Clear" draw_bg.color: #eef3f7 draw_text.color: #x213246 }
                                                }
                                                custom_editor_status := Label { width: Fill height: Fit text: "Changes are saved on this device." draw_text.color: #x66798b draw_text.text_style.font_size: 12 }
                                            }
                                        }
                                    }
                                }
                                }
                                bottom_navigation := View {
                                    width: Fill height: 42 flow: Right spacing: 8
                                    visible: state.current_screen != "layout_surface" && state.current_screen != "layout_settings"
                                    bottom_devices := AppButton { width: Fill height: 42 text: "Devices" icon_walk: Walk{ width: 16 height: 16 } draw_icon +: { color: #x00a7b8 svg: crate_resource("self:resources/bluetooth.svg") } draw_bg.color: if state.current_screen == "devices" { #c9f2f5 } else { #eef3f7 } draw_text.color: #x164e5a }
                                    bottom_layouts := AppButton { width: Fill height: 42 text: "Layouts" icon_walk: Walk{ width: 16 height: 16 } draw_icon +: { color: #x00a7b8 svg: crate_resource("self:resources/touchpad.svg") } draw_bg.color: if state.current_screen == "layouts" { #c9f2f5 } else { #eef3f7 } draw_text.color: #x164e5a }
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
                                    height: 112
                                    flow: Right
                                    spacing: 12
                                    padding: Inset{ left: 20 right: 12 }
                                    align: Align{ y: 0.5 }
                                    show_bg: true
                                    draw_bg +: { color: #x164e5a }

                                    sidebar_brand := View {
                                        width: Fill
                                        height: Fit
                                        flow: Down
                                        spacing: 4
                                        app_brand := Label {
                                            width: Fill
                                            text: "OpenWASD"
                                            draw_text.color: #xffffffff
                                            draw_text.text_style.font_size: 21
                                        }
                                        sidebar_tagline := Label {
                                            width: Fill
                                            text: "Input workspace"
                                            draw_text.color: #xd2eef0
                                            draw_text.text_style.font_size: 12
                                        }
                                    }

                                    sidebar_toggle := AppButton {
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
    create_layout_kind: LayoutKind,
    #[rust]
    create_layout_open: bool,
    #[rust]
    layout_status: String,
    #[rust]
    custom_selected_control: usize,
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
        self.create_layout_kind = LayoutKind::Touchpad;
        self.create_layout_open = false;
        self.layout_status = "Choose a name and a control surface.".into();
        self.custom_selected_control = 0;
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
            self.layout_status = "Choose a name and a control surface.".into();
            self.ui
                .text_input(cx, ids!(layout_name_input))
                .set_text(cx, "");
            self.render_layouts(cx);
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
            .button(cx, ids!(layout_create_keyboard))
            .clicked(actions)
        {
            self.create_layout_kind = LayoutKind::Keyboard;
            self.layout_status = "New layout type: Full keyboard.".into();
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(layout_create_touchpad))
            .clicked(actions)
        {
            self.create_layout_kind = LayoutKind::Touchpad;
            self.layout_status = "New layout type: Touchpad.".into();
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(layout_create_gamepad))
            .clicked(actions)
        {
            self.create_layout_kind = LayoutKind::Gamepad;
            self.layout_status = "New layout type: Gamepad.".into();
            self.render_layouts(cx);
        }
        if self
            .ui
            .button(cx, ids!(layout_create_custom))
            .clicked(actions)
        {
            self.create_layout_kind = LayoutKind::Custom;
            self.layout_status =
                "Custom controls: add buttons and map hardware after saving.".into();
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(layout_cancel)).clicked(actions) {
            self.create_layout_open = false;
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
                    kind: self.create_layout_kind,
                    controls: if self.create_layout_kind == LayoutKind::Custom {
                        default_custom_controls()
                    } else {
                        Vec::new()
                    },
                };
                self.saved_layouts.push(layout);
                match persist_saved_layouts(&self.saved_layouts) {
                    Ok(()) => {
                        let index = self.saved_layouts.len() - 1;
                        self.layout_status = format!("Saved {name}.");
                        self.create_layout_open = false;
                        self.select_layout(self.create_layout_kind, Some(index), &name, cx);
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
            self.release_custom_controls();
            self.layout_surface_active = false;
            self.custom_learning = false;
            self.load_custom_label(cx);
            script_eval!(cx, {
                mod.state.current_screen = "layout_settings";
                ui.main_view.render()
            });
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
            self.render_layouts(cx);
        }

        if self
            .ui
            .button(cx, ids!(layout_settings_back))
            .clicked(actions)
        {
            self.layout_surface_active = true;
            self.custom_learning = false;
            #[cfg(target_os = "android")]
            self.request_hid_start();
            script_eval!(cx, {
                mod.state.current_screen = "layout_surface";
                ui.main_view.render()
            });
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
            self.render_layouts(cx);
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
            self.layout_surface_active = false;
            self.hid_start_pending = false;
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            script_eval!(cx, {
                mod.state.current_screen = "dashboard";
                mod.state.sidebar_collapsed = true;
                ui.main_view.render()
            });
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
        }

        if self.ui.button(cx, ids!(nav_devices)).clicked(actions)
            || self.ui.button(cx, ids!(bottom_devices)).clicked(actions)
        {
            self.release_custom_controls();
            self.custom_learning = false;
            self.layout_surface_active = false;
            self.hid_start_pending = false;
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            script_eval!(cx, {
                mod.state.current_screen = "devices";
                mod.state.sidebar_collapsed = true;
                ui.main_view.render()
            });
            self.ui
                .view(cx, ids!(scroll_body))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
            self.render_usb_devices(cx);
            self.render_bluetooth_devices(cx);
        }

        if self.ui.button(cx, ids!(nav_layouts)).clicked(actions)
            || self.ui.button(cx, ids!(bottom_layouts)).clicked(actions)
        {
            self.release_custom_controls();
            self.custom_learning = false;
            self.layout_surface_active = false;
            self.hid_start_pending = false;
            self.device_detail_open = false;
            self.device_diagnostics_open = false;
            self.ui
                .widget(cx, ids!(device_detail_panel))
                .set_visible(cx, false);
            script_eval!(cx, {
                mod.state.current_screen = "layouts";
                mod.state.sidebar_collapsed = true;
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
            let is_compact = window_width <= 1000;
            let is_narrow = window_width <= 640;
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
                    is_compact: #(is_compact),
                    is_narrow: #(is_narrow),
                    current_screen: #(initial_screen),
                    selected_layout_kind: #(initial_layout_kind),
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
                    self.request_hid_start();
                }
                self.render_layouts(cx);
                if self.startup_screenshot.is_some() {
                    self.startup_screenshot_timer = Some(cx.start_timeout(0.25));
                }
            }
            #[cfg(target_os = "android")]
            Event::Resume if self.layout_surface_active => self.request_hid_start(),
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

    fn load_custom_label(&self, cx: &mut Cx) {
        let label = self
            .selected_custom_control(self.custom_selected_control)
            .map(|control| control.label)
            .unwrap_or_default();
        self.ui
            .text_input(cx, ids!(custom_label_input))
            .set_text(cx, &label);
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
        let action = self
            .selected_custom_layout
            .and_then(|index| self.saved_layouts.get(index))
            .and_then(|layout| {
                layout
                    .controls
                    .iter()
                    .find(|control| control.hardware_key.as_deref() == Some(&name))
            })
            .map(|control| control.action);
        if let Some(action) = action {
            let source = format!("hardware:{name}");
            if down {
                self.press_custom_control(source, action);
            } else {
                self.release_custom_control(&source, action);
            }
        }
    }

    fn handle_custom_actions(&mut self, cx: &mut Cx, actions: &Actions) {
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
                    let source = format!("touch:{index}");
                    if control.action == ControlAction::BluetoothSettings {
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
                self.custom_learning = false;
                self.load_custom_label(cx);
                self.render_layouts(cx);
            }
        }
        if self.ui.button(cx, ids!(custom_add)).clicked(actions) {
            if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
                if layout.controls.len() < CUSTOM_LIMIT {
                    layout.controls.push(CustomControl {
                        label: format!("Button {}", layout.controls.len() + 1),
                        action: ControlAction::Keyboard(44),
                        hardware_key: None,
                    });
                    self.custom_selected_control = layout.controls.len() - 1;
                    self.save_custom_changes();
                    self.load_custom_label(cx);
                } else {
                    self.custom_status = "This layout can contain up to six buttons.".into();
                }
            }
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(custom_delete)).clicked(actions) {
            if let Some(layout) = self.saved_layouts.get_mut(layout_index) {
                if self.custom_selected_control < layout.controls.len() {
                    layout.controls.remove(self.custom_selected_control);
                    self.custom_selected_control = self
                        .custom_selected_control
                        .min(layout.controls.len().saturating_sub(1));
                    self.custom_learning = false;
                    self.save_custom_changes();
                    self.load_custom_label(cx);
                }
            }
            self.render_layouts(cx);
        }
        let move_left = self.ui.button(cx, ids!(custom_move_left)).clicked(actions);
        let move_right = self.ui.button(cx, ids!(custom_move_right)).clicked(actions);
        if move_left || move_right {
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
                }
            }
            self.render_layouts(cx);
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
        for (id, action) in [
            (live_id!(custom_group_keyboard), ControlAction::Keyboard(44)),
            (live_id!(custom_group_volume), ControlAction::Keyboard(128)),
            (
                live_id!(custom_group_gamepad),
                ControlAction::GamepadButton(0),
            ),
            (live_id!(custom_group_mouse), ControlAction::MouseButton(1)),
            (
                live_id!(custom_group_bluetooth),
                ControlAction::BluetoothSettings,
            ),
        ] {
            if self.ui.button(cx, &[id]).clicked(actions) {
                if let Some(control) = self
                    .saved_layouts
                    .get_mut(layout_index)
                    .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
                {
                    control.action = action;
                    self.save_custom_changes();
                }
                self.render_layouts(cx);
            }
        }
        let previous = self
            .ui
            .button(cx, ids!(custom_action_previous))
            .clicked(actions);
        let next = self
            .ui
            .button(cx, ids!(custom_action_next))
            .clicked(actions);
        if previous || next {
            if let Some(control) = self
                .saved_layouts
                .get_mut(layout_index)
                .and_then(|layout| layout.controls.get_mut(self.custom_selected_control))
            {
                let group = custom_output_group(control.action);
                let choices: Vec<ControlAction> = CUSTOM_OUTPUTS
                    .iter()
                    .map(|(_, action)| *action)
                    .filter(|action| custom_output_group(*action) == group)
                    .collect();
                let current = choices
                    .iter()
                    .position(|action| *action == control.action)
                    .unwrap_or(0);
                let target = if previous {
                    (current + choices.len() - 1) % choices.len()
                } else {
                    (current + 1) % choices.len()
                };
                control.action = choices[target];
                self.save_custom_changes();
            }
            self.render_layouts(cx);
        }
        if self.ui.button(cx, ids!(custom_learn)).clicked(actions)
            && self
                .selected_custom_control(self.custom_selected_control)
                .is_some()
        {
            self.custom_learning = true;
            self.custom_status = "Press the hardware button you want to assign.".into();
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
            mod.state.sidebar_collapsed = true;
            ui.main_view.render()
        });
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
        self.ui
            .widget(cx, ids!(custom_surface_row_2))
            .set_visible(cx, is_custom && control_count > 3);
        self.ui
            .widget(cx, ids!(custom_editor_row_2))
            .set_visible(cx, is_custom && control_count > 3);
        self.ui.view(cx, ids!(touchpad_surface)).set_walk(
            cx,
            Walk::new(
                Size::fill(),
                if is_touchpad {
                    Size::fill()
                } else {
                    Size::Fixed(0.0)
                },
            ),
        );
        self.ui
            .view(cx, ids!(layout_surface_mouse_buttons))
            .set_walk(
                cx,
                Walk::new(
                    Size::fill(),
                    Size::Fixed(if is_touchpad { 52.0 } else { 0.0 }),
                ),
            );
        let (window_width, _) = initial_window_size();
        let is_narrow_window = window_width <= 640;
        self.ui.view(cx, ids!(surface_keyboard_function)).set_walk(
            cx,
            Walk::new(
                Size::fill(),
                Size::Fixed(if is_narrow_window { 0.0 } else { 34.0 }),
            ),
        );
        self.ui
            .widget(cx, ids!(touchpad_surface))
            .set_visible(cx, is_touchpad);
        self.ui
            .widget(cx, ids!(layout_surface_mouse_buttons))
            .set_visible(cx, is_touchpad);
        self.ui
            .widget(cx, ids!(layout_surface_keyboard))
            .set_visible(cx, is_keyboard);
        self.ui
            .widget(cx, ids!(layout_surface_gamepad))
            .set_visible(cx, self.selected_layout_kind == LayoutKind::Gamepad);
        self.ui
            .widget(cx, ids!(layout_surface_custom))
            .set_visible(cx, is_custom);
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
        self.ui
            .label(cx, ids!(layout_surface_title))
            .set_text(cx, preview_name);
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
            .set_text(cx, &device_status);
        self.ui.label(cx, ids!(gamepad_status)).set_text(
            cx,
            if usb_gamepad {
                "USB gamepad connected. Tap a control to send a press."
            } else if self.hid_host.gamepad_ready() {
                "Bluetooth gamepad connected. Tap a control to send a press."
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
        for (kind, id, title) in [
            (
                LayoutKind::Keyboard,
                live_id!(layout_create_keyboard),
                "Full keyboard",
            ),
            (
                LayoutKind::Touchpad,
                live_id!(layout_create_touchpad),
                "Touchpad",
            ),
            (
                LayoutKind::Gamepad,
                live_id!(layout_create_gamepad),
                "Gamepad",
            ),
            (
                LayoutKind::Custom,
                live_id!(layout_create_custom),
                "Custom controls",
            ),
        ] {
            self.ui.button(cx, &[id]).set_text(
                cx,
                &format!(
                    "{}{}",
                    if self.create_layout_kind == kind {
                        "✓ "
                    } else {
                        ""
                    },
                    title
                ),
            );
        }
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
                self.ui
                    .button(cx, &[surface_id])
                    .set_text(cx, &control.label);
                self.ui.button(cx, &[edit_id]).set_text(
                    cx,
                    &format!(
                        "{}{}",
                        if index == self.custom_selected_control {
                            "● "
                        } else {
                            ""
                        },
                        control.label
                    ),
                );
            }
        }
        let selected_control = self.selected_custom_control(self.custom_selected_control);
        self.ui.label(cx, ids!(custom_action_label)).set_text(
            cx,
            selected_control
                .as_ref()
                .map(|control| custom_output_label(control.action))
                .unwrap_or("Add a button to choose an action"),
        );
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
                self.ui.widget(cx, &[id]).set_visible(cx, true);
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
