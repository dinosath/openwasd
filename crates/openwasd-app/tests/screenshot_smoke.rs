use std::fs;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct RunningApp(Child);

struct DeviceProfile {
    name: &'static str,
    width: u32,
    height: u32,
}

impl RunningApp {
    fn stop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
#[ignore = "requires a desktop session with an available GPU"]
fn app_captures_handheld_viewports_and_closes() {
    let profiles = [
        DeviceProfile {
            name: "retroid-pocket-5",
            width: 1920,
            height: 1080,
        },
        DeviceProfile {
            name: "retroid-pocket-5-portrait",
            width: 1080,
            height: 1920,
        },
        DeviceProfile {
            name: "ayn-odin-2-portal",
            width: 1920,
            height: 1080,
        },
        DeviceProfile {
            name: "steam-deck-oled",
            width: 1280,
            height: 800,
        },
        DeviceProfile {
            name: "anbernic-rg556-landscape",
            width: 1920,
            height: 1080,
        },
    ];

    // Determine the host's backing scale from the app's default 1100x720
    // logical window, then size each test window to produce device pixels.
    let (_, probe_width, probe_height) = capture_screenshot("scale-probe", None);
    let scale_x = probe_width as f64 / 1100.0;
    let scale_y = probe_height as f64 / 720.0;
    assert!(
        (scale_x - scale_y).abs() < 0.02,
        "default capture should have a uniform backing scale (x={scale_x:.2}, y={scale_y:.2})"
    );
    let backing_scale = (scale_x + scale_y) / 2.0;

    for profile in profiles {
        let logical_width = (profile.width as f64 / backing_scale).round() as u32;
        let logical_height = (profile.height as f64 / backing_scale).round() as u32;
        let (_, width, height) =
            capture_screenshot(profile.name, Some((logical_width, logical_height)));
        assert_eq!(
            (width, height),
            (profile.width, profile.height),
            "{} screenshot should match the device's native pixel resolution",
            profile.name
        );
    }
}

#[test]
#[ignore = "requires a desktop session with an available GPU"]
fn app_captures_mobile_layout_pages_and_closes() {
    // Determine the desktop backing scale, then size the window to capture a
    // 1080x1920 portrait device viewport (Retroid/phone class).
    let (_, probe_width, _) = capture_screenshot("layout-scale-probe", None);
    let backing_scale = probe_width as f64 / 1100.0;
    let logical_size = (
        (1080.0 / backing_scale).round() as u32,
        (1920.0 / backing_scale).round() as u32,
    );

    for (name, screen, layout) in [
        ("layout-picker", "layouts", "touchpad"),
        ("touchpad-surface", "layout_surface", "touchpad"),
        ("keyboard-surface", "layout_surface", "keyboard"),
        ("gamepad-surface", "layout_surface", "gamepad"),
        ("touchpad-settings", "layout_settings", "touchpad"),
        ("keyboard-settings", "layout_settings", "keyboard"),
    ] {
        let (_, width, height) =
            capture_screenshot_for(name, Some(logical_size), Some(screen), Some(layout));
        assert_eq!((width, height), (1080, 1920), "{name} viewport dimensions");
    }

    let landscape_size = (
        (1920.0 / backing_scale).round() as u32,
        (1080.0 / backing_scale).round() as u32,
    );
    for (name, layout) in [
        ("landscape-touchpad-surface", "touchpad"),
        ("landscape-keyboard-surface", "keyboard"),
        ("landscape-gamepad-surface", "gamepad"),
    ] {
        let (_, width, height) = capture_screenshot_for(
            name,
            Some(landscape_size),
            Some("layout_surface"),
            Some(layout),
        );
        assert_eq!((width, height), (1920, 1080), "{name} viewport dimensions");
    }
}

fn capture_screenshot(
    name: &str,
    window_size: Option<(u32, u32)>,
) -> (std::path::PathBuf, u32, u32) {
    capture_screenshot_for(name, window_size, None, None)
}

fn capture_screenshot_for(
    name: &str,
    window_size: Option<(u32, u32)>,
    start_screen: Option<&str>,
    start_layout: Option<&str>,
) -> (std::path::PathBuf, u32, u32) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let screenshot = std::env::temp_dir().join(format!(
        "openwasd-{}-{}-{unique}.png",
        name,
        std::process::id()
    ));

    let mut command = Command::new(env!("CARGO_BIN_EXE_openwasd"));
    command
        .env("OPENWASD_SCREENSHOT_ON_STARTUP", &screenshot)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(screen) = start_screen {
        command.env("OPENWASD_START_SCREEN", screen);
    }
    if let Some(layout) = start_layout {
        command.env("OPENWASD_START_LAYOUT", layout);
    }
    if let Some((width, height)) = window_size {
        command.env("OPENWASD_WINDOW_SIZE", format!("{width}x{height}"));
    }
    let child = command
        .spawn()
        .expect("launch OpenWASD for screenshot capture");
    let mut app = RunningApp(child);

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if screenshot.is_file() {
            break;
        }

        if let Some(status) = app.0.try_wait().expect("check OpenWASD process") {
            panic!(
                "OpenWASD exited with {status} before writing {}",
                screenshot.display()
            );
        }

        assert!(
            Instant::now() < deadline,
            "OpenWASD did not write the {} screenshot within 20 seconds: {}",
            name,
            screenshot.display()
        );
        thread::sleep(Duration::from_millis(100));
    }

    let png = fs::read(&screenshot).expect("read captured screenshot");
    assert!(
        png.starts_with(b"\x89PNG\r\n\x1a\n"),
        "capture is not a PNG"
    );
    assert!(png.len() >= 24, "captured PNG is truncated");
    let width = u32::from_be_bytes(png[16..20].try_into().expect("PNG width"));
    let height = u32::from_be_bytes(png[20..24].try_into().expect("PNG height"));
    assert!(width > 0 && height > 0, "captured PNG has empty dimensions");

    app.stop();
    println!(
        "Captured {} at {} ({}x{})",
        name,
        screenshot.display(),
        width,
        height
    );
    (screenshot, width, height)
}
