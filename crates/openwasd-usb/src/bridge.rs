//! Android Open Accessory transport for input reports over ordinary USB.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::io::Read;
#[cfg(target_os = "android")]
use std::io::Write;

const MAGIC: &[u8; 4] = b"OW02";

/// Fixed-size gamepad messages keep untrusted input bounded and versioned.
#[derive(Clone, Copy, Debug)]
pub enum GamepadMessage {
    Report([u8; 7]),
    Press(u8),
    Hat(u8),
    Keyboard([u8; 8]),
    Key([u8; 8]),
    Mouse {
        buttons: u8,
        dx: i8,
        dy: i8,
        wheel: i8,
    },
    Click(u8),
}
#[cfg(target_os = "android")]
impl GamepadMessage {
    fn encode(self) -> [u8; 9] {
        let mut out = [0; 9];
        match self {
            Self::Report(r) => {
                out[0] = 1;
                out[1..8].copy_from_slice(&r);
            }
            Self::Press(b) => {
                out[0] = 2;
                out[1] = b;
            }
            Self::Hat(h) => {
                out[0] = 3;
                out[1] = h;
            }
            Self::Keyboard(r) => {
                out[0] = 4;
                out[1..9].copy_from_slice(&r);
            }
            Self::Key(r) => {
                out[0] = 5;
                out[1..9].copy_from_slice(&r);
            }
            Self::Mouse {
                buttons,
                dx,
                dy,
                wheel,
            } => {
                out[0] = 6;
                out[1] = buttons;
                out[2] = dx as u8;
                out[3] = dy as u8;
                out[4] = wheel as u8;
            }
            Self::Click(buttons) => {
                out[0] = 7;
                out[1] = buttons;
            }
        }
        out
    }
}

#[cfg(target_os = "android")]
pub struct Client {
    running: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    sender: std::sync::mpsc::SyncSender<GamepadMessage>,
}
#[cfg(target_os = "android")]
impl Client {
    pub fn start() -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = std::sync::mpsc::sync_channel::<GamepadMessage>(256);
        let (run, ready) = (running.clone(), connected.clone());
        thread::spawn(move || {
            let mut permission_requested = false;
            while run.load(Ordering::Relaxed) {
                if let Ok(Some(mut stream)) = open_accessory(&mut permission_requested) {
                    while receiver.try_recv().is_ok() {} // Never replay stale presses after reconnect.
                    ready.store(true, Ordering::Release);
                    while run.load(Ordering::Relaxed) && ready.load(Ordering::Acquire) {
                        let packet = match receiver.recv_timeout(Duration::from_secs(1)) {
                            Ok(msg) => msg.encode(),
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => [0; 9],
                            Err(_) => break,
                        };
                        if stream.write_all(MAGIC).is_err() || stream.write_all(&packet).is_err() {
                            break;
                        }
                    }
                    ready.store(false, Ordering::Release);
                }
                thread::sleep(Duration::from_millis(500));
            }
        });
        Self {
            running,
            connected,
            sender,
        }
    }
    /// Returns false if there is no usable USB host or the bounded queue is full.
    pub fn send(&self, message: GamepadMessage) -> bool {
        if !self.connected.load(Ordering::Acquire) {
            return false;
        }
        if self.sender.try_send(message).is_err() {
            self.connected.store(false, Ordering::Release);
            return false;
        }
        true
    }
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

#[cfg(target_os = "android")]
use std::os::fd::FromRawFd;

#[cfg(target_os = "android")]
fn open_accessory(permission_requested: &mut bool) -> Result<Option<std::fs::File>, String> {
    use jni::{
        JavaVM,
        objects::{JObject, JObjectArray, JValue},
    };
    let vm =
        unsafe { JavaVM::from_raw(makepad_android_state::get_java_vm() as *mut jni::sys::JavaVM) }
            .map_err(|error| error.to_string())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|error| error.to_string())?;
    let activity =
        unsafe { JObject::from_raw(makepad_android_state::get_activity() as jni::sys::jobject) };
    if activity.is_null() {
        return Ok(None);
    }
    let service_name = env.new_string("usb").map_err(|error| error.to_string())?;
    let service_name = JObject::from(service_name);
    let manager = env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        )
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())?;
    let accessories = env
        .call_method(
            &manager,
            "getAccessoryList",
            "()[Landroid/hardware/usb/UsbAccessory;",
            &[],
        )
        .and_then(|value| value.l())
        .map_err(|error| error.to_string())?;
    if accessories.is_null() {
        return Ok(None);
    }
    let accessories = JObjectArray::from(accessories);
    if env
        .get_array_length(&accessories)
        .map_err(|error| error.to_string())?
        == 0
    {
        return Ok(None);
    }
    let accessory = env
        .get_object_array_element(&accessories, 0)
        .map_err(|error| error.to_string())?;
    let has_permission = env
        .call_method(
            &manager,
            "hasPermission",
            "(Landroid/hardware/usb/UsbAccessory;)Z",
            &[JValue::Object(&accessory)],
        )
        .and_then(|value| value.z())
        .map_err(|error| error.to_string())?;
    if !has_permission {
        if !*permission_requested {
            let activity_class = env
                .call_method(&activity, "getClass", "()Ljava/lang/Class;", &[])
                .and_then(|value| value.l())
                .map_err(|error| error.to_string())?;
            let intent = env
                .new_object(
                    "android/content/Intent",
                    "(Landroid/content/Context;Ljava/lang/Class;)V",
                    &[JValue::Object(&activity), JValue::Object(&activity_class)],
                )
                .map_err(|error| error.to_string())?;
            let action = env
                .new_string("dev.makepad.openwasd.USB_ACCESSORY_PERMISSION")
                .map_err(|error| error.to_string())?;
            env.call_method(
                &intent,
                "setAction",
                "(Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&JObject::from(action))],
            )
            .map_err(|error| error.to_string())?;
            let pending = env
                .call_static_method(
                    "android/app/PendingIntent",
                    "getActivity",
                    "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
                    &[
                        JValue::Object(&activity),
                        JValue::Int(0),
                        JValue::Object(&intent),
                        // FLAG_MUTABLE lets UsbManager include its permission result extras.
                        JValue::Int(0x0200_0000 | 0x0800_0000),
                    ],
                )
                .and_then(|value| value.l())
                .map_err(|error| error.to_string())?;
            env.call_method(
                &manager,
                "requestPermission",
                "(Landroid/hardware/usb/UsbAccessory;Landroid/app/PendingIntent;)V",
                &[JValue::Object(&accessory), JValue::Object(&pending)],
            )
            .map_err(|error| {
                let _ = env.exception_clear();
                error.to_string()
            })?;
            *permission_requested = true;
        }
        return Ok(None);
    }
    *permission_requested = false;
    let descriptor = env
        .call_method(
            &manager,
            "openAccessory",
            "(Landroid/hardware/usb/UsbAccessory;)Landroid/os/ParcelFileDescriptor;",
            &[JValue::Object(&accessory)],
        )
        .and_then(|value| value.l())
        .map_err(|error| {
            let _ = env.exception_clear();
            error.to_string()
        })?;
    if descriptor.is_null() {
        return Ok(None);
    }
    let fd = env
        .call_method(&descriptor, "detachFd", "()I", &[])
        .and_then(|value| value.i())
        .map_err(|error| error.to_string())?;
    if fd < 0 {
        return Ok(None);
    }
    Ok(Some(unsafe { std::fs::File::from_raw_fd(fd) }))
}
#[cfg(target_os = "android")]
impl Drop for Client {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

#[cfg(target_os = "linux")]
mod host {
    use super::*;
    use nusb::{
        MaybeFuture,
        transfer::{ControlIn, ControlOut, ControlType, Recipient},
    };
    use openwasd_output::{Gamepad, GamepadReport, Keyboard, Mouse};
    use std::{collections::BTreeMap, io, sync::Mutex};
    struct Session {
        running: Arc<AtomicBool>,
        status: Arc<Mutex<String>>,
        events: Arc<Mutex<std::collections::VecDeque<String>>>,
        thread: thread::JoinHandle<()>,
    }
    pub struct Host {
        sessions: BTreeMap<String, Session>,
    }
    impl Default for Host {
        fn default() -> Self {
            Self {
                sessions: BTreeMap::new(),
            }
        }
    }
    impl Host {
        pub fn connect(&mut self, serial: &str, name: &str) {
            if self.sessions.contains_key(serial) {
                return;
            }
            let serial = serial.to_owned();
            let name = name.to_owned();
            let running = Arc::new(AtomicBool::new(true));
            let status = Arc::new(Mutex::new("Preparing Android USB accessory".into()));
            let (run, state) = (running.clone(), status.clone());
            let events = Arc::new(Mutex::new(std::collections::VecDeque::new()));
            let event_log = events.clone();
            let key = serial.clone();
            let worker = thread::spawn(move || {
                let result = run_session(&serial, &name, &run, &state, &event_log);
                *state.lock().unwrap() = match result {
                    Ok(()) => "Disconnected".into(),
                    Err(e) => format!("USB connection failed: {e}"),
                };
            });
            self.sessions.insert(
                key,
                Session {
                    running,
                    status,
                    events,
                    thread: worker,
                },
            );
        }
        pub fn disconnect(&mut self, serial: &str) {
            if let Some(session) = self.sessions.remove(serial) {
                session.running.store(false, Ordering::Relaxed);
                let _ = session.thread.join();
            }
        }
        pub fn events(&self, serial: &str) -> Vec<String> {
            self.sessions
                .get(serial)
                .map(|s| s.events.lock().unwrap().iter().cloned().collect())
                .unwrap_or_default()
        }
        pub fn clear_events(&self, serial: &str) {
            if let Some(s) = self.sessions.get(serial) {
                s.events.lock().unwrap().clear();
            }
        }
        pub fn status(&self, serial: &str) -> Option<String> {
            self.sessions
                .get(serial)
                .map(|s| s.status.lock().unwrap().clone())
        }
    }
    impl Drop for Host {
        fn drop(&mut self) {
            for session in self.sessions.values() {
                session.running.store(false, Ordering::Relaxed);
            }
            for (_, session) in std::mem::take(&mut self.sessions) {
                let _ = session.thread.join();
            }
        }
    }
    fn key_name(code: u8) -> String {
        match code {
            4..=29 => char::from(b'A' + code - 4).to_string(),
            30..=38 => char::from(b'1' + code - 30).to_string(),
            39 => "0".into(),
            40 => "Enter".into(),
            41 => "Escape".into(),
            42 => "Backspace".into(),
            43 => "Tab".into(),
            44 => "Space".into(),
            79 => "Right".into(),
            80 => "Left".into(),
            81 => "Down".into(),
            82 => "Up".into(),
            128 => "Volume up".into(),
            129 => "Volume down".into(),
            _ => format!("HID {code}"),
        }
    }
    const AOA_VID: u16 = 0x18d1;
    const AOA_PIDS: [u16; 2] = [0x2d00, 0x2d01];

    fn usb_error(error: impl std::fmt::Display) -> io::Error {
        io::Error::other(error.to_string())
    }

    fn device_for_path(path_id: &str) -> io::Result<Option<nusb::DeviceInfo>> {
        let wanted = path_id.strip_prefix("usb:").unwrap_or(path_id);
        Ok(nusb::list_devices()
            .wait()
            .map_err(usb_error)?
            .find(|device| {
                device
                    .sysfs_path()
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy() == wanted)
            }))
    }

    fn request_aoa(info: &nusb::DeviceInfo) -> io::Result<()> {
        if info.vendor_id() == AOA_VID && AOA_PIDS.contains(&info.product_id()) {
            return Ok(());
        }
        let device = info.open().wait().map_err(usb_error)?;
        let version = device
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: 51,
                    value: 0,
                    index: 0,
                    length: 2,
                },
                Duration::from_secs(1),
            )
            .wait()
            .map_err(usb_error)?;
        if version.len() != 2 || u16::from_le_bytes([version[0], version[1]]) == 0 {
            return Err(io::Error::other(
                "Android device does not support USB accessory mode",
            ));
        }
        for (index, value) in [
            "OpenWASD",
            "OpenWASD Companion",
            "OpenWASD USB input",
            "1",
            "https://github.com/dinosath/openwasd",
            "openwasd",
        ]
        .iter()
        .enumerate()
        {
            let mut bytes = value.as_bytes().to_vec();
            bytes.push(0);
            device
                .control_out(
                    ControlOut {
                        control_type: ControlType::Vendor,
                        recipient: Recipient::Device,
                        request: 52,
                        value: 0,
                        index: index as u16,
                        data: &bytes,
                    },
                    Duration::from_secs(1),
                )
                .wait()
                .map_err(usb_error)?;
        }
        device
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: 53,
                    value: 0,
                    index: 0,
                    data: &[],
                },
                Duration::from_secs(1),
            )
            .wait()
            .map_err(usb_error)?;
        Ok(())
    }

    fn accessory_device(
        path_id: &str,
        run: &AtomicBool,
        state: &Mutex<String>,
    ) -> io::Result<(nusb::Device, u8, u8)> {
        let mut started = false;
        let mut started_at = None;
        while run.load(Ordering::Relaxed) {
            if let Some(info) = device_for_path(path_id)? {
                if info.vendor_id() == AOA_VID && AOA_PIDS.contains(&info.product_id()) {
                    *state.lock().unwrap() = "Opening Android accessory USB device".into();
                    let device = info.open().wait().map_err(usb_error)?;
                    if device
                        .active_configuration()
                        .map(|c| c.configuration_value())
                        .ok()
                        != Some(1)
                    {
                        device.set_configuration(1).wait().map_err(usb_error)?;
                    }
                    let configuration = device.active_configuration().map_err(usb_error)?;
                    for interface in configuration.interfaces() {
                        let descriptor = interface.first_alt_setting();
                        let mut input = None;
                        for endpoint in descriptor.endpoints() {
                            if endpoint.transfer_type() != nusb::descriptors::TransferType::Bulk {
                                continue;
                            }
                            if endpoint.address() & 0x80 != 0 {
                                input = Some(endpoint.address());
                            }
                        }
                        if let Some(input) = input {
                            return Ok((device.clone(), interface.interface_number(), input));
                        }
                    }
                    return Err(io::Error::other(
                        "Android accessory interface has no bulk endpoints",
                    ));
                }
                if !started {
                    *state.lock().unwrap() = "Requesting Android accessory mode".into();
                    request_aoa(&info)?;
                    started = true;
                    started_at = Some(std::time::Instant::now());
                    *state.lock().unwrap() = "Waiting for Android USB accessory mode".into();
                }
            }
            if started_at.is_some_and(|time| time.elapsed() > Duration::from_secs(15)) {
                return Err(io::Error::other(
                    "Android device did not enter USB accessory mode",
                ));
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "USB session stopped",
        ))
    }

    fn run_session(
        serial: &str,
        name: &str,
        run: &AtomicBool,
        state: &Mutex<String>,
        events: &Mutex<std::collections::VecDeque<String>>,
    ) -> io::Result<()> {
        *state.lock().unwrap() = "Switching to Android USB accessory mode".into();
        let (device, interface_number, input_address) = accessory_device(serial, run, state)?;
        *state.lock().unwrap() = "Claiming Android accessory interface".into();
        let interface = device
            .detach_and_claim_interface(interface_number)
            .wait()
            .map_err(usb_error)?;
        *state.lock().unwrap() = "Opening Android accessory bulk endpoints".into();
        let mut input = interface
            .endpoint::<nusb::transfer::Bulk, nusb::transfer::In>(input_address)
            .map_err(usb_error)?
            .reader(256)
            .with_read_timeout(Duration::from_secs(3));
        *state.lock().unwrap() = "Waiting for OpenWASD app stream".into();
        let mut display = format!("OpenWASD {name}");
        while display.len() > 60 {
            display.pop();
        }
        let mut gamepad = Gamepad::create(&display)?;
        let mut keyboard = Keyboard::create(&format!("{} keyboard", display))?;
        let mut mouse = Mouse::create(&format!("{} mouse", display))?;
        let mut last_keyboard = [0u8; 8];
        let mut mouse_buttons = 0u8;
        *state.lock().unwrap() = "Connected · gamepad, keyboard, mouse".into();
        let mut packet = [0u8; 9];
        while run.load(Ordering::Relaxed) && read_frame(&mut input, &mut packet).is_ok() {
            match packet[0] {
                0 => (),
                1 => {
                    let p = &packet[1..8];
                    gamepad.send(GamepadReport {
                        buttons: u16::from_le_bytes([p[0], p[1]]),
                        hat: p[2],
                        axes: [p[3] as i8, p[4] as i8, p[5] as i8, p[6] as i8],
                    })?;
                }
                2 if packet[1] < 16 => {
                    gamepad.send(GamepadReport {
                        buttons: 1 << packet[1],
                        ..Default::default()
                    })?;
                    thread::sleep(Duration::from_millis(50));
                    gamepad.send(GamepadReport::default())?;
                }
                3 if packet[1] < 8 => {
                    gamepad.send(GamepadReport {
                        hat: packet[1],
                        ..Default::default()
                    })?;
                    thread::sleep(Duration::from_millis(50));
                    gamepad.send(GamepadReport::default())?;
                }
                4 => {
                    last_keyboard.copy_from_slice(&packet[1..9]);
                    keyboard.send(last_keyboard)?;
                }
                5 => {
                    let mut key = [0; 8];
                    key.copy_from_slice(&packet[1..9]);
                    keyboard.send(key)?;
                    thread::sleep(Duration::from_millis(50));
                    keyboard.send(last_keyboard)?;
                }
                6 => {
                    mouse_buttons = packet[1] & 7;
                    mouse.send(
                        mouse_buttons,
                        packet[2] as i8,
                        packet[3] as i8,
                        packet[4] as i8,
                    )?;
                }
                7 => {
                    mouse.send(mouse_buttons | (packet[1] & 7), 0, 0, 0)?;
                    thread::sleep(Duration::from_millis(50));
                    mouse.send(mouse_buttons, 0, 0, 0)?;
                }
                _ => break,
            }
            if packet[0] != 0 {
                let description = match packet[0] {
                    1 => format!(
                        "Gamepad: buttons 0x{:04x}, hat {}",
                        u16::from_le_bytes([packet[1], packet[2]]),
                        packet[3]
                    ),
                    2 => {
                        format!("Gamepad button {} pressed and released", packet[1] + 1)
                    }
                    3 => {
                        format!("Gamepad direction {} pressed and released", packet[1])
                    }
                    4 | 5 => {
                        let keys = packet[3..9]
                            .iter()
                            .copied()
                            .filter(|code| *code != 0)
                            .map(key_name)
                            .collect::<Vec<_>>();
                        format!(
                            "Keyboard: {} · modifiers 0x{:02x}{}",
                            if keys.is_empty() {
                                "released".into()
                            } else {
                                keys.join(", ")
                            },
                            packet[1],
                            if packet[0] == 5 { " (tap)" } else { "" }
                        )
                    }
                    6 => format!(
                        "Mouse: buttons {}, movement ({}, {}), wheel {}",
                        packet[1], packet[2] as i8, packet[3] as i8, packet[4] as i8
                    ),
                    7 => format!("Mouse: button {} clicked", packet[1]),
                    _ => String::new(),
                };
                let mut log = events.lock().unwrap();
                log.push_front(description);
                log.truncate(20);
            }
        }
        // Closing the uinput handle removes the device and releases held inputs.
        drop(gamepad);
        drop(keyboard);
        drop(mouse);
        Ok(())
    }
    fn read_frame(input: &mut impl Read, packet: &mut [u8; 9]) -> io::Result<()> {
        let mut marker = [0; 4];
        input.read_exact(&mut marker)?;
        while marker != *MAGIC {
            marker.copy_within(1.., 0);
            input.read_exact(&mut marker[3..])?;
        }
        input.read_exact(packet)
    }
}
#[cfg(target_os = "linux")]
pub use host::Host;
