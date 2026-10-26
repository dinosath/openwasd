pub use makepad_widgets;

use makepad_widgets::*;
use openwasd_bluetooth::{
    BluetoothCommand, BluetoothDeviceInfo, BluetoothEvent, BluetoothService, BtleplugAdapter,
    ConnectionState, HidHostState, merge_devices,
};
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
    #[default]
    Touchpad,
}

impl LayoutKind {
    fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "Full keyboard",
            Self::Gamepad => "Gamepad",
            Self::Touchpad => "Touchpad",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SavedLayout {
    name: String,
    kind: LayoutKind,
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

    startup() do #(App::script_component(vm)) {
        ui: Root {
            on_startup: || {
                ui.main_view.render()
            }

            main_window := Window {
                window.inner_size: vec2(state.window_width, state.window_height)
                pass +: { clear_color: #x17191f }

                body +: {
                    main_view := View {
                        width: Fill
                        height: Fill
                        flow: Overlay
                        show_bg: true
                        draw_bg +: { color: #x17191f }

                        on_render: || {
                            content := View {
                                width: Fill
                                height: Fill
                                flow: Down
                                spacing: 22
                                margin: Inset{ left: 0 }
                                padding: if state.is_compact { 18 } else { 30 }
                                show_bg: true
                                draw_bg +: { color: #x17191f }

                                page_header := View {
                                    width: Fill
                                    height: Fit
                                    visible: !(state.current_screen == "layout_surface" && state.selected_layout_kind == "keyboard" && state.is_compact && !state.is_narrow)
                                    flow: Right
                                    spacing: if state.is_narrow { 8 } else { 14 }
                                    align: Align{ y: 0.5 }

                                    mobile_menu_toggle := Button {
                                        width: 42
                                        height: 42
                                        visible: true
                                        text: "☰"
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 17
                                    }

                                    page_title := Label {
                                        width: Fit
                                        visible: state.current_screen != "layout_surface"
                                        text: if state.current_screen == "devices" { "Devices" } else if state.current_screen == "layouts" { "Layouts" } else if state.current_screen == "layout_surface" { if state.selected_layout_kind == "gamepad" { "Gamepad" } else { "Mouse / Keyboard" } } else if state.current_screen == "layout_settings" { "Layout settings" } else { "Dashboard" }
                                        draw_text.color: #f4f5f8
                                        draw_text.text_style.font_size: if state.is_narrow { 20 } else { 28 }
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
                                            draw_text.color: #f4f5f8
                                            draw_text.text_style.font_size: if state.is_narrow { 19 } else { 25 }
                                        }

                                        layout_surface_device_status := Label {
                                            width: Fill
                                            text: "Waiting for a computer to connect"
                                            draw_text.color: #9ba2b1
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

                                        wifi_connect := Button {
                                            width: if state.is_narrow { 40 } else { 82 }
                                            height: 40
                                            enabled: false
                                            text: if state.is_narrow { "" } else { "Wi-Fi" }
                                            icon_walk: Walk{ width: 16, height: 16 }
                                            draw_icon +: {
                                                color: #8f9aaa
                                                svg: crate_resource("self:resources/wifi.svg")
                                            }
                                            draw_bg.color: #252a33
                                            draw_bg.color_disabled: #252a33
                                            draw_text.color: #a3adbc
                                            draw_text.color_disabled: #a3adbc
                                            animator +: { disabled: { default: @on } }
                                        }

                                        bluetooth_connect := Button {
                                            width: if state.is_narrow { 40 } else { 104 }
                                            height: 40
                                            text: if state.is_narrow { "" } else { "Bluetooth" }
                                            icon_walk: Walk{ width: 16, height: 16 }
                                            draw_icon +: {
                                                color: #8f9aaa
                                                svg: crate_resource("self:resources/bluetooth.svg")
                                            }
                                            draw_bg.color: #252a33
                                            draw_bg.color_hover: #303746
                                            draw_bg.color_down: #3b4659
                                            draw_text.color: #e5e7ed
                                        }

                                        usb_connect := Button {
                                            width: if state.is_narrow { 40 } else { 72 }
                                            height: 40
                                            enabled: false
                                            text: if state.is_narrow { "" } else { "USB" }
                                            icon_walk: Walk{ width: 16, height: 16 }
                                            draw_icon +: {
                                                color: #8f9aaa
                                                svg: crate_resource("self:resources/usb.svg")
                                            }
                                            draw_bg.color: #252a33
                                            draw_bg.color_disabled: #252a33
                                            draw_text.color: #a3adbc
                                            draw_text.color_disabled: #a3adbc
                                            animator +: { disabled: { default: @on } }
                                        }
                                    }

                                    layout_toolbar := View {
                                        width: Fit
                                        height: Fit
                                        visible: state.current_screen == "layout_surface"
                                        flow: Right
                                        spacing: 8

                                        layout_keyboard_toggle := Button {
                                            width: 42
                                            height: 42
                                            text: ""
                                            icon_walk: Walk{ width: 20, height: 20 }
                                            draw_icon +: {
                                                color: #a8d8ef
                                                svg: crate_resource("self:resources/keyboard.svg")
                                            }
                                            draw_bg.color: #292d37
                                            draw_bg.color_hover: #363c49
                                            draw_bg.color_down: #414959
                                            draw_text.color: #e5e7ed
                                            draw_text.text_style.font_size: 20
                                        }

                                        layout_settings_button := Button {
                                            width: 42
                                            height: 42
                                            text: ""
                                            icon_walk: Walk{ width: 20, height: 20 }
                                            draw_icon +: {
                                                color: #a8d8ef
                                                svg: crate_resource("self:resources/settings.svg")
                                            }
                                            draw_bg.color: #292d37
                                            draw_bg.color_hover: #363c49
                                            draw_bg.color_down: #414959
                                            draw_text.color: #e5e7ed
                                            draw_text.text_style.font_size: 20
                                        }
                                    }

                                    layout_settings_back := Button {
                                        width: 42
                                        height: 42
                                        visible: state.current_screen == "layout_settings"
                                        text: "←"
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 18
                                    }
                                }

                                scroll_body := ScrollYView {
                                    width: Fill
                                    height: Fill
                                    scroll_bars: ScrollBars {
                                        show_scroll_x: false
                                        show_scroll_y: state.current_screen != "layout_surface"
                                    }
                                    flow: Down
                                    spacing: 22
                                    padding: 0
                                    show_bg: true
                                    draw_bg +: { color: #x17191f }

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
                                    draw_text.color: #9ba2b1
                                    draw_text.text_style.font_size: 14
                                }

                                device_panel := View {
                                    width: Fill
                                    height: Fit
                                    visible: state.current_screen != "layouts" && state.current_screen != "layout_surface" && state.current_screen != "layout_settings"
                                    flow: Down
                                    spacing: 16
                                    padding: 22
                                    show_bg: true
                                    draw_bg +: { color: #x222630 }

                                    panel_title := Label {
                                        width: Fill
                                        text: "Bluetooth devices"
                                        draw_text.color: #f4f5f8
                                        draw_text.text_style.font_size: 19
                                    }

                                    panel_description := Label {
                                        width: Fill
                                        text: "Scan nearby BLE devices. Pair first in system settings if prompted."
                                        draw_text.color: #a0a7b5
                                        draw_text.text_style.font_size: 14
                                    }

                                    refresh_devices := Button {
                                        width: 168
                                        height: 42
                                        text: "Scan Bluetooth"
                                        draw_bg.color: #3567c8
                                        draw_bg.color_hover: #4278df
                                        draw_bg.color_down: #284f9e
                                        draw_text.color: #ffffff
                                        draw_text.text_style.font_size: 14
                                    }

                                    bluetooth_status := Label {
                                        width: Fill
                                        text: "Bluetooth scan has not started."
                                        draw_text.color: #a0a7b5
                                        draw_text.text_style.font_size: 13
                                    }

                                    bluetooth_empty := Label {
                                        width: Fill
                                        text: "No devices listed. Scan to discover nearby devices."
                                        visible: true
                                        draw_text.color: #7f8999
                                        draw_text.text_style.font_size: 13
                                    }

                                    bluetooth_device_1 := Button {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }
                                    bluetooth_device_2 := Button {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }
                                    bluetooth_device_3 := Button {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }
                                    bluetooth_device_4 := Button {
                                        width: Fill
                                        height: 48
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }

                                    selected_bluetooth_device := Label {
                                        width: Fill
                                        text: "Select a device to connect."
                                        draw_text.color: #c2c9d4
                                        draw_text.text_style.font_size: 13
                                    }

                                    connect_device := Button {
                                        width: 160
                                        height: 42
                                        enabled: false
                                        text: "Connect"
                                        draw_bg.color: #3567c8
                                        draw_bg.color_hover: #4278df
                                        draw_bg.color_down: #284f9e
                                        draw_bg.color_disabled: #292d37
                                        draw_text.color: #ffffff
                                        draw_text.color_disabled: #7f8999
                                        animator +: { disabled: { default: @on } }
                                    }

                                    system_bluetooth_settings := Button {
                                        width: 210
                                        height: 38
                                        text: "Bluetooth settings"
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #c2c9d4
                                        draw_text.text_style.font_size: 12
                                    }
                                }

                                device_detail_panel := View {
                                    width: Fill
                                    height: Fit
                                    visible: false
                                    flow: Down
                                    spacing: 16
                                    padding: 22
                                    show_bg: true
                                    draw_bg +: { color: #x222630 }

                                    device_detail_back := Button {
                                        width: 142
                                        height: 38
                                        text: "←  All devices"
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                    }

                                    device_detail_title := Label {
                                        width: Fill
                                        text: "Device settings"
                                        draw_text.color: #f4f5f8
                                        draw_text.text_style.font_size: 22
                                    }

                                    device_detail_connection := Label {
                                        width: Fill
                                        text: "Select a connected device to view its settings."
                                        draw_text.color: #a0a7b5
                                        draw_text.text_style.font_size: 14
                                    }

                                    device_tabs := View {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 8

                                        device_tab_overview := Button {
                                            width: 112
                                            height: 38
                                            text: "Overview"
                                            draw_bg.color: #3159a8
                                            draw_bg.color_hover: #3b68c1
                                            draw_bg.color_down: #284f9e
                                            draw_text.color: #ffffff
                                        }

                                        device_tab_diagnostics := Button {
                                            width: 126
                                            height: 38
                                            text: "Diagnostics"
                                            draw_bg.color: #292d37
                                            draw_bg.color_hover: #363c49
                                            draw_bg.color_down: #414959
                                            draw_text.color: #e5e7ed
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
                                            draw_text.color: #c2c9d4
                                            draw_text.text_style.font_size: 14
                                        }

                                        device_settings_hint := Label {
                                            width: Fill
                                            text: "Use the computer's Bluetooth settings to pair or remove the device."
                                            draw_text.color: #8f9aaa
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
                                            draw_text.color: #f4f5f8
                                            draw_text.text_style.font_size: 18
                                        }

                                        diagnostics_hint := Label {
                                            width: Fill
                                            text: "Move or click with the selected device while this window is focused. Events delivered to OpenWASD appear here."
                                            draw_text.color: #a0a7b5
                                            draw_text.text_style.font_size: 13
                                        }

                                        diagnostics_events := Label {
                                            width: Fill
                                            height: Fit
                                            text: "Waiting for mouse, keyboard, or touch input…"
                                            draw_text.color: #c2e7f6
                                            draw_text.text_style.font_size: 13
                                        }

                                        diagnostics_clear := Button {
                                            width: 126
                                            height: 36
                                            text: "Clear events"
                                            draw_bg.color: #292d37
                                            draw_bg.color_hover: #363c49
                                            draw_bg.color_down: #414959
                                            draw_text.color: #e5e7ed
                                        }
                                    }
                                }

                                layouts_panel := View {
                                    width: Fill
                                    height: Fit
                                    visible: state.current_screen == "layouts"
                                    flow: Down
                                    spacing: 18
                                    padding: 22
                                    show_bg: true
                                    draw_bg +: { color: #x222630 }

                                    layouts_heading := View {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 12
                                        align: Align{ y: 0.5 }

                                        layouts_heading_text := Label {
                                            width: Fill
                                            text: "Your layouts"
                                            draw_text.color: #f4f5f8
                                            draw_text.text_style.font_size: 20
                                        }

                                        layout_new := Button {
                                            width: 142
                                            height: 40
                                            text: "+  New layout"
                                            draw_bg.color: #3567c8
                                            draw_bg.color_hover: #4278df
                                            draw_bg.color_down: #284f9e
                                            draw_text.color: #ffffff
                                            draw_text.text_style.font_size: 13
                                        }
                                    }

                                    layout_choices := View {
                                        width: Fill
                                        height: Fit
                                        flow: Right
                                        spacing: 10

                                        layout_keyboard_select := Button {
                                            width: Fill
                                            height: 56
                                            text: "  Full keyboard"
                                            icon_walk: Walk{ width: 22, height: 22 }
                                            draw_icon +: {
                                                color: #a8d8ef
                                                svg: crate_resource("self:resources/keyboard.svg")
                                            }
                                            draw_bg.color: #292d37
                                            draw_bg.color_hover: #363c49
                                            draw_bg.color_down: #414959
                                            draw_text.color: #e5e7ed
                                            draw_text.text_style.font_size: 13
                                        }

                                        layout_touchpad_select := Button {
                                            width: Fill
                                            height: 56
                                            text: "  Touchpad"
                                            icon_walk: Walk{ width: 22, height: 22 }
                                            draw_icon +: {
                                                color: #a8d8ef
                                                svg: crate_resource("self:resources/touchpad.svg")
                                            }
                                            draw_bg.color: #3159a8
                                            draw_bg.color_hover: #3b68c1
                                            draw_bg.color_down: #284f9e
                                            draw_text.color: #ffffff
                                            draw_text.text_style.font_size: 13
                                        }
                                    }

                                    layout_custom_1 := Button {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }

                                    layout_gamepad_select := Button {
                                        width: Fill
                                        height: 56
                                        text: "  Gamepad"
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_2 := Button {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_3 := Button {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }
                                    layout_custom_4 := Button {
                                        width: Fill
                                        height: 44
                                        visible: false
                                        text: ""
                                        draw_bg.color: #292d37
                                        draw_bg.color_hover: #363c49
                                        draw_bg.color_down: #414959
                                        draw_text.color: #e5e7ed
                                        draw_text.text_style.font_size: 13
                                    }

                                    layout_create_form := View {
                                        width: Fill
                                        height: Fit
                                        visible: false
                                        flow: Down
                                        spacing: 10
                                        padding: 16
                                        show_bg: true
                                        draw_bg +: { color: #x191c23 }

                                        layout_name_input := TextInput {
                                            width: Fill
                                            height: 42
                                            empty_text: "Name this layout"
                                        }

                                        layout_create_type := View {
                                            width: Fill
                                            height: Fit
                                            flow: Right
                                            spacing: 8

                                            layout_create_keyboard := Button {
                                                width: Fill
                                                height: 40
                                                text: "Full keyboard"
                                                draw_bg.color: #3159a8
                                                draw_bg.color_hover: #3b68c1
                                                draw_bg.color_down: #284f9e
                                                draw_text.color: #ffffff
                                                draw_text.text_style.font_size: 12
                                            }

                                            layout_create_touchpad := Button {
                                                width: Fill
                                                height: 40
                                                text: "Touchpad"
                                                draw_bg.color: #292d37
                                                draw_bg.color_hover: #363c49
                                                draw_bg.color_down: #414959
                                                draw_text.color: #e5e7ed
                                                draw_text.text_style.font_size: 12
                                            }
                                            layout_create_gamepad := Button {
                                                width: Fill
                                                height: 40
                                                text: "Gamepad"
                                                draw_bg.color: #292d37
                                                draw_bg.color_hover: #363c49
                                                draw_bg.color_down: #414959
                                                draw_text.color: #e5e7ed
                                                draw_text.text_style.font_size: 12
                                            }
                                        }

                                        layout_create_actions := View {
                                            width: Fill
                                            height: Fit
                                            flow: Right
                                            spacing: 8

                                            layout_save := Button {
                                                width: 104
                                                height: 38
                                                text: "Save layout"
                                                draw_bg.color: #3567c8
                                                draw_bg.color_hover: #4278df
                                                draw_bg.color_down: #284f9e
                                                draw_text.color: #ffffff
                                                draw_text.text_style.font_size: 12
                                            }

                                            layout_cancel := Button {
                                                width: 92
                                                height: 38
                                                text: "Cancel"
                                                draw_bg.color: #292d37
                                                draw_bg.color_hover: #363c49
                                                draw_bg.color_down: #414959
                                                draw_text.color: #c2c9d4
                                                draw_text.text_style.font_size: 12
                                            }
                                        }

                                        layout_save_status := Label {
                                            width: Fill
                                            text: "Choose a name and a control surface."
                                            draw_text.color: #9ba2b1
                                            draw_text.text_style.font_size: 12
                                        }
                                    }

                                    layout_preview_title := Label {
                                        width: Fill
                                        visible: false
                                        text: "Touchpad"
                                        draw_text.color: #f4f5f8
                                        draw_text.text_style.font_size: 18
                                    }

                                    layout_touchpad_preview := View {
                                        width: Fill
                                        height: if state.is_compact { 340 } else { 460 }
                                        visible: false
                                        flow: Overlay
                                        show_bg: true
                                        draw_bg.color: #174b69

                                        touchpad_mouse_icon := Label {
                                            width: Fill
                                            height: Fill
                                            text: "◎"
                                            align: Align{ x: 0.5, y: 0.5 }
                                            draw_text.color: #a8d8ef
                                            draw_text.text_style.font_size: 40
                                        }
                                    }

                                    layout_mouse_buttons := View {
                                        width: Fill
                                        height: 52
                                        visible: false
                                        flow: Right
                                        spacing: 8

                                        mouse_left_button := Button {
                                            width: Fill
                                            height: 52
                                            text: "Left click"
                                            draw_bg.color: #28729a
                                            draw_bg.color_hover: #3286b1
                                            draw_bg.color_down: #1d5a7b
                                            draw_text.color: #ffffff
                                            draw_text.text_style.font_size: 13
                                        }
                                        mouse_middle_button := Button {
                                            width: Fill
                                            height: 52
                                            text: "Middle click"
                                            draw_bg.color: #28729a
                                            draw_bg.color_hover: #3286b1
                                            draw_bg.color_down: #1d5a7b
                                            draw_text.color: #ffffff
                                            draw_text.text_style.font_size: 13
                                        }
                                        mouse_right_button := Button {
                                            width: Fill
                                            height: 52
                                            text: "Right click"
                                            draw_bg.color: #28729a
                                            draw_bg.color_hover: #3286b1
                                            draw_bg.color_down: #1d5a7b
                                            draw_text.color: #ffffff
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
                                        draw_bg.color: #191c23

                                        keyboard_function_row := View {
                                            width: Fill
                                            height: 34
                                            flow: Right
                                            spacing: 4
                                            key_esc := Button { width: Fill height: 34 text: "Esc" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f1 := Button { width: Fill height: 34 text: "F1" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f2 := Button { width: Fill height: 34 text: "F2" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f3 := Button { width: Fill height: 34 text: "F3" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f4 := Button { width: Fill height: 34 text: "F4" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f5 := Button { width: Fill height: 34 text: "F5" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f6 := Button { width: Fill height: 34 text: "F6" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f7 := Button { width: Fill height: 34 text: "F7" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f8 := Button { width: Fill height: 34 text: "F8" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f9 := Button { width: Fill height: 34 text: "F9" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f10 := Button { width: Fill height: 34 text: "F10" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f11 := Button { width: Fill height: 34 text: "F11" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            key_f12 := Button { width: Fill height: 34 text: "F12" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_number_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_num1 := Button { width: Fill height: 40 text: "`" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num2 := Button { width: Fill height: 40 text: "1" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num3 := Button { width: Fill height: 40 text: "2" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num4 := Button { width: Fill height: 40 text: "3" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num5 := Button { width: Fill height: 40 text: "4" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num6 := Button { width: Fill height: 40 text: "5" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num7 := Button { width: Fill height: 40 text: "6" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num8 := Button { width: Fill height: 40 text: "7" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num9 := Button { width: Fill height: 40 text: "8" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num10 := Button { width: Fill height: 40 text: "9" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num11 := Button { width: Fill height: 40 text: "0" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num12 := Button { width: Fill height: 40 text: "-" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_num13 := Button { width: Fill height: 40 text: "=" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_backspace := Button { width: Fill height: 40 text: "⌫" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 11 }
                                        }

                                        keyboard_qwerty_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_tab := Button { width: Fill height: 40 text: "Tab" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_q := Button { width: Fill height: 40 text: "Q" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_w := Button { width: Fill height: 40 text: "W" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_e := Button { width: Fill height: 40 text: "E" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_r := Button { width: Fill height: 40 text: "R" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_t := Button { width: Fill height: 40 text: "T" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_y := Button { width: Fill height: 40 text: "Y" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_u := Button { width: Fill height: 40 text: "U" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_i := Button { width: Fill height: 40 text: "I" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_o := Button { width: Fill height: 40 text: "O" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_p := Button { width: Fill height: 40 text: "P" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_bracket := Button { width: Fill height: 40 text: "[" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_bracket2 := Button { width: Fill height: 40 text: "]" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_enter1 := Button { width: Fill height: 40 text: "Enter" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_home_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_caps := Button { width: Fill height: 40 text: "Caps" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_a := Button { width: Fill height: 40 text: "A" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_s := Button { width: Fill height: 40 text: "S" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_d := Button { width: Fill height: 40 text: "D" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_f := Button { width: Fill height: 40 text: "F" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_g := Button { width: Fill height: 40 text: "G" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_h := Button { width: Fill height: 40 text: "H" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_j := Button { width: Fill height: 40 text: "J" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_k := Button { width: Fill height: 40 text: "K" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_l := Button { width: Fill height: 40 text: "L" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_semicolon := Button { width: Fill height: 40 text: ";" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_quote := Button { width: Fill height: 40 text: "'" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_enter2 := Button { width: Fill height: 40 text: "↵" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 12 }
                                        }

                                        keyboard_bottom_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_shift := Button { width: Fill height: 40 text: "Shift" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_z := Button { width: Fill height: 40 text: "Z" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_x := Button { width: Fill height: 40 text: "X" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_c := Button { width: Fill height: 40 text: "C" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_v := Button { width: Fill height: 40 text: "V" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_b := Button { width: Fill height: 40 text: "B" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_n := Button { width: Fill height: 40 text: "N" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_m := Button { width: Fill height: 40 text: "M" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_comma := Button { width: Fill height: 40 text: "," draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_period := Button { width: Fill height: 40 text: "." draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_slash := Button { width: Fill height: 40 text: "/" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            key_shift2 := Button { width: Fill height: 40 text: "Shift" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                        }

                                        keyboard_modifier_row := View {
                                            width: Fill height: 40 flow: Right spacing: 4
                                            key_ctrl := Button { width: Fill height: 40 text: "Ctrl" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_fn := Button { width: Fill height: 40 text: "Fn" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_alt := Button { width: Fill height: 40 text: "Alt" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_space := Button { width: Fill height: 40 text: "Space" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            key_alt2 := Button { width: Fill height: 40 text: "Alt" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_menu := Button { width: Fill height: 40 text: "Menu" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_ctrl2 := Button { width: Fill height: 40 text: "Ctrl" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            key_up := Button { width: Fill height: 40 text: "↑" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 13 }
                                            key_down := Button { width: Fill height: 40 text: "↓" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 13 }
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
                                            color: #x174b69
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
                                                    color: #a8d8ef
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
                                                draw_text.color: #8fc4df
                                                draw_text.text_style.font_size: 16
                                            }
                                            scroll_track := View {
                                                width: 2
                                                height: Fill
                                                margin: Inset{ left: 8, right: 8 }
                                                show_bg: true
                                                draw_bg +: { color: #x8fc4df }
                                            }
                                            scroll_down := Label {
                                                width: Fill
                                                height: 24
                                                text: "v"
                                                align: Align{ x: 0.5, y: 0.5 }
                                                draw_text.color: #8fc4df
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

                                        surface_left_click := Button {
                                            width: Fill
                                            height: 52
                                            text: "Left click"
                                            draw_bg.color: #287da7
                                            draw_bg.color_hover: #3286b1
                                            draw_bg.color_down: #1d5a7b
                                            draw_text.color: #ffffff
                                            draw_text.text_style.font_size: 13
                                        }
                                        surface_middle_click := Button {
                                            width: Fill
                                            height: 52
                                            text: "Middle click"
                                            draw_bg.color: #287da7
                                            draw_bg.color_hover: #3286b1
                                            draw_bg.color_down: #1d5a7b
                                            draw_text.color: #ffffff
                                            draw_text.text_style.font_size: 13
                                        }
                                        surface_right_click := Button {
                                            width: Fill
                                            height: 52
                                            text: "Right click"
                                            draw_bg.color: #287da7
                                            draw_bg.color_hover: #3286b1
                                            draw_bg.color_down: #1d5a7b
                                            draw_text.color: #ffffff
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
                                        draw_bg +: { color: #x191c23 }

                                        gamepad_shoulders := View {
                                            width: Fill height: 52 flow: Right spacing: 10
                                            gamepad_lb := Button { width: Fill height: 52 text: "LB" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 15 }
                                            gamepad_lt := Button { width: Fill height: 52 text: "LT" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 15 }
                                            gamepad_rb := Button { width: Fill height: 52 text: "RB" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 15 }
                                            gamepad_rt := Button { width: Fill height: 52 text: "RT" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 15 }
                                        }

                                        gamepad_controls := View {
                                            width: Fill height: Fill flow: Right spacing: 10 align: Align{ y: 0.5 }
                                            gamepad_dpad := View {
                                                width: Fill height: 200 flow: Down spacing: 6 align: Align{ x: 0.5, y: 0.5 }
                                                gamepad_dpad_up := Button { width: 66 height: 56 text: "↑" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 20 }
                                                gamepad_dpad_horizontal := View {
                                                    width: Fill height: 56 flow: Right spacing: 6
                                                    gamepad_dpad_left := Button { width: Fill height: 56 text: "←" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 20 }
                                                    gamepad_dpad_right := Button { width: Fill height: 56 text: "→" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 20 }
                                                }
                                                gamepad_dpad_down := Button { width: 66 height: 56 text: "↓" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 20 }
                                            }
                                            gamepad_center_controls := View {
                                                width: Fill height: 200 flow: Down spacing: 10 align: Align{ y: 0.5 }
                                                gamepad_menu_row := View {
                                                    width: Fill height: 48 flow: Right spacing: 8
                                                    gamepad_select := Button { width: Fill height: 48 text: "Select" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                                    gamepad_home := Button { width: Fill height: 48 text: "Home" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                                    gamepad_start := Button { width: Fill height: 48 text: "Start" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_bg.color_down: #3159a8 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                                }
                                                gamepad_face_row := View {
                                                    width: Fill height: Fill flow: Right spacing: 8
                                                    gamepad_x := Button { width: Fill height: Fill text: "X" draw_bg.color: #3159a8 draw_bg.color_hover: #3b68c1 draw_bg.color_down: #284f9e draw_text.color: #ffffff draw_text.text_style.font_size: 20 }
                                                    gamepad_y := Button { width: Fill height: Fill text: "Y" draw_bg.color: #a18324 draw_bg.color_hover: #b3912b draw_bg.color_down: #79611b draw_text.color: #ffffff draw_text.text_style.font_size: 20 }
                                                    gamepad_a := Button { width: Fill height: Fill text: "A" draw_bg.color: #438844 draw_bg.color_hover: #529b53 draw_bg.color_down: #326833 draw_text.color: #ffffff draw_text.text_style.font_size: 20 }
                                                    gamepad_b := Button { width: Fill height: Fill text: "B" draw_bg.color: #a13b3b draw_bg.color_hover: #b44b4b draw_bg.color_down: #7c2b2b draw_text.color: #ffffff draw_text.text_style.font_size: 20 }
                                                }
                                            }
                                        }

                                        gamepad_status := Label { width: Fill height: 24 text: "Tap a control to send a Bluetooth gamepad press." draw_text.color: #9ba2b1 draw_text.text_style.font_size: 12 }
                                    }

                                    layout_surface_keyboard := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.selected_layout_kind == "keyboard"
                                        flow: Down
                                        spacing: if state.is_narrow { 3 } else { 6 }
                                        padding: if state.is_narrow { 5 } else { 10 }
                                        show_bg: true
                                        draw_bg +: { color: #x191c23 }

                                        landscape_keyboard_toolbar := View {
                                            width: Fill
                                            height: 42
                                            visible: state.is_compact && !state.is_narrow
                                            flow: Right
                                            spacing: 8
                                            align: Align{ y: 0.5 }

                                            landscape_menu_toggle := Button {
                                                width: 42
                                                height: 42
                                                text: "☰"
                                                draw_bg.color: #292d37
                                                draw_bg.color_hover: #363c49
                                                draw_text.color: #e5e7ed
                                            }

                                            landscape_keyboard_title := Label {
                                                width: Fit
                                                text: "Mouse / Keyboard"
                                                draw_text.color: #f4f5f8
                                                draw_text.text_style.font_size: 22
                                            }

                                            landscape_toolbar_spacer := View { width: Fill height: 1 }

                                            landscape_keyboard_toggle := Button {
                                                width: 42
                                                height: 42
                                                text: ""
                                                icon_walk: Walk{ width: 20, height: 20 }
                                                draw_icon +: {
                                                    color: #a8d8ef
                                                    svg: crate_resource("self:resources/keyboard.svg")
                                                }
                                                draw_bg.color: #292d37
                                            }

                                            landscape_settings_button := Button {
                                                width: 42
                                                height: 42
                                                text: ""
                                                icon_walk: Walk{ width: 20, height: 20 }
                                                draw_icon +: {
                                                    color: #a8d8ef
                                                    svg: crate_resource("self:resources/settings.svg")
                                                }
                                                draw_bg.color: #292d37
                                            }
                                        }

                                        surface_keyboard_function := View {
                                            width: Fill
                                            height: if state.is_narrow { 26 } else { 34 }
                                            visible: !state.is_narrow
                                            flow: Right
                                            spacing: if state.is_narrow { 2 } else { 4 }
                                            sk_esc := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "Esc" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f1 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F1" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f2 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F2" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f3 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F3" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f4 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F4" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f5 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F5" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f6 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F6" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f7 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F7" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f8 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F8" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f9 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F9" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f10 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F10" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f11 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F11" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                            sk_f12 := Button { width: Fill height: if state.is_narrow { 26 } else { 34 } text: "F12" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_number := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sn_1 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "`" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_2 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "1" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_3 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "2" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_4 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "3" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_5 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "4" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_6 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "5" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_7 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "6" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_8 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "7" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_9 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "8" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_10 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "9" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_11 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "0" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            sn_backspace := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "⌫" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 11 }
                                        }

                                        surface_keyboard_symbols := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            ss_minus := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "-" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_equal := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "=" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_open_bracket := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "[" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_close_bracket := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "]" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_backslash := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "\\" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_semicolon := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: ";" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_quote := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "'" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_comma := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "," draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_period := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "." draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                            ss_slash := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "/" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 11 }
                                        }

                                        surface_keyboard_top := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            st_tab := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Tab" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            st_q := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Q" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_w := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "W" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_e := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "E" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_r := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "R" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_t := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "T" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_y := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Y" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_u := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "U" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_i := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "I" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_o := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "O" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_p := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "P" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            st_enter := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Enter" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_home := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sh_caps := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Caps" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            sh_a := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "A" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_s := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "S" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_d := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "D" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_f := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "F" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_g := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "G" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_h := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "H" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_j := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "J" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_k := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "K" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_l := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "L" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sh_enter := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Ent" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 12 }
                                        }

                                        surface_keyboard_bottom := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sb_shift := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Shift" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            sb_z := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Z" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_x := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "X" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_c := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "C" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_v := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "V" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_b := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "B" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_n := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "N" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_m := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "M" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sb_shift2 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Shift" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                        }

                                        surface_keyboard_modifiers := View {
                                            width: Fill height: if state.is_narrow { 32 } else { 40 } flow: Right spacing: if state.is_narrow { 2 } else { 4 }
                                            sm_ctrl := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Ctrl" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            sm_fn := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Fn" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            sm_alt := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Alt" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            sm_space := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Space" draw_bg.color: #292d37 draw_text.color: #e5e7ed draw_text.text_style.font_size: 12 }
                                            sm_alt2 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Alt" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                            sm_ctrl2 := Button { width: Fill height: if state.is_narrow { 32 } else { 40 } text: "Ctrl" draw_bg.color: #3159a8 draw_text.color: #ffffff draw_text.text_style.font_size: 10 }
                                        }
                                    }
                                }

                                layout_settings_panel := View {
                                    width: Fill
                                    height: Fit
                                    visible: state.current_screen == "layout_settings"
                                    flow: Down
                                    spacing: 16
                                    padding: 20
                                    show_bg: true
                                    draw_bg +: { color: #x222630 }

                                    layout_settings_name := Label {
                                        width: Fill
                                        text: "Touchpad settings"
                                        draw_text.color: #f4f5f8
                                        draw_text.text_style.font_size: 20
                                    }

                                    layout_touchpad_settings := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.selected_layout_kind == "touchpad"
                                        flow: Down
                                        spacing: 12

                                        pointer_speed_label := Label { width: Fill text: "Pointer speed" draw_text.color: #c2c9d4 draw_text.text_style.font_size: 14 }
                                        pointer_speed_choices := View {
                                            width: Fill height: Fit flow: Right spacing: 8
                                            pointer_slow := Button { width: Fill height: 42 text: "Slow" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_text.color: #e5e7ed }
                                            pointer_standard := Button { width: Fill height: 42 text: "Standard" draw_bg.color: #3159a8 draw_bg.color_hover: #3b68c1 draw_text.color: #ffffff }
                                            pointer_fast := Button { width: Fill height: 42 text: "Fast" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_text.color: #e5e7ed }
                                        }
                                        touchpad_tap_toggle := Button { width: Fill height: 42 text: "Tap to click   ·   On" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_text.color: #e5e7ed }
                                        touchpad_scroll_toggle := Button { width: Fill height: 42 text: "Natural scrolling   ·   Off" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_text.color: #e5e7ed }
                                    }

                                    layout_keyboard_settings := View {
                                        width: Fill
                                        height: Fit
                                        visible: state.selected_layout_kind == "keyboard"
                                        flow: Down
                                        spacing: 12

                                        keyboard_layout_label := Label { width: Fill text: "Keyboard layout" draw_text.color: #c2c9d4 draw_text.text_style.font_size: 14 }
                                        keyboard_qwerty_choice := Button { width: Fill height: 42 text: "QWERTY   ·   English (US)" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_text.color: #e5e7ed }
                                        keyboard_repeat_toggle := Button { width: Fill height: 42 text: "Key repeat   ·   On" draw_bg.color: #292d37 draw_bg.color_hover: #363c49 draw_text.color: #e5e7ed }
                                    }
                                }
                                }
                            }

                            mobile_backdrop := Button {
                                width: Fill
                                height: Fill
                                visible: !state.sidebar_collapsed
                                text: ""
                                draw_bg.color: #0009
                            }

                            sidebar := View {
                                width: 280
                                height: Fill
                                visible: !state.sidebar_collapsed
                                flow: Down
                                spacing: 10
                                padding: 14
                                show_bg: true
                                draw_bg +: { color: #x20232b }

                                app_brand := Label {
                                    width: Fill
                                    text: "OpenWASD"
                                    draw_text.color: #f3f4f7
                                    draw_text.text_style.font_size: 22
                                }

                                sidebar_toggle := Button {
                                    width: Fill
                                    height: 40
                                    text: "×   Close menu"
                                    draw_bg.color: #292d37
                                    draw_bg.color_hover: #363c49
                                    draw_bg.color_down: #414959
                                    draw_text.color: #e5e7ed
                                    draw_text.text_style.font_size: 13
                                }

                                nav_devices := Button {
                                    width: Fill
                                    height: 44
                                    text: "Devices"
                                    draw_bg.color: if state.current_screen == "devices" { #3159a8 } else { #20232b }
                                    draw_bg.color_hover: #303746
                                    draw_bg.color_down: #3b4659
                                    draw_text.color: #ffffff
                                    draw_text.text_style.font_size: 14
                                }

                                nav_layouts := Button {
                                    width: Fill
                                    height: 44
                                    text: "Layouts"
                                    draw_bg.color: if state.current_screen == "layouts" { #3159a8 } else { #20232b }
                                    draw_bg.color_hover: #303746
                                    draw_bg.color_down: #3b4659
                                    draw_text.color: #ffffff
                                    draw_text.text_style.font_size: 14
                                }

                                sidebar_spacer := View {
                                    width: Fill
                                    height: Fill
                                }

                                sidebar_footer := Label {
                                    width: Fill
                                    text: "OpenWASD  •  v0.1"
                                    draw_text.color: #777f90
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
            self.layout_surface_active = false;
            script_eval!(cx, {
                mod.state.current_screen = "layout_settings";
                ui.main_view.render()
            });
            self.render_layouts(cx);
        }

        if self
            .ui
            .button(cx, ids!(layout_settings_back))
            .clicked(actions)
        {
            self.layout_surface_active = true;
            #[cfg(target_os = "android")]
            self.request_hid_start();
            script_eval!(cx, {
                mod.state.current_screen = "layout_surface";
                ui.main_view.render()
            });
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

        if self.ui.button(cx, ids!(nav_devices)).clicked(actions) {
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
        }

        if self.ui.button(cx, ids!(nav_layouts)).clicked(actions) {
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
        match event {
            Event::Startup => {
                log!("[OpenWASD][Bluetooth] app startup; starting connected-device refresh timer");
                self.bluetooth_timer = Some(cx.start_interval(0.2));
                self.bluetooth_refresh_timer = Some(cx.start_interval(3.0));
                self.refresh_system_bluetooth_devices();
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
        }

        self.record_diagnostic_event(cx, event);

        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

impl App {
    #[cfg(target_os = "android")]
    fn request_hid_start(&mut self) {
        self.hid_start_pending = true;
        self.hid_start_retries = 0;
        self.send_bluetooth_command(BluetoothCommand::StartHid);
    }

    fn send_bluetooth_command(&mut self, command: BluetoothCommand) {
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
        self.selected_layout_kind = kind;
        self.selected_custom_layout = custom_index;
        self.layout_surface_active = true;
        #[cfg(target_os = "android")]
        self.request_hid_start();
        let kind_name = match kind {
            LayoutKind::Keyboard => "keyboard",
            LayoutKind::Gamepad => "gamepad",
            LayoutKind::Touchpad => "touchpad",
        };
        script_eval!(cx, {
            mod.state.current_screen = "layout_surface";
            mod.state.selected_layout_kind = #(kind_name);
            mod.state.sidebar_collapsed = true;
            ui.main_view.render()
        });
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
            .widget(cx, ids!(layout_touchpad_settings))
            .set_visible(cx, is_touchpad);
        self.ui
            .widget(cx, ids!(layout_keyboard_settings))
            .set_visible(cx, is_keyboard);
        let preview_name = self
            .selected_custom_layout
            .and_then(|index| self.saved_layouts.get(index))
            .map(|layout| layout.name.as_str())
            .unwrap_or(self.selected_layout_kind.label());
        self.ui
            .label(cx, ids!(layout_surface_title))
            .set_text(cx, preview_name);
        let device_status = if let Some(status) = self.hid_host.label() {
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
            if self.hid_host.gamepad_ready() {
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
        let text = if self.diagnostic_events.is_empty() {
            "Waiting for mouse, keyboard, or touch input…".to_string()
        } else {
            self.diagnostic_events.join("\n")
        };
        self.ui
            .label(cx, ids!(diagnostics_events))
            .set_text(cx, &text);
    }

    fn record_diagnostic_event(&mut self, cx: &mut Cx, event: &Event) {
        if !self.device_diagnostics_open {
            return;
        }
        let entry = match event {
            Event::MouseMove(mouse) => Some(format!(
                "Mouse moved     x {:>5.0}   y {:>5.0}",
                mouse.abs.x, mouse.abs.y
            )),
            Event::MouseDown(mouse) => Some(format!("{:?} pressed", mouse.button)),
            Event::MouseUp(mouse) => Some(format!("{:?} released", mouse.button)),
            Event::KeyDown(key) => Some(format!("Key pressed    {:?}", key.key_code)),
            Event::KeyUp(key) => Some(format!("Key released   {:?}", key.key_code)),
            _ => None,
        };
        if let Some(entry) = entry {
            self.diagnostic_events.insert(0, entry);
            self.diagnostic_events.truncate(12);
            self.render_diagnostics(cx);
        }
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

#[cfg(target_os = "android")]
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
