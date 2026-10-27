use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PACKAGE: &str = "dev.makepad.openwasd";
const HID_SERVICE: &str = "00001812-0000-1000-8000-00805f9b34fb";

#[derive(Default)]
struct Results {
    android_app: String,
    mac_host: String,
    host_unique_device: String,
    host_ble_reports: String,
    host_pointer: String,
    host_buttons: String,
    host_keyboard: String,
    host_reconnect: String,
}

struct DeviceState {
    adb: PathBuf,
    output: PathBuf,
    app_started: bool,
    old_accelerometer_rotation: Option<String>,
    old_user_rotation: Option<String>,
    old_stay_awake: Option<String>,
}

impl DeviceState {
    fn new(adb: PathBuf, output: PathBuf) -> Self {
        Self {
            adb,
            output,
            app_started: false,
            old_accelerometer_rotation: None,
            old_user_rotation: None,
            old_stay_awake: None,
        }
    }

    fn adb(&self, args: &[&str]) -> io::Result<Output> {
        let mut last_output = None;
        for attempt in 0..5 {
            let output = Command::new(&self.adb).args(args).output()?;
            let transient_disconnect = !output.status.success()
                && (String::from_utf8_lossy(&output.stderr).contains("no devices/emulators found")
                    || String::from_utf8_lossy(&output.stderr).contains("device offline")
                    || String::from_utf8_lossy(&output.stderr).contains("error: closed"));
            if !transient_disconnect || attempt == 4 {
                return Ok(output);
            }
            last_output = Some(output);
            sleep(Duration::from_millis(700));
        }
        Ok(last_output.expect("the retry loop runs at least once"))
    }

    fn adb_ok(&self, args: &[&str]) -> Result<Output, String> {
        let output = self
            .adb(args)
            .map_err(|error| format!("Could not run ADB {:?}: {error}", args))?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(format!(
                "ADB {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    fn adb_text(&self, args: &[&str]) -> Result<String, String> {
        self.adb_ok(args)
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn capture_screenshot(&self, name: &str) -> Result<PathBuf, String> {
        let output = self.adb_ok(&["exec-out", "screencap", "-p"])?;
        let path = self.output.join(name);
        fs::write(&path, output.stdout)
            .map_err(|error| format!("Write screenshot {}: {error}", path.display()))?;
        Ok(path)
    }

    fn capture_logcat(&self) {
        if let Ok(output) = self.adb(&["logcat", "-d"]) {
            let _ = fs::write(self.output.join("android-logcat.txt"), output.stdout);
        }
    }

    fn restore(&mut self) {
        if self.app_started {
            println!(
                "Cleaning up Android test session: closing the OpenWASD BLE HID server and releasing any active connection (keeping the pairing bond)."
            );
            let _ = self
                .adb(&["exec-out", "screencap", "-p"])
                .and_then(|screen| {
                    fs::write(self.output.join("02-app-after-test.png"), screen.stdout)
                });
            self.capture_logcat();
            let _ = self.adb(&["shell", "am", "force-stop", PACKAGE]);
            for _ in 0..10 {
                if !is_app_process_running(self) {
                    break;
                }
                sleep(Duration::from_millis(200));
            }
            if is_app_process_running(self) {
                eprintln!(
                    "Cleanup could not confirm that OpenWASD stopped; the BLE HID connection may still be active."
                );
            } else {
                println!(
                    "Android app stopped; its GATT server closed and any active BLE HID connection was released. Pairing bond preserved."
                );
            }
            self.app_started = false;
        }
        if let Some(value) = &self.old_stay_awake {
            let _ = self.adb(&[
                "shell",
                "settings",
                "put",
                "global",
                "stay_on_while_plugged_in",
                value,
            ]);
        }
        if let Some(rotation) = &self.old_accelerometer_rotation {
            let _ = self.adb(&[
                "shell",
                "settings",
                "put",
                "system",
                "accelerometer_rotation",
                rotation,
            ]);
        }
        if let Some(rotation) = &self.old_user_rotation {
            let _ = self.adb(&[
                "shell",
                "settings",
                "put",
                "system",
                "user_rotation",
                rotation,
            ]);
        }
    }
}

impl Drop for DeviceState {
    fn drop(&mut self) {
        self.restore();
    }
}

fn main() {
    if !cfg!(target_os = "macos") {
        eprintln!("The Android + Mac BLE HID integration runner currently requires macOS.");
        std::process::exit(2);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/android-ble-hid-test is inside the repository")
        .to_path_buf();
    let output_dir = match output_directory(&root) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Could not prepare test output: {error}");
            std::process::exit(2);
        }
    };
    println!("Artifacts: {}", output_dir.display());

    let adb = adb_path();
    let mut device = DeviceState::new(adb.clone(), output_dir.clone());
    let mut results = Results::default();

    let test_result = run_test(&root, &mut device, &mut results);
    device.restore();
    let _ = write_results(&output_dir, &results);

    match test_result {
        Ok(()) => {
            println!(
                "PASS: Android app and Mac host checks. Artifacts: {}",
                output_dir.display()
            );
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            eprintln!("Results: {}/test-results.txt", output_dir.display());
            std::process::exit(1);
        }
    }
}

fn run_test(root: &Path, device: &mut DeviceState, results: &mut Results) -> Result<(), String> {
    android_preflight(device)?;
    setup_rotation(device)?;
    run_android_app(root, device)?;
    results.android_app = "PASS".into();
    println!(
        "PASS: Android app is foregrounded and rendered, registered HID GATT, and started one BLE advertiser."
    );

    if env::var("OPENWASD_ANDROID_ONLY").as_deref() == Ok("1") {
        results.mac_host = "SKIPPED (OPENWASD_ANDROID_ONLY=1)".into();
        return Ok(());
    }

    run_host_test(root, device, results)?;
    device.capture_logcat();
    let first_output = device.output.clone();
    device.output = first_output.join("reconnect");
    fs::create_dir_all(&device.output).map_err(|e| e.to_string())?;
    println!("Restarting Android without removing the bond and repeating real Mac HID checks...");
    let reconnect =
        run_android_app(root, device).and_then(|_| run_host_test(root, device, results));
    device.output = first_output;
    results.host_reconnect = if reconnect.is_ok() { "PASS" } else { "FAIL" }.into();
    reconnect?;
    if !is_app_process_running(device) {
        results.android_app = "FAIL (process exited during host test)".into();
        return Err("OpenWASD exited while the Mac host test was running".into());
    }
    results.android_app = "PASS (remained running through host test)".into();
    Ok(())
}

fn android_preflight(device: &DeviceState) -> Result<(), String> {
    if !device.adb.is_file() {
        return Err(format!("ADB not found: {}", device.adb.display()));
    }
    let _ = Command::new(&device.adb).arg("start-server").output();
    let mut adb_state = None;
    for _ in 0..10 {
        match device.adb(&["get-state"]) {
            Ok(output) if output.status.success() => {
                adb_state = Some("device".to_owned());
                break;
            }
            Ok(output) => {
                adb_state = Some(String::from_utf8_lossy(&output.stderr).trim().to_owned())
            }
            Err(error) => adb_state = Some(error.to_string()),
        }
        sleep(Duration::from_millis(500));
    }
    if adb_state.as_deref() != Some("device") {
        let details = device
            .adb(&["devices", "-l"])
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_default();
        return Err(format!(
            "No authorized Android device. Connect it over USB and accept the ADB authorization prompt. Latest adb state: {}; devices: {}",
            adb_state.unwrap_or_else(|| "unknown".into()),
            details
        ));
    }
    let _ = device.adb(&["shell", "input", "keyevent", "224"]);
    let _ = device.adb(&["shell", "wm", "dismiss-keyguard"]);
    let mut awake = false;
    for _ in 0..10 {
        if device
            .adb_text(&["shell", "dumpsys", "power"])
            .is_ok_and(|text| text.lines().any(|line| line.contains("mWakefulness=Awake")))
        {
            awake = true;
            break;
        }
        sleep(Duration::from_secs(1));
    }
    if !awake {
        return Err("Android display did not wake. Wake the device and rerun.".into());
    }
    // The RP5 uses a swipe-only lock screen. Wake and dismiss it through ADB
    // so this Rust test can run unattended without a PIN/password prompt.
    if device
        .adb_text(&["shell", "dumpsys", "window"])
        .is_ok_and(|window| window.contains("isKeyguardShowing=true"))
    {
        if let Ok(size) = device.adb_text(&["shell", "wm", "size"]) {
            if let Some((width, height)) = parse_display_size(&size) {
                let _ = device.adb(&[
                    "shell",
                    "input",
                    "swipe",
                    &(width / 2).to_string(),
                    &(height * 4 / 5).to_string(),
                    &(width / 2).to_string(),
                    &(height / 5).to_string(),
                    "350",
                ]);
            }
        }
        sleep(Duration::from_millis(700));
    }
    device.adb_ok(&["shell", "input", "keyevent", "KEYCODE_BUTTON_A"])?;
    device.capture_screenshot("00-preflight.png")?;
    let window = device.adb_text(&["shell", "dumpsys", "window"])?;
    if window.contains("isKeyguardShowing=true") {
        return Err("Retroid is locked; unlock it before running the test.".into());
    }
    Ok(())
}

fn setup_rotation(device: &mut DeviceState) -> Result<(), String> {
    device.old_stay_awake = Some(device.adb_text(&[
        "shell",
        "settings",
        "get",
        "global",
        "stay_on_while_plugged_in",
    ])?);
    device.adb_ok(&["shell", "svc", "power", "stayon", "usb"])?;
    device.old_accelerometer_rotation = Some(device.adb_text(&[
        "shell",
        "settings",
        "get",
        "system",
        "accelerometer_rotation",
    ])?);
    device.old_user_rotation =
        Some(device.adb_text(&["shell", "settings", "get", "system", "user_rotation"])?);
    device.adb_ok(&[
        "shell",
        "settings",
        "put",
        "system",
        "accelerometer_rotation",
        "0",
    ])?;
    let size = device.adb_text(&["shell", "wm", "size"])?;
    let (width, height) = parse_display_size(&size).unwrap_or((1920, 1080));
    let rotation = if width < height { "1" } else { "0" };
    device.adb_ok(&[
        "shell",
        "settings",
        "put",
        "system",
        "user_rotation",
        rotation,
    ])?;
    device.adb_ok(&["logcat", "-c"])?;
    Ok(())
}

fn run_android_app(root: &Path, device: &mut DeviceState) -> Result<(), String> {
    println!("Building the Android APK...");
    let build_log = device.output.join("build-and-install.log");
    let target_dir = root.join("target/hid-host-test");
    let mut build = Command::new("cargo")
        .args(["run", "--quiet", "--manifest-path"])
        .arg(root.join("../makepad/tools/cargo_makepad/Cargo.toml"))
        .args(["--", "android", "build", "-p", "openwasd", "--release"])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target_dir)
        .stdout(Stdio::from(
            File::create(&build_log).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            File::options()
                .append(true)
                .open(&build_log)
                .map_err(|e| e.to_string())?,
        ))
        .spawn()
        .map_err(|error| format!("Could not start cargo makepad Android build: {error}"))?;
    let status = build
        .wait()
        .map_err(|error| format!("Wait for Android build: {error}"))?;
    ensure_success(status, "Android APK build", &build_log)?;

    let apk = target_dir.join("makepad-android-apk/openwasd/apk/openwasd.apk");
    if !apk.is_file() {
        return Err(format!(
            "Android build succeeded but APK was not found at {}",
            apk.display()
        ));
    }
    println!("Installing and launching OpenWASD on Android...");
    device.adb_ok(&[
        "install",
        "-r",
        apk.to_str().ok_or("APK path is not UTF-8")?,
    ])?;
    device.adb_ok(&["shell", "am", "force-stop", PACKAGE])?;
    device.adb_ok(&["logcat", "-c"])?;
    device.adb_ok(&["shell", "monkey", "-p", PACKAGE, "1"])?;
    device.app_started = true;
    let mut startup_log = String::new();
    for _ in 0..30 {
        startup_log = device.adb_text(&["logcat", "-d", "-s", "Makepad:I"])?;
        if startup_log.contains("[OpenWASD][Bluetooth] app startup") {
            break;
        }
        sleep(Duration::from_secs(1));
    }
    if !startup_log.contains("[OpenWASD][Bluetooth] app startup") {
        return Err(format!(
            "Android launched the activity but Makepad did not finish app startup within 30 seconds. Startup logs: {startup_log}"
        ));
    }

    if !is_app_process_running(device) {
        return Err("OpenWASD process is not running after launch".into());
    }
    let activities = device.adb_text(&["shell", "dumpsys", "activity", "activities"])?;
    if !activities
        .lines()
        .any(|line| line.contains("ResumedActivity") && line.contains(PACKAGE))
    {
        return Err("OpenWASD is not the resumed Android activity".into());
    }

    let dashboard = device.capture_screenshot("01-app-dashboard.png")?;
    let png = fs::read(&dashboard).map_err(|error| error.to_string())?;
    if png.len() < 10_000 {
        return Err("Android app screenshot is empty or unexpectedly small".into());
    }
    let (width, height) = png_dimensions(&png).ok_or("Could not read screenshot PNG dimensions")?;
    device
        .adb_ok(&[
            "shell",
            "input",
            "tap",
            &(width * 6 / 100).to_string(),
            &(height * 17 / 100).to_string(),
        ])
        .ok();
    device
        .adb_ok(&[
            "shell",
            "input",
            "tap",
            &(width * 17 / 100).to_string(),
            &(height * 49 / 100).to_string(),
        ])
        .ok();
    sleep(Duration::from_secs(1));

    device.adb_ok(&[
        "shell",
        "input",
        "tap",
        &(width * 72 / 100).to_string(),
        &(height * 64 / 100).to_string(),
    ])?;
    let mut hid_logs = String::new();
    for _ in 0..15 {
        hid_logs = device.adb_text(&["logcat", "-d", "-s", "OpenWASD-HID:I"])?;
        if hid_logs.contains("BLE HID advertising started")
            || hid_logs.contains("System HID registered;")
        {
            break;
        }
        sleep(Duration::from_secs(1));
    }
    device.capture_screenshot("01-app-before-test.png")?;
    if hid_logs.contains("System HID registered;") {
        if hid_logs.contains("Starting connectable BLE HID advertising") {
            return Err("System HID unexpectedly started a second BLE advertiser".into());
        }
        return Ok(());
    }
    if !hid_logs.contains("Required HOGP services registered; HID GATT database is ready")
        || !hid_logs.contains(&format!("GATT service added; uuid={HID_SERVICE} status=0"))
    {
        return Err("Android did not report a ready HID GATT service".into());
    }
    let connected_host = hid_logs.lines().any(|line| {
        line.contains("GATT host connection changed")
            && line.contains("status=0")
            && line.contains("state=2")
    });
    let advertising_started = hid_logs.contains("BLE HID advertising started");
    if count(&hid_logs, "Starting connectable BLE HID advertising") > 1
        || count(&hid_logs, "BLE HID advertising started") > 1
        || !(connected_host || advertising_started)
    {
        return Err("Android HID GATT is ready but neither has one connected host nor one active advertiser; inspect android-logcat.txt".into());
    }
    Ok(())
}

fn run_host_test(root: &Path, device: &DeviceState, results: &mut Results) -> Result<(), String> {
    let target_dir = root.join("target/hid-host-test");
    let manifest = root.join("tools/hid-host-test/Cargo.toml");
    let build_log = device.output.join("host-test-build.log");
    println!("Building the Rust macOS HID host test...");
    let status = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(&manifest)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target_dir)
        .stdout(Stdio::from(
            File::create(&build_log).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            File::options()
                .append(true)
                .open(&build_log)
                .map_err(|e| e.to_string())?,
        ))
        .status()
        .map_err(|error| format!("Could not start host test build: {error}"))?;
    ensure_success(status, "Mac host test build", &build_log)?;

    let app = root.join("target/OpenWASD HID Host Test.app");
    let contents = app.join("Contents");
    let executable = contents.join("MacOS/openwasd-hid-host-test");
    fs::create_dir_all(executable.parent().expect("host app executable has parent"))
        .map_err(|error| error.to_string())?;
    fs::copy(
        root.join("tools/hid-host-test/Info.plist"),
        contents.join("Info.plist"),
    )
    .map_err(|error| format!("Copy host app Info.plist: {error}"))?;
    fs::copy(
        target_dir.join("release/openwasd-hid-host-test"),
        &executable,
    )
    .map_err(|error| format!("Copy host test executable: {error}"))?;
    command_ok(
        "codesign",
        &["--force", "--deep", "--sign", "-"],
        Some(&app),
    )?;

    let host_log = device.output.join("host-test.log");
    let host_out = File::create(&host_log).map_err(|error| error.to_string())?;
    let host_err = File::options()
        .append(true)
        .open(&host_log)
        .map_err(|error| error.to_string())?;
    println!("Running the Mac HID input integration test...");
    let status = Command::new("open")
        .args(["-n", "-W", "--stdout"])
        .arg(&host_log)
        .arg("--stderr")
        .arg(&host_log)
        .arg("--env")
        .arg(format!("ADB={}", device.adb.display()))
        .arg("--env")
        .arg(format!("OPENWASD_TEST_OUTPUT={}", device.output.display()))
        .arg(&app)
        .stdout(Stdio::from(host_out))
        .stderr(Stdio::from(host_err))
        .status()
        .map_err(|error| format!("Could not launch host test app: {error}"))?;
    let log = fs::read_to_string(&host_log).unwrap_or_default();
    classify_host_results(&log, results);
    if status.success() && log.contains("TEST_RESULT: PASS") {
        results.mac_host = "PASS".into();
        Ok(())
    } else {
        results.mac_host = "FAIL".into();
        Err(format!(
            "Mac BLE host integration failed. See {}",
            host_log.display()
        ))
    }
}

fn classify_host_results(log: &str, results: &mut Results) {
    results.host_unique_device = if log.contains("HOST_CHECK: unique BLE HID device count=1 PASS")
        || log.contains("HOST_CHECK: unique system Bluetooth HID device count=1 PASS")
    {
        "PASS"
    } else if log.contains("HOST_CHECK: BLE scan skipped;") {
        "SKIPPED (system HID connection reused)"
    } else {
        "FAIL"
    }
    .into();
    results.host_ble_reports =
        if log.contains("HOST_CHECK: Android HID mouse/button/keyboard reports submitted PASS") {
            "PASS"
        } else {
            "FAIL"
        }
        .into();
    results.host_pointer = if log.contains("HOST_CHECK: macOS pointer moved PASS") {
        "PASS"
    } else {
        "FAIL"
    }
    .into();
    results.host_buttons =
        if log.contains("HOST_CHECK: macOS left/middle/right buttons registered PASS") {
            "PASS"
        } else {
            "FAIL"
        }
        .into();
    results.host_keyboard = if log.contains("HOST_CHECK: macOS keyboard A registered PASS") {
        "PASS"
    } else {
        "FAIL"
    }
    .into();
}

fn is_app_process_running(device: &DeviceState) -> bool {
    device
        .adb(&["shell", "pidof", PACKAGE])
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}

fn write_results(output: &Path, results: &Results) -> io::Result<()> {
    let text = format!(
        "android_app={}\nmac_host={}\nhost_unique_hid_device={}\nhost_ble_reports_acknowledged={}\nhost_pointer_moved={}\nhost_mouse_buttons={}\nhost_keyboard={}\n",
        if results.android_app.is_empty() {
            "NOT_RUN"
        } else {
            &results.android_app
        },
        if results.mac_host.is_empty() {
            "NOT_RUN"
        } else {
            &results.mac_host
        },
        if results.host_unique_device.is_empty() {
            "NOT_RUN"
        } else {
            &results.host_unique_device
        },
        if results.host_ble_reports.is_empty() {
            "NOT_RUN"
        } else {
            &results.host_ble_reports
        },
        if results.host_pointer.is_empty() {
            "NOT_RUN"
        } else {
            &results.host_pointer
        },
        if results.host_buttons.is_empty() {
            "NOT_RUN"
        } else {
            &results.host_buttons
        },
        if results.host_keyboard.is_empty() {
            "NOT_RUN"
        } else {
            &results.host_keyboard
        },
    );
    fs::write(output.join("test-results.txt"), text)
}

fn adb_path() -> PathBuf {
    env::var_os("ADB").map(PathBuf::from).unwrap_or_else(|| {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        home.join(".makepad/android_33_macos_aarch64/platform-tools/adb")
    })
}

fn output_directory(root: &Path) -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("OPENWASD_TEST_OUTPUT") {
        let path = PathBuf::from(path);
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        return Ok(path);
    }
    let timestamp = Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string()
        });
    let output = root.join(format!("target/android-ble-hid-test/{timestamp}"));
    fs::create_dir_all(&output).map_err(|error| error.to_string())?;
    Ok(output)
}

fn parse_display_size(text: &str) -> Option<(u32, u32)> {
    text.lines()
        .filter_map(|line| line.split_once(": ").map(|(_, size)| size.trim()))
        .last()
        .and_then(|size| size.split_once('x'))
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
}

fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(png.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(png.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

fn ensure_success(status: ExitStatus, description: &str, log: &Path) -> Result<(), String> {
    if status.success() {
        Ok(())
    } else {
        let output = fs::read_to_string(log).unwrap_or_default();
        Err(format!(
            "{description} failed ({status}). Log: {}\n{output}",
            log.display()
        ))
    }
}

fn command_ok(program: &str, args: &[&str], bundle: Option<&Path>) -> Result<(), String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(bundle) = bundle {
        command.arg(bundle);
    }
    let output = command
        .output()
        .map_err(|error| format!("Could not run {program}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_display_size, png_dimensions};

    #[test]
    fn parses_android_display_size() {
        assert_eq!(
            parse_display_size("Physical size: 1920x1080\n"),
            Some((1920, 1080))
        );
    }

    #[test]
    fn reads_png_dimensions_from_header() {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend_from_slice(&1920_u32.to_be_bytes());
        png.extend_from_slice(&1080_u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((1920, 1080)));
    }
}
