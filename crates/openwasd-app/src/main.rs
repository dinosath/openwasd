use dioxus::prelude::*;
use openwasd_bluetooth::{
    BluetoothCommand, BluetoothDeviceInfo, BluetoothEvent, BluetoothService, BtleplugAdapter,
    ConnectionState, HidHostState, merge_devices,
};
use openwasd_core::custom_layout::{ActiveControls, ControlAction, CustomControl};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, rc::Rc};
use tokio::runtime::Runtime;

const APP_CSS: &str = include_str!("../assets/dioxus.css");
const CUSTOM_LIMIT: usize = 6;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
enum LayoutKind {
    Keyboard,
    Gamepad,
    Custom,
    #[default]
    Touchpad,
}

impl LayoutKind {
    fn title(self) -> &'static str {
        match self {
            Self::Keyboard => "Keyboard",
            Self::Gamepad => "Gamepad",
            Self::Custom => "Custom layout",
            Self::Touchpad => "Mouse / keyboard",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Devices,
    Layouts,
    CreateLayout,
    Surface,
    Settings,
}

struct Backend {
    _runtime: Runtime,
    bluetooth: BluetoothService,
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
    layouts_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
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
        label: "clickme".into(),
        action: ControlAction::GamepadButton(0),
        text_output: None,
        hardware_key: None,
        show_text: true,
        show_icon: false,
        custom_icon: None,
    }
}

fn action_id(action: ControlAction) -> &'static str {
    match action {
        ControlAction::Keyboard(44) => "key-space",
        ControlAction::Keyboard(40) => "key-enter",
        ControlAction::Keyboard(41) => "key-escape",
        ControlAction::GamepadButton(0) => "gamepad-a",
        ControlAction::GamepadButton(1) => "gamepad-b",
        ControlAction::GamepadButton(2) => "gamepad-x",
        ControlAction::GamepadButton(3) => "gamepad-y",
        ControlAction::MouseButton(_) => "mouse-left",
        ControlAction::BluetoothSettings => "bluetooth-settings",
        _ => "gamepad-a",
    }
}

fn action_from_id(id: &str) -> ControlAction {
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

#[allow(non_snake_case)] // Dioxus components use PascalCase names.
fn OpenWASDApp() -> Element {
    let backend = use_hook(|| Rc::new(Backend::new()));
    let mut screen = use_signal(|| Screen::Dashboard);
    let devices = use_signal(Vec::<BluetoothDeviceInfo>::new);
    let mut bluetooth_status = use_signal(|| "Bluetooth is ready.".to_string());
    let host = use_signal(HidHostState::default);
    let hid_ready = use_signal(|| false);
    let mut layouts = use_signal(load_layouts);
    let mut selected_kind = use_signal(|| LayoutKind::Touchpad);
    let mut selected_layout = use_signal(|| None::<usize>);
    let mut controls = use_signal(Vec::<CustomControl>::new);
    let active_controls = use_signal(ActiveControls::default);
    let mut edit_mode = use_signal(|| false);
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

    let current_screen = screen();
    let selected_layout_index = selected_layout();
    let layout_title = selected_layout_index
        .and_then(|index| layouts().get(index).map(|layout| layout.name.clone()))
        .unwrap_or_else(|| selected_kind().title().to_string());
    let current_controls = selected_layout_index
        .and_then(|index| layouts().get(index).map(|layout| layout.controls.clone()))
        .unwrap_or_else(|| controls());
    let device_list = devices();
    let edit = edit_mode();
    let app_backend = backend.clone();
    let refresh_backend = app_backend.clone();
    let scan_backend = app_backend.clone();
    let keyboard_backend = app_backend.clone();
    let touchpad_backend = app_backend.clone();
    let gamepad_backend = app_backend.clone();
    let create_backend = app_backend.clone();
    let surface_gamepad_backend = app_backend.clone();
    let surface_keyboard_backend = app_backend.clone();
    let surface_touch_backend = app_backend.clone();
    let surface_left_backend = app_backend.clone();
    let surface_middle_backend = app_backend.clone();
    let surface_right_backend = app_backend.clone();
    let status_text = bluetooth_status();
    let hid_host = host();
    let hid_is_ready = hid_ready();

    rsx! {
        style { {APP_CSS} }
        div { class: "app-shell",
            header { class: "topbar",
                if current_screen == Screen::Dashboard || current_screen == Screen::Devices || current_screen == Screen::Layouts {
                    div { class: "brand-mark", "O" }
                } else {
                    button { class: "icon-button", aria_label: "Back", onclick: move |_| {
                        screen.set(if current_screen == Screen::Settings { Screen::Surface } else { Screen::Layouts });
                    }, "←" }
                }
                div { class: "title-block",
                    h1 { {match current_screen {
                        Screen::Dashboard => "OpenWASD",
                        Screen::Devices => "Devices",
                        Screen::Layouts => "Layouts",
                        Screen::CreateLayout => "New layout",
                        Screen::Surface => layout_title.as_str(),
                        Screen::Settings => "Layout settings",
                    }} }
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
                if current_screen == Screen::Surface && selected_kind() == LayoutKind::Custom {
                    button { class: "icon-button", aria_label: "Edit layout", onclick: move |_| edit_mode.set(!edit_mode()), if edit { "✓" } else { "✎" } }
                    button { class: "icon-button", aria_label: "Layout settings", onclick: move |_| screen.set(Screen::Settings), "⚙" }
                }
                if current_screen == Screen::Surface && selected_kind() != LayoutKind::Custom {
                    button { class: "icon-button", aria_label: "Layout settings", onclick: move |_| screen.set(Screen::Settings), "⚙" }
                }
                if matches!(current_screen, Screen::Dashboard | Screen::Devices | Screen::Layouts) {
                    div { class: "connection-indicators",
                        span { class: "status-dot", title: "Bluetooth" }
                        span { class: "status-label", "Bluetooth" }
                    }
                }
            }

            main { class: "page-content",
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
                            QuickCard { title: "Gamepad", detail: "Send gamepad buttons to a paired host.", onclick: move |_| { selected_kind.set(LayoutKind::Gamepad); selected_layout.set(None); screen.set(Screen::Surface); gamepad_backend.send(BluetoothCommand::StartHid); } }
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
                                        let device_signal = device.rssi_dbm.map(|value| format!("{value} dBm")).unwrap_or_else(|| "Signal unavailable".into());
                                        let row_backend = app_backend.clone();
                                        rsx! { article { class: "device-row",
                                            div { class: "device-symbol", "◉" }
                                            div { class: "device-info", h3 { "{name}" }, p { "{state_name} · {device_signal}" } }
                                            button { class: "button secondary", onclick: move |_| row_backend.send(device_command.clone()), "{device_action}" }
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
                                LayoutCard { title: "Full keyboard", icon: "⌨", selected: selected_kind() == LayoutKind::Keyboard && selected_layout().is_none(), onclick: move |_| { selected_kind.set(LayoutKind::Keyboard); selected_layout.set(None); screen.set(Screen::Surface); keyboard_backend.send(BluetoothCommand::StartHid); } }
                                LayoutCard { title: "Touchpad", icon: "▱", selected: selected_kind() == LayoutKind::Touchpad && selected_layout().is_none(), onclick: move |_| { selected_kind.set(LayoutKind::Touchpad); selected_layout.set(None); screen.set(Screen::Surface); touchpad_backend.send(BluetoothCommand::StartHid); } }
                                LayoutCard { title: "Gamepad", icon: "◉", selected: selected_kind() == LayoutKind::Gamepad && selected_layout().is_none(), onclick: move |_| { selected_kind.set(LayoutKind::Gamepad); selected_layout.set(None); screen.set(Screen::Surface); surface_gamepad_backend.send(BluetoothCommand::StartHid); } }
                                for (index, layout) in layouts().into_iter().enumerate() {
                                    {
                                        let title = layout.name.clone();
                                        let kind = layout.kind;
                                        let layout_backend = app_backend.clone();
                                        rsx! { LayoutCard { title, icon: "▦", selected: selected_layout() == Some(index), onclick: move |_| { selected_kind.set(kind); selected_layout.set(Some(index)); screen.set(Screen::Surface); layout_backend.send(BluetoothCommand::StartHid); } } }
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
                                    else if layouts().len() >= 4 { page_status.set("You can save up to four custom layouts for now.".into()); }
                                    else {
                                        let mut updated = layouts();
                                        updated.push(SavedLayout { name: name.clone(), kind: LayoutKind::Custom, controls: vec![default_control()] });
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
                    Screen::Surface => rsx! {
                        section { class: "surface-panel",
                            if let Some(host_name) = hid_host.name { p { class: "surface-status", "{host_name} · {status_text}" } }
                            else { p { class: "surface-status", "{status_text}" } }
                            if selected_kind() == LayoutKind::Custom {
                                div { class: "custom-surface-grid",
                                    for (index, control) in current_controls.clone().into_iter().enumerate() {
                                        if edit {
                                            div { class: "control-editor",
                                                input { class: "text-input compact-input", value: "{control.label}", oninput: move |event| {
                                                    let mut updated = layouts();
                                                    if let Some(layout_index) = selected_layout() {
                                                    if let Some(layout) = updated.get_mut(layout_index) {
                                                        if let Some(item) = layout.controls.get_mut(index) { item.label = event.value(); }
                                                            let _ = save_layouts(&updated);
                                                            layouts.set(updated);
                                                        }
                                                    }
                                                } }
                                                select { value: "{action_id(control.action)}", onchange: move |event| {
                                                    let mut updated = layouts();
                                                    if let Some(layout_index) = selected_layout() {
                                                        if let Some(layout) = updated.get_mut(layout_index) {
                                                            if let Some(item) = layout.controls.get_mut(index) { item.action = action_from_id(&event.value()); }
                                                            let _ = save_layouts(&updated);
                                                            layouts.set(updated);
                                                        }
                                                    }
                                                },
                                                    option { value: "gamepad-a", "Gamepad · A" }
                                                    option { value: "gamepad-b", "Gamepad · B" }
                                                    option { value: "gamepad-x", "Gamepad · X" }
                                                    option { value: "gamepad-y", "Gamepad · Y" }
                                                    option { value: "key-space", "Keyboard · Space" }
                                                    option { value: "key-enter", "Keyboard · Enter" }
                                                    option { value: "key-escape", "Keyboard · Escape" }
                                                    option { value: "mouse-left", "Mouse · Left click" }
                                                    option { value: "bluetooth-settings", "Bluetooth · Settings" }
                                                }
                                                button { class: "icon-button danger-icon", aria_label: "Remove button", onclick: move |_| {
                                                    let mut updated = layouts();
                                                    if let Some(layout_index) = selected_layout() {
                                                        if let Some(layout) = updated.get_mut(layout_index) { if index < layout.controls.len() { layout.controls.remove(index); } }
                                                        let _ = save_layouts(&updated);
                                                        layouts.set(updated);
                                                    }
                                                }, "×" }
                                            }
                                        } else {
                                            {
                                                let backend_down = app_backend.clone();
                                                let backend_up = app_backend.clone();
                                                let backend_cancel = app_backend.clone();
                                                let backend_leave = app_backend.clone();
                                                let mut active_down = active_controls;
                                                let mut active_up = active_controls;
                                                let mut active_cancel = active_controls;
                                                let mut active_leave = active_controls;
                                                let down_source = format!("surface:{index}");
                                                let up_source = down_source.clone();
                                                let cancel_source = down_source.clone();
                                                let leave_source = down_source.clone();
                                                let label = control.label.clone();
                                                rsx! { button { class: "surface-control",
                                                    onpointerdown: move |_| press_control(&backend_down, &mut active_down, down_source.clone(), control.action),
                                                    onpointerup: move |_| release_control(&backend_up, &mut active_up, &up_source),
                                                    onpointercancel: move |_| release_control(&backend_cancel, &mut active_cancel, &cancel_source),
                                                    onpointerleave: move |_| release_control(&backend_leave, &mut active_leave, &leave_source),
                                                    "{label}"
                                                } }
                                            }
                                        }
                                    }
                                }
                                if edit && current_controls.len() < CUSTOM_LIMIT {
                                    button { class: "button add-control", onclick: move |_| {
                                        if let Some(index) = selected_layout() {
                                            let mut updated = layouts();
                                            if let Some(layout) = updated.get_mut(index) {
                                                let number = layout.controls.len() + 1;
                                                layout.controls.push(CustomControl { label: format!("Button {number}"), ..default_control() });
                                                match save_layouts(&updated) { Ok(()) => { layouts.set(updated); page_status.set("Button added.".into()); }, Err(error) => page_status.set(format!("Could not save button: {error}")) }
                                            }
                                        }
                                    }, "+ Add button" }
                                }
                                if edit && !page_status().is_empty() { p { class: "form-status", "{page_status}" } }
                            } else if selected_kind() == LayoutKind::Gamepad {
                                div { class: "gamepad-surface",
                                    button { class: "gamepad-button gamepad-y", onclick: move |_| surface_gamepad_backend.send(BluetoothCommand::HidGamepadPress(3)), "Y" }
                                    button { class: "gamepad-button gamepad-x", onclick: move |_| surface_keyboard_backend.send(BluetoothCommand::HidGamepadPress(2)), "X" }
                                    button { class: "gamepad-button gamepad-b", onclick: move |_| surface_touch_backend.send(BluetoothCommand::HidGamepadPress(1)), "B" }
                                    button { class: "gamepad-button gamepad-a", onclick: move |_| surface_left_backend.send(BluetoothCommand::HidGamepadPress(0)), "A" }
                                }
                            } else if selected_kind() == LayoutKind::Keyboard {
                                div { class: "keyboard-surface",
                                    for (label, usage) in [("Esc", 41), ("1", 30), ("2", 31), ("3", 32), ("4", 33), ("5", 34), ("6", 35), ("7", 36), ("8", 37), ("9", 38), ("0", 39), ("Tab", 43), ("Q", 20), ("W", 26), ("E", 8), ("R", 21), ("T", 23), ("Y", 28), ("U", 24), ("I", 12), ("O", 18), ("P", 19), ("A", 4), ("S", 22), ("D", 7), ("F", 9), ("G", 10), ("H", 11), ("J", 13), ("K", 14), ("L", 15), ("Z", 29), ("X", 27), ("C", 6), ("V", 25), ("B", 5), ("N", 17), ("M", 16), ("Space", 44)] {
                                        {
                                            let key_backend = surface_keyboard_backend.clone();
                                            rsx! { button { class: "key-button", onclick: move |_| { let mut report = [0; 8]; report[2] = usage; key_backend.send(BluetoothCommand::HidKey(report)); }, "{label}" } }
                                        }
                                    }
                                }
                            } else {
                                div { class: "touch-surface", onclick: move |_| surface_touch_backend.send(BluetoothCommand::HidClick(1)),
                                    span { "Tap to click" }
                                }
                                div { class: "mouse-actions",
                                    button { class: "button secondary", onclick: move |_| surface_left_backend.send(BluetoothCommand::HidClick(1)), "Left click" }
                                    button { class: "button secondary", onclick: move |_| surface_middle_backend.send(BluetoothCommand::HidClick(4)), "Middle click" }
                                    button { class: "button secondary", onclick: move |_| surface_right_backend.send(BluetoothCommand::HidClick(2)), "Right click" }
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
                                p { class: "muted", "Surface settings are not available yet." }
                            }
                            button { class: "button secondary", onclick: move |_| screen.set(Screen::Surface), "Done" }
                        }
                    },
                }
            }

            nav { class: "bottom-nav",
                NavButton { active: current_screen == Screen::Dashboard, label: "Home", onclick: move |_| screen.set(Screen::Dashboard) }
                NavButton { active: current_screen == Screen::Devices, label: "Devices", onclick: move |_| screen.set(Screen::Devices) }
                NavButton { active: matches!(current_screen, Screen::Layouts | Screen::CreateLayout), label: "Layouts", onclick: move |_| screen.set(Screen::Layouts) }
            }
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
fn NavButton(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! { button { class: if active { "nav-button active" } else { "nav-button" }, onclick: move |event| onclick.call(event), "{label}" } }
}

fn main() {
    dioxus::launch(OpenWASDApp);
}
