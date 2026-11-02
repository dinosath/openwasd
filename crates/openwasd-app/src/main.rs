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
    layouts
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

fn action_label(action: ControlAction) -> &'static str {
    match action {
        ControlAction::Keyboard(44) => "Space",
        ControlAction::Keyboard(40) => "Enter",
        ControlAction::Keyboard(_) => "Keyboard key",
        ControlAction::GamepadButton(0) => "Gamepad A",
        ControlAction::GamepadButton(1) => "Gamepad B",
        ControlAction::GamepadButton(2) => "Gamepad X",
        ControlAction::GamepadButton(3) => "Gamepad Y",
        ControlAction::GamepadButton(_) => "Gamepad button",
        ControlAction::MouseButton(_) => "Mouse button",
        ControlAction::BluetoothSettings => "Bluetooth settings",
    }
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
    let mut edit_mode = use_signal(|| false);
    let mut layout_undo = use_signal(Vec::<LayoutSnapshot>::new);
    let mut layout_redo = use_signal(Vec::<LayoutSnapshot>::new);
    let gesture_state = use_signal(|| None::<PointerGesture>);
    let preview_geometry = use_signal(|| None::<(usize, ControlGeometry)>);
    let button_editor_index = use_signal(|| None::<usize>);
    let mut button_editor_tab = use_signal(|| 0_u8);
    let mut hardware_learning = use_signal(|| None::<LearnTarget>);
    let mut layout_mapping_action = use_signal(|| "gamepad-a".to_string());
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
                                    else {
                                        let mut updated = layouts();
                                        updated.push(SavedLayout { name: name.clone(), kind: LayoutKind::Custom, controls: vec![default_control()], geometry: vec![default_geometry(0)], hardware_mappings: Vec::new() });
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
                                            p { class: "muted", "Choose a keyboard shortcut to send when this button is pressed." }
                                            div { class: "mapping-choice-grid",
                                                for (label, usage) in [("Space", 44_u8), ("Enter", 40), ("Escape", 41), ("Tab", 43), ("A", 4), ("B", 5), ("X", 27), ("Y", 28)] {
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
                                        option { value: "gamepad-a", "Gamepad A" }
                                        option { value: "gamepad-b", "Gamepad B" }
                                        option { value: "gamepad-x", "Gamepad X" }
                                        option { value: "gamepad-y", "Gamepad Y" }
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

fn main() {
    dioxus::launch(OpenWASDApp);
}
