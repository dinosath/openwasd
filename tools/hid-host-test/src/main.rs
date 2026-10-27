use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::{Adapter, Manager, Peripheral as PlatformPeripheral};
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::time::{Instant, sleep, timeout};
use uuid::Uuid;

const HID_SERVICE: Uuid = Uuid::from_u128(0x0000_1812_0000_1000_8000_0080_5f9b_34fb);

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
    fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
    fn CGEventSourceKeyState(state_id: i32, keycode: u16) -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[tokio::main]
async fn main() {
    match run_test().await {
        Ok(()) => println!("TEST_RESULT: PASS"),
        Err(error) => {
            eprintln!("TEST_RESULT: FAIL: {error}");
            std::process::exit(1);
        }
    }
}

async fn run_test() -> Result<(), Box<dyn std::error::Error>> {
    let log = android_hid_log()?;
    if log.contains("System HID session starting") {
        wait_for_android_hid_host()?;
        verify_system_identity()?;
        return run_host_input_test();
    }
    println!("Creating CoreBluetooth manager with btleplug...");
    let manager = timeout(Duration::from_secs(15), Manager::new())
            .await
            .map_err(|_| {
                "CoreBluetooth initialization timed out; grant OpenWASD HID Host Test Bluetooth access in macOS Privacy & Security and rerun"
            })?
            .map_err(|error| format!("Create CoreBluetooth manager: {error:?}"))?;
    let adapters = timeout(Duration::from_secs(20), manager.adapters())
            .await
            .map_err(|_| "CoreBluetooth did not finish adapter initialization within 20 seconds. Check macOS Bluetooth permission and that Bluetooth is enabled.")?
            .map_err(|error| format!("Enumerate Bluetooth adapters: {error:?}"))?;
    let adapter = adapters
        .first()
        .ok_or("No Bluetooth adapter found on this Mac")?;
    println!("Bluetooth adapter: {:?}", adapter.adapter_info().await?);
    if android_hid_input_ready()? {
        println!(
            "HOST_CHECK: BLE scan skipped; the existing Android HID host connection is already active"
        );
    } else {
        let _peripheral = timeout(
            Duration::from_secs(90),
            scan_for_hid(adapter, Duration::from_secs(30)),
        )
        .await
        .map_err(|_| "CoreBluetooth BLE HID scan did not complete within 90 seconds")??;
    }
    println!("Running Android input actions and checking macOS HID events...");
    run_host_input_test()?;
    Ok(())
}

fn android_hid_log() -> Result<String, Box<dyn std::error::Error>> {
    let adb = std::env::var("ADB").unwrap_or_else(|_| "adb".to_owned());
    let output = Command::new(adb)
        .args(["logcat", "-d", "-s", "OpenWASD-HID:D"])
        .output()?;
    if !output.status.success() {
        return Err("Could not read Android HID log".into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn native_identity_counts(json: &serde_json::Value, name: &str) -> (usize, usize) {
    let mut counts = (0, 0);
    if let Some(controllers) = json["SPBluetoothDataType"].as_array() {
        for controller in controllers {
            for (key, count) in [
                ("device_connected", &mut counts.0),
                ("device_not_connected", &mut counts.1),
            ] {
                if let Some(devices) = controller[key].as_array() {
                    *count += devices
                        .iter()
                        .filter(|device| device.get(name).is_some())
                        .count();
                }
            }
        }
    }
    counts
}

fn verify_system_identity() -> Result<(), Box<dyn std::error::Error>> {
    let name = std::env::var("OPENWASD_HID_DEVICE_NAME").unwrap_or_else(|_| "RP5".into());
    let output = Command::new("system_profiler")
        .args(["SPBluetoothDataType", "-json"])
        .output()?;
    if !output.status.success() {
        return Err("Could not inspect macOS Bluetooth identities".into());
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let (connected, disconnected) = native_identity_counts(&json, &name);
    if connected != 1 || disconnected != 0 {
        return Err(format!("Expected one connected {name} identity and no duplicate saved identity; connected={connected}, disconnected={disconnected}").into());
    }
    let log = android_hid_log()?;
    let session = log
        .rsplit("System HID session starting")
        .next()
        .unwrap_or(&log);
    if session.contains("Starting connectable BLE HID advertising") {
        return Err("System HID must not start a second BLE advertiser".into());
    }
    println!("HOST_CHECK: unique system Bluetooth HID device count=1 PASS");
    Ok(())
}

fn android_hid_input_ready() -> Result<bool, Box<dyn std::error::Error>> {
    let adb = std::env::var("ADB").unwrap_or_else(|_| "adb".to_owned());
    let output = Command::new(adb)
        .args(["logcat", "-d", "-s", "OpenWASD-HID:I"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Could not read Android HID status from logcat: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(hid_input_ready(&String::from_utf8_lossy(&output.stdout)) == (true, true, true))
}

async fn scan_for_hid(
    adapter: &Adapter,
    scan_timeout: Duration,
) -> Result<PlatformPeripheral, Box<dyn std::error::Error>> {
    let target_name = std::env::var("OPENWASD_HID_DEVICE_NAME").unwrap_or_else(|_| "RP5".into());
    println!("Looking for the configured Android HID device name {target_name:?}...");
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(|error| format!("Start BLE scan (check macOS Bluetooth permission): {error:?}"))?;
    let scan_deadline = Instant::now() + scan_timeout;
    let mut observed: BTreeMap<String, (String, Option<i16>)> = BTreeMap::new();
    let mut selected = None;
    let mut observe_until = None;
    let result: Result<PlatformPeripheral, Box<dyn std::error::Error>> = 'scan: loop {
        for candidate in adapter.peripherals().await? {
            let Some(properties) = candidate.properties().await? else {
                continue;
            };
            if properties.services.contains(&HID_SERVICE) {
                let name = properties.local_name.clone().unwrap_or_default();
                if name != target_name {
                    println!(
                        "Ignoring unrelated HID advertisement name={name:?} rssi={:?}",
                        properties.rssi
                    );
                    continue;
                }
                // CoreBluetooth intentionally hides the BLE address, so use
                // btleplug's platform peripheral identity to distinguish live
                // advertisements from the specifically selected Android host.
                // Other HID devices may be nearby and must not make this test
                // appear to have duplicate OpenWASD advertisers.
                let id = format!("{:?}", candidate.id());
                observed
                    .entry(id)
                    .or_insert_with(|| (name, properties.rssi));
                selected = Some(candidate);
                observe_until.get_or_insert_with(|| Instant::now() + Duration::from_secs(2));
            }
        }
        if observe_until.is_some_and(|deadline| Instant::now() >= deadline) {
            let matches: Vec<_> = observed.iter().collect();
            println!("Observed {} distinct BLE HID peripheral(s):", matches.len());
            for (id, (name, rssi)) in &matches {
                println!("  id={id} name={name:?} rssi={rssi:?}");
            }
            if matches.len() != 1 {
                break Err(format!(
                    "Expected exactly one visible BLE HID peripheral, found {}. macOS may retain stale RP5 rows from earlier BLE addresses; clear those entries and retry.",
                    matches.len()
                ).into());
            }
            println!("HOST_CHECK: unique BLE HID device count=1 PASS");
            break 'scan Ok(selected.take().expect("selected was checked"));
        }
        if Instant::now() >= scan_deadline {
            break Err(format!(
                "No BLE HID (0x1812) advertisement found within {} seconds",
                scan_timeout.as_secs()
            )
            .into());
        }
        sleep(Duration::from_millis(400)).await;
    };
    let stop_result = adapter.stop_scan().await;
    if let Err(error) = stop_result {
        if result.is_ok() {
            return Err(format!("Stop BLE scan: {error:?}").into());
        }
    }
    result.map_err(|error| format!("btleplug BLE HID scan failed: {error}").into())
}

fn run_host_input_test() -> Result<(), Box<dyn std::error::Error>> {
    wait_for_android_hid_host()?;
    #[cfg(target_os = "macos")]
    let _safe_target = HidTestWindow::show()?;
    let initial = pointer_location();
    let observed = Arc::new(Mutex::new(HostEvents::default()));
    let watch = Arc::clone(&observed);
    let keep_watching = Arc::new(AtomicBool::new(true));
    let watcher_flag = Arc::clone(&keep_watching);
    let observer = thread::spawn(move || {
        let mut previous = initial;
        while watcher_flag.load(Ordering::Relaxed) {
            let position = pointer_location();
            let mut events = watch.lock().expect("event monitor mutex");
            if (position.x - previous.x).abs() > 1.0 || (position.y - previous.y).abs() > 1.0 {
                events.pointer_moved = true;
                previous = position;
            }
            for button in 0..3 {
                if button_state(button) {
                    events.buttons |= 1 << button;
                }
            }
            if key_state(0) {
                events.a_key = true;
            }
            drop(events);
            thread::sleep(Duration::from_millis(1));
        }
    });

    let drive_result = drive_android_ui();
    thread::sleep(Duration::from_millis(300));
    keep_watching.store(false, Ordering::Relaxed);
    observer.join().expect("event observer thread");
    drive_result?;
    let events = observed.lock().expect("event monitor mutex").clone();

    let adb = std::env::var("ADB").unwrap_or_else(|_| "adb".to_owned());
    let output = Command::new(adb)
        .args(["logcat", "-d", "-s", "OpenWASD-HID:D"])
        .output()?;
    let logs = String::from_utf8_lossy(&output.stdout);
    let acknowledged: Vec<_> = logs
        .lines()
        .filter(|line| {
            (line.contains("HID BLE report acknowledged") && line.contains("status=0"))
                || line.contains("HID system report queued")
        })
        .filter_map(parse_acknowledged_report)
        .collect();
    let mouse = acknowledged.iter().any(|(id, payload)| {
        *id == 1 && payload.len() == 4 && payload[1..].iter().any(|byte| *byte != 0)
    });
    let keyboard_down = acknowledged
        .iter()
        .any(|(id, payload)| *id == 2 && payload.len() == 8 && payload[2] == 4);
    let keyboard_up = acknowledged
        .iter()
        .any(|(id, payload)| *id == 2 && payload == &[0; 8]);
    let keyboard = keyboard_down && keyboard_up;
    let buttons = [1, 4, 2]
        .iter()
        .all(|button| has_mouse_click_pair(&acknowledged, *button));
    if mouse && buttons && keyboard {
        println!("HOST_CHECK: Android HID mouse/button/keyboard reports submitted PASS");
    } else {
        println!(
            "HOST_CHECK: Android HID report submission FAIL (mouse_movement={mouse}, mouse_buttons={buttons}, keyboard_A={keyboard})"
        );
    }

    println!("Mac HID events: {events:?}");
    // Ignore unrelated physical pointer/key activity unless Android actually
    // received and acknowledged the corresponding BLE report in this run.
    let pointer_moved = mouse && events.pointer_moved;
    let mouse_buttons = buttons && events.buttons & 0b111 == 0b111;
    let keyboard_a = keyboard && events.a_key;
    if pointer_moved {
        println!("HOST_CHECK: macOS pointer moved PASS");
    } else {
        println!("HOST_CHECK: macOS pointer moved FAIL");
    }
    if mouse_buttons {
        println!("HOST_CHECK: macOS left/middle/right buttons registered PASS");
    } else {
        println!(
            "HOST_CHECK: macOS left/middle/right buttons registered FAIL (observed mask={:#05b})",
            events.buttons
        );
    }
    if keyboard_a {
        println!("HOST_CHECK: macOS keyboard A registered PASS");
    } else {
        println!("HOST_CHECK: macOS keyboard A registered FAIL");
    }
    if !(mouse && buttons && keyboard && pointer_moved && mouse_buttons && keyboard_a) {
        return Err("One or more Android BLE acknowledgement or macOS HID event checks failed. See the per-check results and android-logcat.txt in the test artifacts.".into());
    }
    Ok(())
}

fn has_mouse_click_pair(reports: &[(u8, Vec<u8>)], button: u8) -> bool {
    let pressed = reports
        .iter()
        .position(|(id, payload)| *id == 1 && payload.first() == Some(&button));
    pressed.is_some_and(|pressed| {
        reports
            .iter()
            .skip(pressed + 1)
            .any(|(id, payload)| *id == 1 && payload.first() == Some(&0))
    })
}

/// Do not inject UI events while Android is only advertising. In that state
/// touch and key presses are correctly rejected by the HID server, which used
/// to make the test report a generic input failure without identifying the
/// missing host connection.
fn wait_for_android_hid_host() -> Result<(), Box<dyn std::error::Error>> {
    let adb = std::env::var("ADB").unwrap_or_else(|_| "adb".to_owned());
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut last_notice = Instant::now() - Duration::from_secs(10);
    loop {
        let output = Command::new(&adb)
            .args(["logcat", "-d", "-s", "OpenWASD-HID:I"])
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "Could not read Android HID status from logcat: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
        let log = String::from_utf8_lossy(&output.stdout);
        let (connected, mouse_subscribed, keyboard_subscribed) = hid_input_ready(&log);
        if connected && mouse_subscribed && keyboard_subscribed {
            println!(
                "HOST_CHECK: Android HID host connected with mouse and keyboard reports subscribed PASS"
            );
            return Ok(());
        }
        if last_notice.elapsed() >= Duration::from_secs(10) {
            println!(
                "Waiting for macOS to connect to the Android HID device and subscribe to mouse and keyboard reports..."
            );
            last_notice = Instant::now();
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Android has no active HID host with both reports subscribed after 90 seconds (connected={connected}, mouse_subscribed={mouse_subscribed}, keyboard_subscribed={keyboard_subscribed}). Approve the Mac pairing request if shown, then connect to OpenWASD in Bluetooth settings. The test left the pairing bond intact."
            )
            .into());
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Replay only the current server session and each host's current subscriptions.
/// Historical subscriptions are not evidence that this connection accepts input.
fn hid_input_ready(log: &str) -> (bool, bool, bool) {
    let mut hosts: BTreeMap<String, (bool, bool, bool)> = BTreeMap::new();
    for line in log.lines() {
        if line.contains("BLE HID session starting")
            || line.contains("BLE HID session stopped")
            || line.contains("System HID session starting")
            || line.contains("System HID registration stopped")
            || line.contains("System HID service disconnected")
        {
            hosts.clear();
            continue;
        }
        let Some(rest) = line.split("host=").nth(1) else {
            continue;
        };
        let end = [" address=", " reportId=", " mask=", " status=", " state="]
            .iter()
            .filter_map(|separator| rest.find(separator))
            .min()
            .unwrap_or(rest.len());
        let name = &rest[..end];
        if line.contains("System HID host connection changed") {
            if line.contains("state=2") {
                hosts.insert(name.into(), (true, true, true));
            } else if line.contains("state=0") {
                hosts.remove(name);
            }
        } else if line.contains("GATT host connection changed") {
            if line.contains("status=0") && line.contains("state=2") {
                hosts.insert(name.into(), (true, false, false));
            } else if line.contains("state=0") {
                hosts.remove(name);
            }
        } else if let Some(state) = hosts.get_mut(name) {
            if line.contains("HID report subscriptions restored") {
                let mask = line
                    .split("mask=")
                    .nth(1)
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(0);
                state.1 = mask & 1 != 0;
                state.2 = mask & 2 != 0;
            } else if line.contains("HID report subscription changed") {
                if line.contains("reportId=1 ") {
                    state.1 = line.contains("enabled=true");
                }
                if line.contains("reportId=2 ") {
                    state.2 = line.contains("enabled=true");
                }
            }
        }
    }
    hosts
        .values()
        .copied()
        .find(|s| *s == (true, true, true))
        .or_else(|| hosts.values().copied().next())
        .unwrap_or((false, false, false))
}

fn parse_acknowledged_report(line: &str) -> Option<(u8, Vec<u8>)> {
    let report_id = line
        .split("reportId=")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let payload = line.split("payload=").nth(1)?;
    let bytes = payload
        .split_whitespace()
        .take(if report_id == 1 { 4 } else { 8 })
        .map(|byte| u8::from_str_radix(byte, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    Some((report_id, bytes))
}

#[cfg(target_os = "macos")]
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[cfg(target_os = "macos")]
struct HidTestWindow(objc2::rc::Retained<objc2::runtime::AnyObject>);
#[cfg(target_os = "macos")]
impl HidTestWindow {
    fn show() -> Result<Self, Box<dyn std::error::Error>> {
        use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
        use objc2_foundation::{NSRect, NSString};
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let _: bool = msg_send![app,setActivationPolicy:0isize];
            let _: () = msg_send![app, finishLaunching];
            let screen: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
            let frame: NSRect = msg_send![screen, visibleFrame];
            let window: *mut AnyObject = msg_send![class!(NSWindow), alloc];
            let window: *mut AnyObject = msg_send![window, initWithContentRect: frame, styleMask: 1usize, backing: 2usize, defer: false];
            let window =
                Retained::from_raw(window).ok_or("Could not create safe HID test target")?;
            let _: () = msg_send![&*window,setReleasedWhenClosed:false];
            let title = NSString::from_str("OpenWASD HID input test — temporary blank target");
            let _: () = msg_send![&*window,setTitle:&*title];
            let _: () = msg_send![&*window,makeKeyAndOrderFront:std::ptr::null::<AnyObject>()];
            let _: () = msg_send![app,activateIgnoringOtherApps:true];
            let _: () = msg_send![app, updateWindows];
            Ok(Self(window))
        }
    }
}
#[cfg(target_os = "macos")]
impl Drop for HidTestWindow {
    fn drop(&mut self) {
        unsafe {
            let _: () =
                objc2::msg_send![&*self.0,orderOut:std::ptr::null::<objc2::runtime::AnyObject>()];
        }
    }
}

#[derive(Clone, Debug, Default)]
struct HostEvents {
    pointer_moved: bool,
    buttons: u8,
    a_key: bool,
}

fn pointer_location() -> CGPoint {
    #[cfg(target_os = "macos")]
    unsafe {
        let event = CGEventCreate(std::ptr::null_mut());
        if event.is_null() {
            return CGPoint { x: 0.0, y: 0.0 };
        }
        let location = CGEventGetLocation(event);
        CFRelease(event);
        location
    }
    #[cfg(not(target_os = "macos"))]
    CGPoint { x: 0.0, y: 0.0 }
}

fn button_state(button: u32) -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        CGEventSourceButtonState(0, button)
    }
    #[cfg(not(target_os = "macos"))]
    false
}

fn key_state(keycode: u16) -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        CGEventSourceKeyState(0, keycode)
    }
    #[cfg(not(target_os = "macos"))]
    false
}

fn drive_android_ui() -> Result<(), Box<dyn std::error::Error>> {
    let adb = std::env::var("ADB").unwrap_or_else(|_| "adb".to_owned());
    let screenshot_path = std::env::var_os("OPENWASD_TEST_OUTPUT")
        .map(std::path::PathBuf::from)
        .map(|dir| dir.join("01-app-before-test.png"));
    let dimensions = screenshot_path
        .as_ref()
        .and_then(|path| fs::read(path).ok())
        .and_then(|png| png_dimensions(&png))
        .unwrap_or((1920, 1080));
    let (width, height) = (dimensions.0 as i32, dimensions.1 as i32);
    let tap = |x: u32, y: u32| -> Result<(), String> {
        let output = Command::new(&adb)
            .args(["shell", "input", "tap"])
            .arg((width * x as i32 / 100).to_string())
            .arg((height * y as i32 / 100).to_string())
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    };
    let swipe = Command::new(&adb)
        .args(["shell", "input", "swipe"])
        .arg((width * 30 / 100).to_string())
        .arg((height * 50 / 100).to_string())
        .arg((width * 43 / 100).to_string())
        .arg((height * 59 / 100).to_string())
        .arg("650")
        .output()?;
    if !swipe.status.success() {
        return Err(format!(
            "ADB touchpad swipe failed: {}",
            String::from_utf8_lossy(&swipe.stderr)
        )
        .into());
    }
    let reverse = Command::new(&adb)
        .args(["shell", "input", "swipe"])
        .arg((width * 43 / 100).to_string())
        .arg((height * 59 / 100).to_string())
        .arg((width * 30 / 100).to_string())
        .arg((height * 50 / 100).to_string())
        .arg("650")
        .output()?;
    if !reverse.status.success() {
        return Err("ADB reverse touchpad swipe failed".into());
    }
    thread::sleep(Duration::from_millis(300));
    for x in [18, 50, 82] {
        tap(x, 84)?;
        thread::sleep(Duration::from_millis(250));
    }
    tap(88, 20)?;
    thread::sleep(Duration::from_millis(700));
    if let Some(path) = std::env::var_os("OPENWASD_TEST_OUTPUT")
        .map(std::path::PathBuf::from)
        .map(|dir| dir.join("03-keyboard-screen.png"))
    {
        let screenshot = Command::new(&adb)
            .args(["exec-out", "screencap", "-p"])
            .output()?;
        if screenshot.status.success() {
            fs::write(path, screenshot.stdout)?;
        }
    }
    // The home row starts with Caps, then A; use the visible row in either
    // portrait or landscape screenshots instead of the QWERTY row above it.
    tap(16, 78)?;
    thread::sleep(Duration::from_millis(1000));
    Ok(())
}

fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes(png.get(16..20)?.try_into().ok()?),
        u32::from_be_bytes(png.get(20..24)?.try_into().ok()?),
    ))
}

#[cfg(test)]
mod tests {
    use super::{has_mouse_click_pair, hid_input_ready, parse_acknowledged_report};

    #[test]
    fn requires_live_host_and_both_hid_report_subscriptions() {
        let log = "I OpenWASD-HID: GATT host connection changed; host=Mac status=0 state=2\n\
            I OpenWASD-HID: HID report subscription changed; host=Mac reportId=1 enabled=true\n\
            I OpenWASD-HID: HID report subscription changed; host=Mac reportId=2 enabled=true";
        assert_eq!(hid_input_ready(log), (true, true, true));
        assert_eq!(
            hid_input_ready(
                "I OpenWASD-HID: GATT host connection changed; host=Mac status=0 state=2"
            ),
            (true, false, false)
        );
    }

    #[test]
    fn parses_mouse_button_report_acknowledgement() {
        assert_eq!(
            parse_acknowledged_report(
                "D OpenWASD-HID: HID BLE report acknowledged; host=Mac report=reportId=1 payload=04 00 00 00 status=0"
            ),
            Some((1, vec![4, 0, 0, 0]))
        );
    }

    #[test]
    fn parses_keyboard_usage_report_acknowledgement() {
        assert_eq!(
            parse_acknowledged_report(
                "D OpenWASD-HID: HID BLE report acknowledged; host=Mac report=reportId=2 payload=00 00 04 00 00 00 00 00 status=0"
            ),
            Some((2, vec![0, 0, 4, 0, 0, 0, 0, 0]))
        );
    }

    #[test]
    fn requires_button_down_and_release_reports_for_each_click() {
        let reports = vec![
            (1, vec![1, 0, 0, 0]),
            (1, vec![0, 0, 0, 0]),
            (1, vec![4, 0, 0, 0]),
            (1, vec![0, 0, 0, 0]),
            (1, vec![2, 0, 0, 0]),
            (1, vec![0, 0, 0, 0]),
        ];
        assert!(has_mouse_click_pair(&reports, 1));
        assert!(has_mouse_click_pair(&reports, 4));
        assert!(has_mouse_click_pair(&reports, 2));
        assert!(!has_mouse_click_pair(&reports[..1], 1));
    }
    #[test]
    fn disconnected_or_restarted_sessions_cannot_pass_from_old_subscriptions() {
        let ready = "BLE HID session starting\nGATT host connection changed; host=Mac address=AA status=0 state=2\nHID report subscriptions restored; host=Mac mask=3\n";
        assert_eq!(hid_input_ready(ready), (true, true, true));
        assert_eq!(
            hid_input_ready(&format!(
                "{ready}GATT host connection changed; host=Mac address=AA status=0 state=0"
            )),
            (false, false, false)
        );
        assert_eq!(
            hid_input_ready(&format!(
                "{ready}BLE HID session starting\nGATT host connection changed; host=Mac address=AA status=0 state=2"
            )),
            (true, false, false)
        );
        assert_eq!(
            hid_input_ready(&format!(
                "{ready}HID report subscription changed; host=Mac reportId=1 enabled=false"
            )),
            (true, false, true)
        );
    }

    #[test]
    fn subscriptions_from_different_hosts_do_not_combine_into_readiness() {
        let log = "GATT host connection changed; host=One status=0 state=2\nGATT host connection changed; host=Two status=0 state=2\nHID report subscription changed; host=One reportId=1 enabled=true\nHID report subscription changed; host=Two reportId=2 enabled=true";
        assert_ne!(hid_input_ready(log), (true, true, true));
    }
    #[test]
    fn native_hid_link_is_ready_only_until_disconnect() {
        let log = "System HID session starting\nSystem HID host connection changed; host=Mac state=1\nSystem HID host connection changed; host=Mac state=2\n";
        assert_eq!(hid_input_ready(log), (true, true, true));
        assert_eq!(
            hid_input_ready(&format!(
                "{log}System HID host connection changed; host=Mac state=0"
            )),
            (false, false, false)
        );
    }
    #[test]
    fn rejects_duplicate_saved_native_identities() {
        let json = serde_json::json!({"SPBluetoothDataType":[{"device_connected":[{"RP5":{}}],"device_not_connected":[{"RP5":{}},{"Other":{}}]}]});
        assert_eq!(super::native_identity_counts(&json, "RP5"), (1, 1));
        assert_eq!(super::native_identity_counts(&json, "Absent"), (0, 0));
    }
}
