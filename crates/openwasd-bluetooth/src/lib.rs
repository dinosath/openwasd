//! Platform-neutral Bluetooth lifecycle service.
//!
//! Native backends implement [`BluetoothAdapter`]. The service owns Tokio,
//! serializes commands, and publishes state changes for Makepad or another UI.
//! The mock adapter is intentional: it makes the Phase 1 UI and state machine
//! testable before platform permissions and native APIs are wired in.

use async_trait::async_trait;
use openwasd_core::{DeviceKind, Transport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

fn debug_log(message: &str) {
    #[cfg(target_os = "android")]
    {
        use std::ffi::{CString, c_char, c_int};

        #[link(name = "log")]
        unsafe extern "C" {
            fn __android_log_write(
                priority: c_int,
                tag: *const c_char,
                text: *const c_char,
            ) -> c_int;
        }

        let Ok(message) = CString::new(message.replace('\0', " ")) else {
            return;
        };
        // cargo-makepad's Android runner captures the Makepad DEBUG tag.
        unsafe { __android_log_write(3, c"Makepad".as_ptr(), message.as_ptr()) };
    }
    #[cfg(not(target_os = "android"))]
    eprintln!("[OpenWASD-BT] {message}");
}

#[cfg(target_os = "android")]
fn log_hid_report_delivery(kind: &str, delivered: bool, details: &str) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static MOUSE_REPORTS: AtomicUsize = AtomicUsize::new(0);
    static KEYBOARD_REPORTS: AtomicUsize = AtomicUsize::new(0);
    static GAMEPAD_REPORTS: AtomicUsize = AtomicUsize::new(0);
    let counter = if kind == "mouse" {
        &MOUSE_REPORTS
    } else if kind == "gamepad" {
        &GAMEPAD_REPORTS
    } else {
        &KEYBOARD_REPORTS
    };
    let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
    if delivered {
        if count == 1 || count % 100 == 0 {
            debug_log(&format!(
                "HID {kind} report delivered (report #{count}); {details}"
            ));
        }
    } else if count == 1 || count % 25 == 0 {
        debug_log(&format!(
            "HID {kind} report not delivered (report #{count}); no connected host with an enabled report subscription; {details}"
        ));
    }
}

#[cfg(feature = "btleplug")]
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
/// Android-only system Bluetooth HID peripheral support from the btleplug fork.
#[cfg(all(feature = "btleplug", target_os = "android"))]
pub use btleplug::platform::HidPeripheral;
#[cfg(feature = "btleplug")]
use btleplug::platform::{Adapter, Manager, Peripheral};

pub type DeviceId = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionState {
    Discovered,
    Pairing,
    Paired,
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
    Disconnected,
    Failed { message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BluetoothDeviceInfo {
    pub id: DeviceId,
    pub name: Option<String>,
    pub kind: DeviceKind,
    pub transport: Transport,
    pub state: ConnectionState,
    pub rssi_dbm: Option<i16>,
    pub battery_percent: Option<u8>,
    pub services: Vec<String>,
    pub trusted: bool,
}

/// A Bluetooth link can exist before the host has enabled HID input reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidHostState {
    pub name: Option<String>,
    pub report_mask: u8,
}

impl HidHostState {
    pub fn mouse_ready(&self) -> bool {
        self.name.is_some() && self.report_mask & 1 != 0
    }
    pub fn keyboard_ready(&self) -> bool {
        self.name.is_some() && self.report_mask & 2 != 0
    }
    pub fn gamepad_ready(&self) -> bool {
        self.name.is_some() && self.report_mask & 4 != 0
    }
    pub fn label(&self) -> Option<String> {
        self.name.as_ref().map(|name| {
            if self.mouse_ready() && self.keyboard_ready() {
                if self.gamepad_ready() {
                    format!("{name} connected · mouse, keyboard, and gamepad ready")
                } else {
                    format!("{name} connected · mouse and keyboard ready")
                }
            } else if self.gamepad_ready() {
                format!("{name} connected · gamepad ready")
            } else if self.mouse_ready() {
                format!("{name} connected · mouse ready; keyboard waiting")
            } else if self.keyboard_ready() {
                format!("{name} connected · keyboard ready; mouse waiting")
            } else {
                format!("{name} connected · waiting for HID input to be enabled")
            }
        })
    }
}

/// Merge native paired and scanned views by identity, never by display name.
pub fn merge_devices(devices: &mut Vec<BluetoothDeviceInfo>, incoming: Vec<BluetoothDeviceInfo>) {
    fn identity(id: &str) -> String {
        let id = id.strip_prefix("system:").unwrap_or(id);
        let address = id.replace([':', '-'], "");
        if address.len() == 12 && address.bytes().all(|b| b.is_ascii_hexdigit()) {
            address.to_ascii_uppercase()
        } else {
            id.to_string()
        }
    }
    for mut device in incoming {
        if let Some(existing) = devices
            .iter_mut()
            .find(|old| identity(&old.id) == identity(&device.id))
        {
            // Keep the scan's actionable ID and live connection state when the
            // paired registry only knows that a bond exists.
            if !existing.id.starts_with("system:") {
                device.id = existing.id.clone();
            }
            if existing.state == ConnectionState::Connected
                && matches!(
                    device.state,
                    ConnectionState::Paired | ConnectionState::Discovered
                )
            {
                device.state = ConnectionState::Connected;
            }
            if device.name.is_none() {
                device.name = existing.name.clone();
            }
            device.trusted |= existing.trusted;
            device.rssi_dbm = device.rssi_dbm.or(existing.rssi_dbm);
            if device.services.is_empty() {
                device.services = existing.services.clone();
            }
            *existing = device;
        } else {
            devices.push(device);
        }
    }
}

#[derive(Clone, Debug)]
pub enum BluetoothError {
    NotFound(DeviceId),
    PermissionDenied,
    Unsupported(&'static str),
    Failed(String),
}

impl std::fmt::Display for BluetoothError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "device {id} was not found"),
            Self::PermissionDenied => write!(f, "Bluetooth permission was denied"),
            Self::Unsupported(message) => write!(f, "unsupported: {message}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

/// Open Android's Bluetooth settings for pairing and connection management.
#[cfg(target_os = "android")]
pub fn open_android_bluetooth_settings() -> Result<(), BluetoothError> {
    robius_android_env::with_activity(|env, activity| -> jni::errors::Result<()> {
        let action = env.new_string("android.settings.BLUETOOTH_SETTINGS")?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[(&action).into()],
        )?;
        env.call_method(
            activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[(&intent).into()],
        )?;
        Ok(())
    })
    .map_err(|error| BluetoothError::Failed(error.to_string()))?
    .map_err(|error| BluetoothError::Failed(error.to_string()))
}

#[async_trait]
pub trait BluetoothAdapter: Send + Sync + 'static {
    async fn scan(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError>;
    async fn connected_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError> {
        Ok(Vec::new())
    }
    async fn pair(&self, id: &DeviceId) -> Result<(), BluetoothError>;
    async fn unpair(&self, id: &DeviceId) -> Result<(), BluetoothError>;
    async fn connect(&self, id: &DeviceId) -> Result<(), BluetoothError>;
    async fn disconnect(&self, id: &DeviceId) -> Result<(), BluetoothError>;
    async fn start_hid(&self) -> Result<(), BluetoothError> {
        Ok(())
    }
    async fn retry_hid(&self) -> Result<(), BluetoothError> {
        self.start_hid().await
    }
    async fn send_hid_mouse(
        &self,
        _buttons: i8,
        _dx: i8,
        _dy: i8,
        _wheel: i8,
    ) -> Result<bool, BluetoothError> {
        Ok(false)
    }
    async fn send_hid_keyboard(&self, _report: [u8; 8]) -> Result<bool, BluetoothError> {
        Ok(false)
    }
    async fn send_hid_gamepad(&self, _report: [u8; 7]) -> Result<bool, BluetoothError> {
        Ok(false)
    }
    async fn hid_host_name(&self) -> Result<Option<String>, BluetoothError> {
        Ok(None)
    }
    async fn hid_host_state(&self) -> Result<HidHostState, BluetoothError> {
        Ok(HidHostState {
            name: self.hid_host_name().await?,
            report_mask: 0,
        })
    }
}

#[cfg(feature = "btleplug")]
pub struct BtleplugAdapter {
    adapter: Mutex<Option<Adapter>>,
    init_error: Option<String>,
    #[cfg(target_os = "android")]
    hid_peripheral: std::sync::Mutex<Option<HidPeripheral>>,
    #[cfg(target_os = "android")]
    hid_permission_prompted: std::sync::atomic::AtomicBool,
    #[cfg(target_os = "android")]
    scan_permission_prompted: std::sync::atomic::AtomicBool,
}

#[cfg(feature = "btleplug")]
impl BtleplugAdapter {
    pub fn new() -> Self {
        #[cfg(target_os = "android")]
        let init_error = match initialize_android_btleplug() {
            Ok(()) => {
                debug_log("btleplug Android JNI initialized successfully");
                None
            }
            Err(error) => {
                debug_log(&format!(
                    "btleplug Android JNI initialization failed: {error}"
                ));
                Some(error)
            }
        };
        #[cfg(not(target_os = "android"))]
        let init_error = None;

        Self {
            adapter: Mutex::new(None),
            init_error,
            #[cfg(target_os = "android")]
            hid_peripheral: std::sync::Mutex::new(None),
            #[cfg(target_os = "android")]
            hid_permission_prompted: std::sync::atomic::AtomicBool::new(false),
            #[cfg(target_os = "android")]
            scan_permission_prompted: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Start advertising this Android device as a BLE HID mouse and keyboard.
    ///
    /// The host must pair with OpenWASD through Android's Bluetooth settings.
    /// Android 12+ also requires the app to be granted Bluetooth connect and
    /// advertise permissions. Returns the peripheral's current status text.
    #[cfg(target_os = "android")]
    pub fn start_hid_peripheral(&self, device_name: &str) -> Result<String, BluetoothError> {
        if let Some(error) = &self.init_error {
            return Err(BluetoothError::Failed(error.clone()));
        }
        request_android_hid_permissions(&self.hid_permission_prompted)?;

        let mut peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        if peripheral.is_none() {
            let name = device_name.to_owned();
            let created = robius_android_env::with_activity(|env, activity| {
                // Bluetooth commands run on Tokio workers whose JNI context
                // loader defaults to the platform loader. Use the Activity's
                // loader so the HID helper packaged in the APK is visible.
                let activity_class = env
                    .call_method(activity, "getClass", "()Ljava/lang/Class;", &[])
                    .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?
                    .l()
                    .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?;
                let class_loader = env
                    .call_method(
                        &activity_class,
                        "getClassLoader",
                        "()Ljava/lang/ClassLoader;",
                        &[],
                    )
                    .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?
                    .l()
                    .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?;
                let current_thread = env
                    .call_static_method(
                        "java/lang/Thread",
                        "currentThread",
                        "()Ljava/lang/Thread;",
                        &[],
                    )
                    .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?
                    .l()
                    .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?;
                env.call_method(
                    current_thread,
                    "setContextClassLoader",
                    "(Ljava/lang/ClassLoader;)V",
                    &[(&class_loader).into()],
                )
                .map_err(|error| btleplug::Error::RuntimeError(error.to_string()))?;
                unsafe { HidPeripheral::new_from_raw_context(activity.as_raw(), &name) }
            })
            .map_err(|error| BluetoothError::Failed(error.to_string()))?
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
            *peripheral = Some(created);
        }

        let peripheral = peripheral
            .as_ref()
            .expect("Android HID peripheral was initialized above");
        let started = peripheral
            .start()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        let status = peripheral
            .status()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        debug_log(&format!(
            "BLE HID start returned started={started}, status={status}"
        ));
        if started {
            Ok(status)
        } else {
            Err(BluetoothError::Failed(status))
        }
    }

    /// Stop Android BLE HID advertising and close its GATT server.
    #[cfg(target_os = "android")]
    pub fn stop_hid_peripheral(&self) -> Result<(), BluetoothError> {
        let peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        if let Some(peripheral) = peripheral.as_ref() {
            peripheral
                .stop()
                .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        }
        Ok(())
    }

    /// Send a relative mouse report to every connected BLE HID host.
    #[cfg(target_os = "android")]
    pub fn send_hid_mouse_report(
        &self,
        buttons: i8,
        dx: i8,
        dy: i8,
        wheel: i8,
    ) -> Result<bool, BluetoothError> {
        let peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        let peripheral = peripheral
            .as_ref()
            .ok_or_else(|| BluetoothError::Failed("BLE HID peripheral is not started".into()))?;
        peripheral
            .send_mouse_report(buttons, dx, dy, wheel)
            .map_err(|error| BluetoothError::Failed(error.to_string()))
    }

    /// Send an eight-byte boot-format keyboard report to connected BLE HID hosts.
    #[cfg(target_os = "android")]
    pub fn send_hid_keyboard_report(&self, report: [u8; 8]) -> Result<bool, BluetoothError> {
        let peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        let peripheral = peripheral
            .as_ref()
            .ok_or_else(|| BluetoothError::Failed("BLE HID peripheral is not started".into()))?;
        peripheral
            .send_keyboard_report(report)
            .map_err(|error| BluetoothError::Failed(error.to_string()))
    }

    /// Send a gamepad report: 16 buttons, a neutral hat, and four centered axes.
    #[cfg(target_os = "android")]
    pub fn send_hid_gamepad_report(&self, report: [u8; 7]) -> Result<bool, BluetoothError> {
        let peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        let peripheral = peripheral
            .as_ref()
            .ok_or_else(|| BluetoothError::Failed("BLE HID peripheral is not started".into()))?;
        peripheral
            .send_gamepad_report(report)
            .map_err(|error| BluetoothError::Failed(error.to_string()))
    }

    /// Return the active BLE HID host name, if one is connected.
    #[cfg(target_os = "android")]
    pub fn hid_connected_host_name(&self) -> Result<Option<String>, BluetoothError> {
        let peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        let Some(peripheral) = peripheral.as_ref() else {
            return Ok(None);
        };
        peripheral
            .connected_host_name()
            .map_err(|error| BluetoothError::Failed(error.to_string()))
    }

    /// Return the current Android HID peripheral status.
    #[cfg(target_os = "android")]
    pub fn hid_status(&self) -> Result<String, BluetoothError> {
        let peripheral = self
            .hid_peripheral
            .lock()
            .map_err(|error| BluetoothError::Failed(error.to_string()))?;
        let Some(peripheral) = peripheral.as_ref() else {
            return Ok("BLE HID peripheral is not initialized".into());
        };
        peripheral
            .status()
            .map_err(|error| BluetoothError::Failed(error.to_string()))
    }

    #[cfg(target_os = "android")]
    async fn start_hid_and_wait_for_ready(&self) -> Result<(), BluetoothError> {
        self.start_hid_peripheral("OpenWASD")?;
        for _ in 0..50 {
            let status = self.hid_status()?;
            if status.starts_with("System HID ready")
                || status.starts_with("System HID is ready")
                || status.starts_with("Connecting system HID to ")
                || status.starts_with("Connected to ")
            {
                debug_log(&format!("Bluetooth HID is ready: {status}"));
                return Ok(());
            }
            if !status.starts_with("Registering system HID") {
                return Err(BluetoothError::Failed(status));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let status = self.hid_status()?;
        Err(BluetoothError::Failed(format!(
            "Timed out waiting for Android system HID registration; current status: {status}"
        )))
    }

    async fn adapter(&self) -> Result<Adapter, BluetoothError> {
        if let Some(error) = &self.init_error {
            return Err(BluetoothError::Failed(error.clone()));
        }
        if let Some(adapter) = self.adapter.lock().await.clone() {
            return Ok(adapter);
        }
        debug_log("creating Bluetooth manager");
        let manager = Manager::new()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        let mut adapters = manager
            .adapters()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        debug_log(&format!(
            "Bluetooth manager returned {} adapter(s)",
            adapters.len()
        ));
        let adapter = adapters.drain(..).next().ok_or(BluetoothError::Failed(
            "No Bluetooth adapter available".into(),
        ))?;
        *self.adapter.lock().await = Some(adapter.clone());
        Ok(adapter)
    }

    fn merge_system_paired_devices(&self, devices: &mut Vec<BluetoothDeviceInfo>) {
        let mut system_devices = system_paired_devices();
        #[cfg(target_os = "android")]
        if let Ok(Some(host_name)) = self.hid_connected_host_name() {
            if let Some(host) = system_devices.iter_mut().find(|device| {
                device
                    .name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&host_name))
            }) {
                debug_log(&format!(
                    "marking Android HID host {:?} as connected",
                    host.name
                ));
                host.state = ConnectionState::Connected;
            }
        }
        merge_devices(devices, system_devices);
        sort_devices(devices);
    }

    async fn peripheral(&self, id: &DeviceId) -> Result<Peripheral, BluetoothError> {
        let adapter = self.adapter().await?;
        let peripherals = adapter
            .peripherals()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        peripherals
            .into_iter()
            .find(|p| p.id().to_string() == *id)
            .ok_or_else(|| BluetoothError::NotFound(id.clone()))
    }
}

#[cfg(all(feature = "btleplug", target_os = "android"))]
fn initialize_android_btleplug() -> Result<(), String> {
    // Makepad owns the Android Activity/VM; borrow its current JNI environment
    // and initialize btleplug's Android classes and native callbacks before
    // any Tokio worker asks the manager for adapters.
    debug_log("starting btleplug Android JNI initialization");
    robius_android_env::with_activity(|env, activity| {
        debug_log(&format!(
            "obtained Makepad JNI environment (env_null={}, activity_null={})",
            env.get_raw().is_null(),
            activity.as_raw().is_null()
        ));
        let class_loader_result = (|| -> jni::errors::Result<()> {
            // btleplug uses the thread context class loader when resolving its
            // Java helpers. Makepad invokes this hook from a native thread, so
            // set the loader explicitly to the app's Activity class loader.
            let activity_class = env
                .call_method(activity, "getClass", "()Ljava/lang/Class;", &[])?
                .l()?;
            let class_loader = env
                .call_method(
                    activity_class,
                    "getClassLoader",
                    "()Ljava/lang/ClassLoader;",
                    &[],
                )?
                .l()?;
            let current_thread = env
                .call_static_method(
                    "java/lang/Thread",
                    "currentThread",
                    "()Ljava/lang/Thread;",
                    &[],
                )?
                .l()?;
            env.call_method(
                current_thread,
                "setContextClassLoader",
                "(Ljava/lang/ClassLoader;)V",
                &[(&class_loader).into()],
            )?;
            Ok(())
        })();
        if let Err(error) = class_loader_result {
            debug_log(&format!("failed to set app context class loader: {error}"));
            return Err(btleplug::Error::RuntimeError(error.to_string()));
        }
        debug_log("set native thread context class loader to Makepad Activity class loader");
        let result = unsafe { btleplug::platform::init_from_raw_env(env.get_raw().cast()) };
        match &result {
            Ok(()) => debug_log("btleplug JNI class/callback registration succeeded"),
            Err(error) => debug_log(&format!(
                "btleplug JNI registration returned error: {error}"
            )),
        }
        result
    })
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[cfg(all(feature = "btleplug", target_os = "android"))]
fn request_android_hid_permissions(
    prompted: &std::sync::atomic::AtomicBool,
) -> Result<(), BluetoothError> {
    use jni::objects::JObject;

    let permissions_granted =
        robius_android_env::with_activity(|env, activity| -> jni::errors::Result<bool> {
            let sdk = env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?;
            if sdk < 31 {
                return Ok(true);
            }

            let names = [
                "android.permission.BLUETOOTH_CONNECT",
                "android.permission.BLUETOOTH_ADVERTISE",
            ];
            let mut missing = Vec::with_capacity(names.len());
            for name in names {
                let permission = env.new_string(name)?;
                let granted = env
                    .call_method(
                        activity,
                        "checkSelfPermission",
                        "(Ljava/lang/String;)I",
                        &[(&permission).into()],
                    )?
                    .i()?
                    == 0;
                if !granted {
                    missing.push(permission);
                }
            }
            if missing.is_empty() {
                return Ok(true);
            }

            // Android expects a dense list of permission names. Do not preserve
            // the original two-slot list with nulls for permissions already
            // granted; that request can fail on devices with partial grants.
            if prompted.swap(true, std::sync::atomic::Ordering::AcqRel) {
                return Ok(false);
            }

            let string_class = env.find_class("java/lang/String")?;
            let permission_array =
                env.new_object_array(missing.len() as i32, string_class, JObject::null())?;
            for (index, permission) in missing.iter().enumerate() {
                env.set_object_array_element(&permission_array, index as i32, permission)?;
            }
            env.call_method(
                activity,
                "requestPermissions",
                "([Ljava/lang/String;I)V",
                &[(&permission_array).into(), 0x4f57.into()],
            )?;
            Ok(false)
        })
        .map_err(|error| BluetoothError::Failed(error.to_string()))?
        .map_err(|error| BluetoothError::Failed(error.to_string()))?;

    if permissions_granted {
        prompted.store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    } else {
        Err(BluetoothError::Failed(
            "Android Bluetooth permissions requested; grant them and start the HID connection again".into(),
        ))
    }
}

#[cfg(all(feature = "btleplug", target_os = "android"))]
fn request_android_scan_permission(
    prompted: &std::sync::atomic::AtomicBool,
) -> Result<(), BluetoothError> {
    use jni::objects::JObject;

    let granted = robius_android_env::with_activity(|env, activity| -> jni::errors::Result<bool> {
        let sdk = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
            .i()?;
        if sdk < 23 {
            return Ok(true);
        }

        let permission_name = if sdk >= 31 {
            "android.permission.BLUETOOTH_SCAN"
        } else {
            "android.permission.ACCESS_FINE_LOCATION"
        };
        let permission = env.new_string(permission_name)?;
        let is_granted = env
            .call_method(
                activity,
                "checkSelfPermission",
                "(Ljava/lang/String;)I",
                &[(&permission).into()],
            )?
            .i()?
            == 0;
        if is_granted {
            return Ok(true);
        }

        if prompted.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return Ok(false);
        }

        let string_class = env.find_class("java/lang/String")?;
        let permission_array = env.new_object_array(1, string_class, JObject::null())?;
        env.set_object_array_element(&permission_array, 0, permission)?;
        env.call_method(
            activity,
            "requestPermissions",
            "([Ljava/lang/String;I)V",
            &[(&permission_array).into(), 0x4f58.into()],
        )?;
        Ok(false)
    })
    .map_err(|error| BluetoothError::Failed(error.to_string()))?
    .map_err(|error| BluetoothError::Failed(error.to_string()))?;

    if granted {
        prompted.store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    } else {
        Err(BluetoothError::Failed(
            "Android Bluetooth scan permission requested; grant it and scan again".into(),
        ))
    }
}

#[cfg(feature = "btleplug")]
#[async_trait]
impl BluetoothAdapter for BtleplugAdapter {
    async fn hid_host_state(&self) -> Result<HidHostState, BluetoothError> {
        #[cfg(target_os = "android")]
        {
            let peripheral = self
                .hid_peripheral
                .lock()
                .map_err(|error| BluetoothError::Failed(error.to_string()))?;
            let Some(peripheral) = peripheral.as_ref() else {
                return Ok(HidHostState::default());
            };
            Ok(HidHostState {
                name: peripheral
                    .connected_host_name()
                    .map_err(|e| BluetoothError::Failed(e.to_string()))?,
                report_mask: peripheral
                    .connected_host_report_mask()
                    .map_err(|e| BluetoothError::Failed(e.to_string()))?,
            })
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(HidHostState::default())
        }
    }
    async fn hid_host_name(&self) -> Result<Option<String>, BluetoothError> {
        #[cfg(target_os = "android")]
        {
            self.hid_connected_host_name()
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(None)
        }
    }

    async fn start_hid(&self) -> Result<(), BluetoothError> {
        #[cfg(target_os = "android")]
        {
            self.hid_permission_prompted
                .store(false, std::sync::atomic::Ordering::Release);
            self.start_hid_and_wait_for_ready().await
        }
        #[cfg(not(target_os = "android"))]
        {
            Err(BluetoothError::Unsupported(
                "BLE HID peripheral mode is currently implemented on Android",
            ))
        }
    }

    async fn retry_hid(&self) -> Result<(), BluetoothError> {
        #[cfg(target_os = "android")]
        {
            self.start_hid_and_wait_for_ready().await
        }
        #[cfg(not(target_os = "android"))]
        {
            Err(BluetoothError::Unsupported(
                "BLE HID peripheral mode is currently implemented on Android",
            ))
        }
    }

    async fn send_hid_mouse(
        &self,
        buttons: i8,
        dx: i8,
        dy: i8,
        wheel: i8,
    ) -> Result<bool, BluetoothError> {
        #[cfg(target_os = "android")]
        {
            let delivered = self.send_hid_mouse_report(buttons, dx, dy, wheel)?;
            let report = format!("buttons={buttons} dx={dx} dy={dy} wheel={wheel}");
            if delivered {
                log_hid_report_delivery("mouse", true, &report);
            } else {
                let status = self.hid_status().unwrap_or_else(|error| error.to_string());
                let host = self
                    .hid_connected_host_name()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "none".into());
                log_hid_report_delivery(
                    "mouse",
                    false,
                    &format!("{report}; service_status={status}; connected_host={host}"),
                );
            }
            Ok(delivered)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = (buttons, dx, dy, wheel);
            Ok(false)
        }
    }

    async fn send_hid_keyboard(&self, report: [u8; 8]) -> Result<bool, BluetoothError> {
        #[cfg(target_os = "android")]
        {
            let delivered = self.send_hid_keyboard_report(report)?;
            let report_details = format!("report={report:?}");
            if delivered {
                log_hid_report_delivery("keyboard", true, &report_details);
            } else {
                let status = self.hid_status().unwrap_or_else(|error| error.to_string());
                let host = self
                    .hid_connected_host_name()
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "none".into());
                log_hid_report_delivery(
                    "keyboard",
                    false,
                    &format!("{report_details}; service_status={status}; connected_host={host}"),
                );
            }
            Ok(delivered)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = report;
            Ok(false)
        }
    }

    async fn send_hid_gamepad(&self, report: [u8; 7]) -> Result<bool, BluetoothError> {
        #[cfg(target_os = "android")]
        {
            let delivered = self.send_hid_gamepad_report(report)?;
            if delivered {
                log_hid_report_delivery("gamepad", true, "");
            }
            Ok(delivered)
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = report;
            Ok(false)
        }
    }

    async fn scan(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError> {
        #[cfg(target_os = "android")]
        request_android_scan_permission(&self.scan_permission_prompted)?;

        let adapter = self.adapter().await?;
        debug_log("starting BLE scan");
        adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        debug_log("BLE scan started; collecting advertisements for 2 seconds");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        adapter
            .stop_scan()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        let mut devices = Vec::new();
        let peripherals = adapter
            .peripherals()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        debug_log(&format!(
            "BLE scan returned {} peripheral(s)",
            peripherals.len()
        ));
        for peripheral in peripherals {
            let Some(properties) = peripheral
                .properties()
                .await
                .map_err(|e| BluetoothError::Failed(e.to_string()))?
            else {
                debug_log("skipping peripheral with no properties yet");
                continue;
            };
            let name = properties.local_name;
            let kind = classify_name(name.as_deref());
            let connected = peripheral.is_connected().await.unwrap_or(false);
            debug_log(&format!(
                "discovered device name={:?} kind={kind:?} connected={connected} rssi={:?}",
                name, properties.rssi
            ));
            devices.push(BluetoothDeviceInfo {
                id: peripheral.id().to_string(),
                name,
                kind,
                transport: Transport::Bluetooth,
                state: if connected {
                    ConnectionState::Connected
                } else {
                    ConnectionState::Discovered
                },
                rssi_dbm: properties.rssi,
                battery_percent: None,
                services: properties
                    .services
                    .into_iter()
                    .map(|uuid| uuid.to_string())
                    .collect(),
                trusted: false,
            });
        }
        self.merge_system_paired_devices(&mut devices);
        Ok(devices)
    }

    async fn connected_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError> {
        let adapter = self.adapter().await?;
        let mut devices = Vec::new();

        // `btleplug` is a BLE central on every supported target, including
        // Android. Return connected GATT peripherals here so RefreshConnected
        // behaves consistently on desktop and Android. Native system
        // registries supplement this with paired devices outside btleplug's
        // BLE-central view.
        debug_log("querying btleplug connected GATT peripherals");
        let peripherals = adapter
            .connected_peripherals()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        debug_log(&format!(
            "btleplug returned {} connected GATT peripheral(s)",
            peripherals.len()
        ));
        for peripheral in peripherals {
            let properties = peripheral
                .properties()
                .await
                .map_err(|e| BluetoothError::Failed(e.to_string()))?;
            let name = properties.as_ref().and_then(|p| p.local_name.clone());
            let id = peripheral.id().to_string();
            let rssi_dbm = properties.as_ref().and_then(|p| p.rssi);
            let services = properties
                .map(|p| {
                    p.services
                        .into_iter()
                        .map(|uuid| uuid.to_string())
                        .collect()
                })
                .unwrap_or_default();
            debug_log(&format!(
                "connected GATT device id={id:?} name={name:?} rssi_dbm={rssi_dbm:?} services={services:?}"
            ));
            devices.push(BluetoothDeviceInfo {
                id,
                kind: classify_name(name.as_deref()),
                name,
                transport: Transport::Bluetooth,
                state: ConnectionState::Connected,
                rssi_dbm,
                battery_percent: None,
                services,
                trusted: false,
            });
        }

        let mut devices = devices;
        self.merge_system_paired_devices(&mut devices);
        let system_devices = devices
            .iter()
            .filter(|device| device.id.starts_with("system:"))
            .cloned()
            .collect::<Vec<_>>();
        let system_connected = system_devices
            .iter()
            .filter(|device| device.state == ConnectionState::Connected)
            .count();
        let system_paired = system_devices
            .iter()
            .filter(|device| device.state == ConnectionState::Paired)
            .count();
        debug_log(&format!(
            "native system Bluetooth query returned {} device(s): {system_connected} connected, {system_paired} paired",
            system_devices.len(),
        ));
        for device in &system_devices {
            debug_log(&format!(
                "system device id={:?} name={:?} state={:?} kind={:?}",
                device.id, device.name, device.state, device.kind
            ));
        }
        let connected = devices
            .iter()
            .filter(|device| device.state == ConnectionState::Connected)
            .count();
        let paired = devices
            .iter()
            .filter(|device| device.state == ConnectionState::Paired)
            .count();
        debug_log(&format!(
            "publishing {} Bluetooth device(s): {connected} connected, {paired} paired",
            devices.len()
        ));
        Ok(devices)
    }

    async fn pair(&self, _id: &DeviceId) -> Result<(), BluetoothError> {
        Err(BluetoothError::Unsupported(
            "pairing is managed by the operating system",
        ))
    }

    async fn unpair(&self, _id: &DeviceId) -> Result<(), BluetoothError> {
        Err(BluetoothError::Unsupported(
            "unpairing is managed by the operating system",
        ))
    }

    async fn connect(&self, id: &DeviceId) -> Result<(), BluetoothError> {
        self.peripheral(id)
            .await?
            .connect()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))
    }

    async fn disconnect(&self, id: &DeviceId) -> Result<(), BluetoothError> {
        self.peripheral(id)
            .await?
            .disconnect()
            .await
            .map_err(|e| BluetoothError::Failed(e.to_string()))
    }
}

#[cfg(all(target_os = "macos", feature = "btleplug"))]
fn system_paired_devices() -> Vec<BluetoothDeviceInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_io_bluetooth::IOBluetoothDevice;

    // IOBluetoothDevice exposes macOS's paired-device registry and reports
    // whether each device currently has a baseband connection.
    autoreleasepool(|_| unsafe {
        let Some(paired) = IOBluetoothDevice::pairedDevices() else {
            return Vec::new();
        };
        paired
            .iter()
            .filter_map(|object| {
                let device = (&*object).downcast_ref::<IOBluetoothDevice>()?;
                let name = device
                    .nameOrAddress()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| "Bluetooth device".to_string());
                let address = device
                    .addressString()
                    .map(|address| address.to_string())
                    .unwrap_or_else(|| name.clone());
                Some(BluetoothDeviceInfo {
                    id: format!("system:macos:{}", address.to_ascii_lowercase()),
                    kind: classify_name(Some(&name)),
                    name: Some(name),
                    transport: Transport::Bluetooth,
                    state: if device.isConnected() {
                        ConnectionState::Connected
                    } else {
                        ConnectionState::Paired
                    },
                    rssi_dbm: None,
                    battery_percent: None,
                    services: Vec::new(),
                    trusted: true,
                })
            })
            .collect()
    })
}

#[cfg(all(target_os = "android", feature = "btleplug"))]
fn system_paired_devices() -> Vec<BluetoothDeviceInfo> {
    use jni::objects::JObject;

    let result = robius_android_env::with_activity(
        |env, activity| -> jni::errors::Result<Vec<_>> {
            let sdk = env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?;
            if sdk >= 31 {
                let permission = env.new_string("android.permission.BLUETOOTH_CONNECT")?;
                let granted = env
                    .call_method(
                        activity,
                        "checkSelfPermission",
                        "(Ljava/lang/String;)I",
                        &[(&permission).into()],
                    )?
                    .i()?
                    == 0;
                if !granted {
                    let string_class = env.find_class("java/lang/String")?;
                    let permission_array =
                        env.new_object_array(1, string_class, JObject::null())?;
                    env.set_object_array_element(&permission_array, 0, permission)?;
                    env.call_method(
                        activity,
                        "requestPermissions",
                        "([Ljava/lang/String;I)V",
                        &[(&permission_array).into(), 0x4f57.into()],
                    )?;
                    debug_log(
                        "requested Android BLUETOOTH_CONNECT permission for paired-device listing",
                    );
                    return Ok(Vec::new());
                }
            }

            let adapter = env
                .call_static_method(
                    "android/bluetooth/BluetoothAdapter",
                    "getDefaultAdapter",
                    "()Landroid/bluetooth/BluetoothAdapter;",
                    &[],
                )?
                .l()?;
            if adapter.is_null() {
                debug_log("Android BluetoothAdapter is unavailable");
                return Ok(Vec::new());
            }
            let bonded = env
                .call_method(&adapter, "getBondedDevices", "()Ljava/util/Set;", &[])?
                .l()?;
            if bonded.is_null() {
                return Ok(Vec::new());
            }
            let array = env
                .call_method(&bonded, "toArray", "()[Ljava/lang/Object;", &[])?
                .l()?;
            let array = jni::objects::JObjectArray::from(array);
            let count = env.get_array_length(&array)?;
            let mut devices = Vec::with_capacity(count as usize);
            for index in 0..count {
                let peer = env.get_object_array_element(&array, index)?;
                if peer.is_null() {
                    continue;
                }
                let name = env
                    .call_method(&peer, "getName", "()Ljava/lang/String;", &[])?
                    .l()?;
                let name = if name.is_null() {
                    None
                } else {
                    Some(
                        env.get_string(&jni::objects::JString::from(name))?
                            .to_string_lossy()
                            .into_owned(),
                    )
                };
                let address = env
                    .call_method(&peer, "getAddress", "()Ljava/lang/String;", &[])?
                    .l()?;
                let address = env
                    .get_string(&jni::objects::JString::from(address))?
                    .to_string_lossy()
                    .into_owned();
                let address_arg = env.new_string(&address)?;
                let class_loader = env
                    .call_method(
                        &activity,
                        "getClassLoader",
                        "()Ljava/lang/ClassLoader;",
                        &[],
                    )?
                    .l()?;
                let class_name = env
                    .new_string("com.nonpolynomial.btleplug.android.impl.BluetoothProfileQuery")?;
                let query_class = env
                    .call_method(
                        &class_loader,
                        "loadClass",
                        "(Ljava/lang/String;)Ljava/lang/Class;",
                        &[(&class_name).into()],
                    )?
                    .l()?;
                let query_class = jni::objects::JClass::from(query_class);
                let mut connected = false;
                for profile in [1_i32, 2_i32] {
                    connected = env
                        .call_static_method(
                            &query_class,
                            "isDeviceConnected",
                            "(Landroid/content/Context;Ljava/lang/String;I)Z",
                            &[(&activity).into(), (&address_arg).into(), profile.into()],
                        )?
                        .z()?;
                    if connected {
                        debug_log(&format!(
                            "Android Bluetooth device name={name:?} is connected on profile {profile}"
                        ));
                        break;
                    }
                }
                debug_log(&format!(
                    "Android bonded Bluetooth device name={name:?} address={address}"
                ));
                devices.push(BluetoothDeviceInfo {
                    id: format!("system:android:{}", address.to_ascii_lowercase()),
                    kind: classify_name(name.as_deref()),
                    name,
                    transport: Transport::Bluetooth,
                    state: if connected {
                        ConnectionState::Connected
                    } else {
                        ConnectionState::Paired
                    },
                    rssi_dbm: None,
                    battery_percent: None,
                    services: Vec::new(),
                    trusted: true,
                });
            }
            Ok(devices)
        },
    );

    match result {
        Ok(Ok(devices)) => devices,
        Ok(Err(error)) => {
            debug_log(&format!("Android paired-device query failed: {error}"));
            Vec::new()
        }
        Err(error) => {
            debug_log(&format!(
                "could not access Android Activity for paired-device query: {error}"
            ));
            Vec::new()
        }
    }
}

#[cfg(all(
    feature = "btleplug",
    not(target_os = "macos"),
    not(target_os = "android")
))]
fn system_paired_devices() -> Vec<BluetoothDeviceInfo> {
    Vec::new()
}

#[cfg(feature = "btleplug")]
fn sort_devices(devices: &mut [BluetoothDeviceInfo]) {
    devices.sort_by_key(|device| match &device.state {
        ConnectionState::Connected => 0,
        ConnectionState::Connecting => 1,
        ConnectionState::Paired => 2,
        ConnectionState::Discovered => 3,
        ConnectionState::Pairing => 4,
        ConnectionState::Reconnecting { .. } => 5,
        ConnectionState::Disconnected => 6,
        ConnectionState::Failed { .. } => 7,
    });
}

#[cfg(feature = "btleplug")]
fn classify_name(name: Option<&str>) -> DeviceKind {
    let lower = name.unwrap_or_default().to_ascii_lowercase();
    if lower.contains("keyboard") {
        DeviceKind::Keyboard
    } else if lower.contains("mouse") {
        DeviceKind::Mouse
    } else if lower.contains("gamepad") || lower.contains("controller") {
        DeviceKind::Gamepad
    } else {
        DeviceKind::Companion
    }
}

#[derive(Clone, Debug)]
pub enum BluetoothCommand {
    Scan,
    RefreshConnected,
    Pair(DeviceId),
    Unpair(DeviceId),
    Connect(DeviceId),
    Disconnect(DeviceId),
    Forget(DeviceId),
    StartHid,
    RetryStartHid,
    HidMouse {
        buttons: i8,
        dx: i8,
        dy: i8,
        wheel: i8,
    },
    HidKeyboard([u8; 8]),
    HidGamepad([u8; 7]),
    HidClick(i8),
    HidKey([u8; 8]),
    HidGamepadPress(u8),
    HidGamepadHat(u8),
}

#[derive(Clone, Debug)]
pub enum BluetoothEvent {
    ScanStarted,
    Devices(Vec<BluetoothDeviceInfo>),
    SystemDevices(Vec<BluetoothDeviceInfo>),
    DeviceUpdated(BluetoothDeviceInfo),
    ScanFinished,
    HidReady,
    HidHost(HidHostState),
    Error {
        device_id: Option<DeviceId>,
        message: String,
    },
}

pub struct BluetoothService {
    commands: mpsc::Sender<BluetoothCommand>,
    events: Arc<Mutex<mpsc::Receiver<BluetoothEvent>>>,
}

impl BluetoothService {
    pub fn spawn<A: BluetoothAdapter>(adapter: A) -> Self {
        let (command_tx, mut command_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(64);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                debug_log(&format!("service received command {command:?}"));
                let is_scan = matches!(&command, BluetoothCommand::Scan);
                let is_refresh = matches!(&command, BluetoothCommand::RefreshConnected);
                let is_start_hid = matches!(
                    &command,
                    BluetoothCommand::StartHid | BluetoothCommand::RetryStartHid
                );
                let is_hid_input = matches!(
                    &command,
                    BluetoothCommand::StartHid
                        | BluetoothCommand::RetryStartHid
                        | BluetoothCommand::HidMouse { .. }
                        | BluetoothCommand::HidKeyboard(_)
                        | BluetoothCommand::HidGamepad(_)
                        | BluetoothCommand::HidClick(_)
                        | BluetoothCommand::HidKey(_)
                        | BluetoothCommand::HidGamepadPress(_)
                        | BluetoothCommand::HidGamepadHat(_)
                );
                let result = match command {
                    BluetoothCommand::Scan => {
                        let _ = event_tx.send(BluetoothEvent::ScanStarted).await;
                        let result = adapter.scan().await;
                        if let Ok(devices) = &result {
                            debug_log(&format!("scan completed with {} device(s)", devices.len()));
                            let _ = event_tx
                                .send(BluetoothEvent::Devices(devices.clone()))
                                .await;
                        }
                        let _ = event_tx.send(BluetoothEvent::ScanFinished).await;
                        result.map(|_| ())
                    }
                    BluetoothCommand::RefreshConnected => match adapter.connected_devices().await {
                        Ok(devices) => {
                            let connected = devices
                                .iter()
                                .filter(|device| device.state == ConnectionState::Connected)
                                .count();
                            let paired = devices
                                .iter()
                                .filter(|device| device.state == ConnectionState::Paired)
                                .count();
                            debug_log(&format!(
                                "Bluetooth device refresh: {connected} connected, {paired} paired ({} total)",
                                devices.len(),
                            ));
                            let _ = event_tx.send(BluetoothEvent::SystemDevices(devices)).await;
                            match adapter.hid_host_state().await {
                                Ok(host_name) => {
                                    let _ = event_tx.send(BluetoothEvent::HidHost(host_name)).await;
                                }
                                Err(error) => {
                                    debug_log(&format!("HID host refresh failed: {error}"))
                                }
                            }
                            Ok(())
                        }
                        Err(error) => Err(error),
                    },
                    BluetoothCommand::Pair(id) => adapter.pair(&id).await,
                    BluetoothCommand::Unpair(id) => adapter.unpair(&id).await,
                    BluetoothCommand::Connect(id) => adapter.connect(&id).await,
                    BluetoothCommand::Disconnect(id) => adapter.disconnect(&id).await,
                    BluetoothCommand::Forget(_) => Ok(()),
                    BluetoothCommand::StartHid => adapter.start_hid().await,
                    BluetoothCommand::RetryStartHid => adapter.retry_hid().await,
                    BluetoothCommand::HidMouse {
                        buttons,
                        dx,
                        dy,
                        wheel,
                    } => adapter
                        .send_hid_mouse(buttons, dx, dy, wheel)
                        .await
                        .and_then(|delivered| {
                            if delivered {
                                Ok(())
                            } else {
                                Err(BluetoothError::Failed(
                                    "No connected Bluetooth HID host accepts mouse reports".into(),
                                ))
                            }
                        }),
                    BluetoothCommand::HidKeyboard(report) => adapter
                        .send_hid_keyboard(report)
                        .await
                        .and_then(|delivered| {
                            if delivered {
                                Ok(())
                            } else {
                                Err(BluetoothError::Failed(
                                    "No connected Bluetooth HID host accepts keyboard reports"
                                        .into(),
                                ))
                            }
                        }),
                    BluetoothCommand::HidGamepad(report) => adapter
                        .send_hid_gamepad(report)
                        .await
                        .and_then(|delivered| {
                            if delivered {
                                Ok(())
                            } else {
                                Err(BluetoothError::Failed(
                                    "No connected Bluetooth HID host accepts gamepad reports"
                                        .into(),
                                ))
                            }
                        }),
                    BluetoothCommand::HidClick(button) => {
                        debug_log(&format!("sending HID mouse click; buttons={button}"));
                        match adapter.send_hid_mouse(button, 0, 0, 0).await {
                            Ok(true) => {
                                // Give the host at least a few connection
                                // intervals to observe the pressed state before
                                // releasing it. Sending both reports back to
                                // back can collapse a tap into no click.
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                adapter.send_hid_mouse(0, 0, 0, 0).await.map(|_| ())
                            }
                            Ok(false) => Err(BluetoothError::Failed(
                                "HID mouse click report was not delivered".into(),
                            )),
                            Err(error) => Err(error),
                        }
                    }
                    BluetoothCommand::HidKey(report) => {
                        match adapter.send_hid_keyboard(report).await {
                            Ok(true) => {
                                // Keep the key-down report observable for at
                                // least one BLE connection interval before
                                // sending the release report.
                                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                adapter.send_hid_keyboard([0; 8]).await.map(|_| ())
                            }
                            Ok(false) => Err(BluetoothError::Failed(
                                "HID keyboard press report was not delivered".into(),
                            )),
                            Err(error) => Err(error),
                        }
                    }
                    BluetoothCommand::HidGamepadPress(button) => {
                        if button >= 16 {
                            Err(BluetoothError::Failed(
                                "Gamepad button index must be between 0 and 15".into(),
                            ))
                        } else {
                            let mut report = [0; 7];
                            report[(button / 8) as usize] = 1 << (button % 8);
                            report[2] = 0x0f; // centered hat switch
                            match adapter.send_hid_gamepad(report).await {
                                Ok(true) => {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                    adapter
                                        .send_hid_gamepad([0, 0, 0x0f, 0, 0, 0, 0])
                                        .await
                                        .map(|_| ())
                                }
                                Ok(false) => Err(BluetoothError::Failed(
                                    "HID gamepad press report was not delivered".into(),
                                )),
                                Err(error) => Err(error),
                            }
                        }
                    }
                    BluetoothCommand::HidGamepadHat(hat) => {
                        if hat > 7 {
                            Err(BluetoothError::Failed(
                                "Gamepad hat direction must be between 0 and 7".into(),
                            ))
                        } else {
                            let mut report = [0; 7];
                            report[2] = hat & 0x0f;
                            match adapter.send_hid_gamepad(report).await {
                                Ok(true) => {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                    adapter
                                        .send_hid_gamepad([0, 0, 0x0f, 0, 0, 0, 0])
                                        .await
                                        .map(|_| ())
                                }
                                Ok(false) => Err(BluetoothError::Failed(
                                    "HID gamepad direction report was not delivered".into(),
                                )),
                                Err(error) => Err(error),
                            }
                        }
                    }
                };
                let succeeded = result.is_ok();
                if let Err(error) = result {
                    debug_log(&format!("Bluetooth command failed: {error}"));
                    let _ = event_tx
                        .send(BluetoothEvent::Error {
                            device_id: None,
                            message: error.to_string(),
                        })
                        .await;
                } else if !is_scan && !is_refresh && !is_hid_input {
                    if let Ok(devices) = adapter.scan().await {
                        let _ = event_tx.send(BluetoothEvent::Devices(devices)).await;
                    }
                }
                if is_start_hid && succeeded {
                    let _ = event_tx.send(BluetoothEvent::HidReady).await;
                }
            }
        });
        Self {
            commands: command_tx,
            events: Arc::new(Mutex::new(event_rx)),
        }
    }

    pub fn command_sender(&self) -> mpsc::Sender<BluetoothCommand> {
        self.commands.clone()
    }

    pub async fn next_event(&self) -> Option<BluetoothEvent> {
        self.events.lock().await.recv().await
    }

    pub fn try_next_event(&self) -> Option<BluetoothEvent> {
        self.events.try_lock().ok()?.try_recv().ok()
    }
}

#[derive(Clone, Default)]
pub struct MockBluetoothAdapter {
    devices: Arc<Mutex<BTreeMap<DeviceId, BluetoothDeviceInfo>>>,
}

impl MockBluetoothAdapter {
    pub fn demo() -> Self {
        let devices = [
            ("phone", "Android companion", DeviceKind::Companion, -42),
            ("gamepad", "Wireless gamepad", DeviceKind::Gamepad, -57),
            ("keyboard", "Bluetooth keyboard", DeviceKind::Keyboard, -68),
        ]
        .into_iter()
        .map(|(id, name, kind, rssi)| {
            (
                id.into(),
                BluetoothDeviceInfo {
                    id: id.into(),
                    name: Some(name.into()),
                    kind,
                    transport: Transport::Bluetooth,
                    state: ConnectionState::Discovered,
                    rssi_dbm: Some(rssi),
                    battery_percent: None,
                    services: vec!["HID".into(), "Battery Service".into()],
                    trusted: false,
                },
            )
        })
        .collect();
        Self {
            devices: Arc::new(Mutex::new(devices)),
        }
    }
}

#[async_trait]
impl BluetoothAdapter for MockBluetoothAdapter {
    async fn scan(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError> {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        Ok(self.devices.lock().await.values().cloned().collect())
    }

    async fn connected_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError> {
        Ok(self
            .devices
            .lock()
            .await
            .values()
            .filter(|device| device.state == ConnectionState::Connected)
            .cloned()
            .collect())
    }

    async fn pair(&self, id: &DeviceId) -> Result<(), BluetoothError> {
        let mut devices = self.devices.lock().await;
        let device = devices
            .get_mut(id)
            .ok_or_else(|| BluetoothError::NotFound(id.clone()))?;
        device.trusted = true;
        device.state = ConnectionState::Paired;
        Ok(())
    }

    async fn unpair(&self, id: &DeviceId) -> Result<(), BluetoothError> {
        let mut devices = self.devices.lock().await;
        let device = devices
            .get_mut(id)
            .ok_or_else(|| BluetoothError::NotFound(id.clone()))?;
        device.trusted = false;
        device.state = ConnectionState::Disconnected;
        Ok(())
    }

    async fn connect(&self, id: &DeviceId) -> Result<(), BluetoothError> {
        let mut devices = self.devices.lock().await;
        let device = devices
            .get_mut(id)
            .ok_or_else(|| BluetoothError::NotFound(id.clone()))?;
        device.trusted = true;
        device.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&self, id: &DeviceId) -> Result<(), BluetoothError> {
        let mut devices = self.devices.lock().await;
        let device = devices
            .get_mut(id)
            .ok_or_else(|| BluetoothError::NotFound(id.clone()))?;
        device.state = ConnectionState::Disconnected;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default)]
    struct ReportRecordingAdapter {
        mouse: Arc<Mutex<Vec<(i8, i8, i8, i8)>>>,
        keyboard: Arc<Mutex<Vec<[u8; 8]>>>,
        gamepad: Arc<Mutex<Vec<[u8; 7]>>>,
        connected: bool,
    }

    #[async_trait]
    impl BluetoothAdapter for ReportRecordingAdapter {
        async fn scan(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError> {
            Ok(Vec::new())
        }
        async fn pair(&self, _: &DeviceId) -> Result<(), BluetoothError> {
            Ok(())
        }
        async fn unpair(&self, _: &DeviceId) -> Result<(), BluetoothError> {
            Ok(())
        }
        async fn connect(&self, _: &DeviceId) -> Result<(), BluetoothError> {
            Ok(())
        }
        async fn disconnect(&self, _: &DeviceId) -> Result<(), BluetoothError> {
            Ok(())
        }
        async fn send_hid_mouse(
            &self,
            buttons: i8,
            dx: i8,
            dy: i8,
            wheel: i8,
        ) -> Result<bool, BluetoothError> {
            self.mouse.lock().await.push((buttons, dx, dy, wheel));
            Ok(self.connected)
        }
        async fn send_hid_keyboard(&self, report: [u8; 8]) -> Result<bool, BluetoothError> {
            self.keyboard.lock().await.push(report);
            Ok(self.connected)
        }
        async fn send_hid_gamepad(&self, report: [u8; 7]) -> Result<bool, BluetoothError> {
            self.gamepad.lock().await.push(report);
            Ok(self.connected)
        }
    }

    #[tokio::test]
    async fn mock_adapter_supports_scan_pair_and_connect() {
        let adapter = MockBluetoothAdapter::demo();
        assert_eq!(adapter.scan().await.unwrap().len(), 3);
        assert!(adapter.connected_devices().await.unwrap().is_empty());
        adapter.pair(&"phone".into()).await.unwrap();
        adapter.connect(&"phone".into()).await.unwrap();
        assert_eq!(adapter.connected_devices().await.unwrap().len(), 1);
        assert_eq!(
            adapter
                .scan()
                .await
                .unwrap()
                .iter()
                .find(|d| d.id == "phone")
                .unwrap()
                .state,
            ConnectionState::Connected
        );
    }

    #[tokio::test]
    async fn mouse_click_sends_button_down_then_release_over_adapter() {
        let adapter = ReportRecordingAdapter {
            connected: true,
            ..Default::default()
        };
        let reports = Arc::clone(&adapter.mouse);
        let service = BluetoothService::spawn(adapter);
        service
            .command_sender()
            .send(BluetoothCommand::HidClick(4))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert_eq!(*reports.lock().await, vec![(4, 0, 0, 0), (0, 0, 0, 0)]);
    }

    #[tokio::test]
    async fn disconnected_mouse_report_produces_actionable_error() {
        let service = BluetoothService::spawn(ReportRecordingAdapter::default());
        service
            .command_sender()
            .send(BluetoothCommand::HidMouse {
                buttons: 0,
                dx: 3,
                dy: -2,
                wheel: 0,
            })
            .await
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), service.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            BluetoothEvent::Error { message, .. }
                if message.contains("No connected Bluetooth HID host")
        ));
    }
    #[test]
    fn bluetooth_connection_alone_never_claims_hid_input_is_ready() {
        let mut host = HidHostState {
            name: Some("Mac".into()),
            report_mask: 0,
        };
        assert!(!host.mouse_ready());
        assert!(!host.keyboard_ready());
        assert!(host.label().unwrap().contains("waiting"));
        host.report_mask = 1;
        assert!(host.mouse_ready());
        assert!(!host.keyboard_ready());
        host.report_mask = 3;
        assert!(host.mouse_ready() && host.keyboard_ready());
        host.name = None;
        assert!(!host.mouse_ready() && !host.keyboard_ready());
    }

    fn device(id: &str, name: Option<&str>, state: ConnectionState) -> BluetoothDeviceInfo {
        BluetoothDeviceInfo {
            id: id.into(),
            name: name.map(str::to_owned),
            kind: DeviceKind::Companion,
            transport: Transport::Bluetooth,
            state,
            rssi_dbm: None,
            battery_percent: None,
            services: Vec::new(),
            trusted: false,
        }
    }

    #[test]
    fn paired_and_scan_views_merge_by_address_without_losing_live_connection() {
        let mut devices = vec![device(
            "AA:BB:CC:DD:EE:FF",
            Some("RP5"),
            ConnectionState::Connected,
        )];
        merge_devices(
            &mut devices,
            vec![device(
                "system:aa-bb-cc-dd-ee-ff",
                Some("RP5"),
                ConnectionState::Paired,
            )],
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "AA:BB:CC:DD:EE:FF");
        assert_eq!(devices[0].state, ConnectionState::Connected);
        merge_devices(
            &mut devices,
            vec![device(
                "AA:BB:CC:DD:EE:FF",
                None,
                ConnectionState::Discovered,
            )],
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name.as_deref(), Some("RP5"));
    }

    #[test]
    fn same_name_and_unnamed_devices_keep_distinct_identities() {
        let mut devices = vec![device("one", Some("RP5"), ConnectionState::Discovered)];
        merge_devices(
            &mut devices,
            vec![
                device("two", Some("RP5"), ConnectionState::Discovered),
                device("three", None, ConnectionState::Paired),
                device("four", None, ConnectionState::Paired),
            ],
        );
        assert_eq!(devices.len(), 4);
    }

    #[tokio::test]
    async fn movement_reports_reach_the_adapter_in_order_with_signed_deltas() {
        let adapter = ReportRecordingAdapter {
            connected: true,
            ..Default::default()
        };
        let reports = Arc::clone(&adapter.mouse);
        let service = BluetoothService::spawn(adapter);
        for (dx, dy) in [(15, 3), (-8, -5), (127, -127)] {
            service
                .command_sender()
                .send(BluetoothCommand::HidMouse {
                    buttons: 0,
                    dx,
                    dy,
                    wheel: 0,
                })
                .await
                .unwrap();
        }
        // A refresh is a barrier after all input commands; do not rely on sleeps.
        service
            .command_sender()
            .send(BluetoothCommand::RefreshConnected)
            .await
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), service.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, BluetoothEvent::SystemDevices(_)));
        assert_eq!(
            *reports.lock().await,
            vec![(0, 15, 3, 0), (0, -8, -5, 0), (0, 127, -127, 0)]
        );
    }

    #[tokio::test]
    async fn gamepad_button_press_sends_button_bit_then_neutral_release() {
        let adapter = ReportRecordingAdapter {
            connected: true,
            ..Default::default()
        };
        let reports = Arc::clone(&adapter.gamepad);
        let service = BluetoothService::spawn(adapter);

        for button in [0, 7, 8, 15] {
            service
                .command_sender()
                .send(BluetoothCommand::HidGamepadPress(button))
                .await
                .unwrap();
        }
        service
            .command_sender()
            .send(BluetoothCommand::RefreshConnected)
            .await
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), service.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, BluetoothEvent::SystemDevices(_)));

        let reports = reports.lock().await;
        assert_eq!(reports.len(), 8);
        for (pair, expected) in reports.chunks_exact(2).zip([
            [1, 0, 0x0f, 0, 0, 0, 0],
            [0x80, 0, 0x0f, 0, 0, 0, 0],
            [0, 1, 0x0f, 0, 0, 0, 0],
            [0, 0x80, 0x0f, 0, 0, 0, 0],
        ]) {
            assert_eq!(pair[0], expected);
            assert_eq!(pair[1], [0, 0, 0x0f, 0, 0, 0, 0]);
        }
    }

    #[tokio::test]
    async fn gamepad_hat_press_sends_direction_then_neutral_release() {
        let adapter = ReportRecordingAdapter {
            connected: true,
            ..Default::default()
        };
        let reports = Arc::clone(&adapter.gamepad);
        let service = BluetoothService::spawn(adapter);

        for direction in [0, 2, 4, 6] {
            service
                .command_sender()
                .send(BluetoothCommand::HidGamepadHat(direction))
                .await
                .unwrap();
        }
        service
            .command_sender()
            .send(BluetoothCommand::RefreshConnected)
            .await
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), service.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, BluetoothEvent::SystemDevices(_)));

        let reports = reports.lock().await;
        assert_eq!(reports.len(), 8);
        for (pair, direction) in reports.chunks_exact(2).zip([0, 2, 4, 6]) {
            assert_eq!(pair[0], [0, 0, direction, 0, 0, 0, 0]);
            assert_eq!(pair[1], [0, 0, 0x0f, 0, 0, 0, 0]);
        }
    }

    #[tokio::test]
    async fn invalid_gamepad_input_is_rejected_without_sending_reports() {
        let adapter = ReportRecordingAdapter {
            connected: true,
            ..Default::default()
        };
        let reports = Arc::clone(&adapter.gamepad);
        let service = BluetoothService::spawn(adapter);
        for command in [
            BluetoothCommand::HidGamepadPress(16),
            BluetoothCommand::HidGamepadHat(8),
        ] {
            service.command_sender().send(command).await.unwrap();
            let event =
                tokio::time::timeout(std::time::Duration::from_secs(1), service.next_event())
                    .await
                    .unwrap()
                    .unwrap();
            assert!(matches!(event, BluetoothEvent::Error { .. }));
        }
        assert!(reports.lock().await.is_empty());
    }
}
