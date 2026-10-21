# OpenWASD

OpenWASD is a cross-platform input control workspace built around a shared Rust engine, Makepad UI, and Robius platform foundations.

## Current implementation

- `crates/openwasd-core`: portable profiles, layers, mappings, device state, and Bluetooth/USB/Wi-Fi transport types.
- `crates/openwasd-app`: Makepad application shell inspired by Robrix's script-driven app structure.
- Collapsible virtual-device sidebar with explicit virtual-to-physical routing.
- Transport-neutral Bluetooth session queues for platform adapters to connect devices and pass normalized packets into the engine.
- Phase 1 Bluetooth workspace with live `btleplug` discovery, RSSI, device selection, pairing/OS-settings messaging, and connect/disconnect state updates.
- Connectivity strip for Bluetooth, USB, and Wi-Fi companion routes.
- reWASD-inspired mapping workspace with profile/device toolbar, layer rail, controller preview, mapping editor, link-health inspector, and safe-stop affordance.

The Bluetooth service uses the local `../btleplug` fork for BLE central scanning, GATT connections, and the `Central::connected_peripherals()` API on supported platforms, including macOS and Android. Keep the fork checked out beside this repository; the local path dependency ensures development builds include its committed Android BLE HID-over-GATT support for mouse and keyboard reports. OpenWASD's Android adapter exposes start/stop, mouse/keyboard report, and connected-host methods; the touchpad and click controls send HID reports through this adapter.

## Run

Run the desktop app locally:

```text
cargo run -p openwasd
```

Run on an Android device connected over USB with USB debugging enabled. First
confirm that ADB lists the device as `device` (accept the authorization prompt
on the device if needed), then build, install, and launch OpenWASD:

```text
adb devices
cargo makepad android run -p openwasd --release
```

### Run on an Android emulator (AVD)

Install the Android CLI using the [official installation instructions](https://developer.android.com/tools/agents/android-cli/download), then point it and Cargo Makepad at the same SDK. These commands use the Makepad SDK directory on an Apple Silicon Mac:

```sh
export ANDROID_SDK_ROOT="$HOME/.makepad/android_33_macos_aarch64"
export PATH="$ANDROID_SDK_ROOT/platform-tools:$ANDROID_SDK_ROOT/emulator:$PATH"

# Install the emulator and an API 33 ARM64 image (API 33 matches Makepad's local SDK).
android --sdk="$ANDROID_SDK_ROOT" sdk install emulator platform-tools system-images/android-33/google_apis/arm64-v8a

# Create and start a medium phone AVD. Skip create if you already have one.
android --sdk="$ANDROID_SDK_ROOT" emulator create --profile=medium_phone
android --sdk="$ANDROID_SDK_ROOT" emulator list
android --sdk="$ANDROID_SDK_ROOT" emulator start medium_phone
adb devices -l
```

Install Cargo Makepad from the sibling Makepad checkout if the `cargo makepad` command is not available, then build, deploy, and launch OpenWASD on the running AVD:

```sh
cargo install --path ../makepad/tools/cargo_makepad --force
cargo makepad android --sdk-path="$ANDROID_SDK_ROOT" run -p openwasd --release
```

To build without launching, replace `run` with `build`; the APK is written under `target/android/makepad-android-apk/openwasd/apk/openwasd.apk`. For an Intel Mac, use the `android_33_macos_x64` SDK directory and an `x86_64` system image, and pass `--abi=x86_64` to Cargo Makepad.

AVDs are useful for checking Android startup, layout rendering, rotation, and touch input. Treat Bluetooth HID pairing and report delivery as hardware checks: Android's emulator documentation lists Bluetooth among unsupported virtual hardware, while newer emulator networking docs describe Bluetooth simulation. Verify gamepad reports with a physical Android device and a Bluetooth host before relying on emulator results ([emulator limitations](https://developer.android.com/studio/run/advanced-emulator-usage), [emulator networking](https://developer.android.com/studio/run/emulator-networking)).

`cargo makepad android run` uses ADB to deploy and start the app. To build and
install separately, then launch the installed APK directly with ADB:

```text
cargo makepad android build -p openwasd --release
adb install -r target/android/makepad-android-apk/openwasd/apk/openwasd.apk
adb shell monkey -p dev.makepad.openwasd 1
```

Run the shared-engine tests with:

```text
cargo test --workspace
```

The platform test workflow runs the core and Bluetooth service tests on Linux,
macOS, and Windows, and compiles the same tests for Android. It covers the
platform-independent report encoding and service behavior; Bluetooth hardware
delivery remains covered by the physical-device integration test below.

The Bluetooth service tests cover gamepad button and D-pad press/release
reports and are platform-independent. Run them on the current host with the
workspace tests above. To compile those same test cases for Linux, Windows, or
Android from another host (compile-only; they still need to run on that OS to
exercise its runtime), use:

```text
cargo check -p openwasd-bluetooth --tests --target x86_64-unknown-linux-gnu
cargo check -p openwasd-bluetooth --tests --target x86_64-pc-windows-gnu
cargo check -p openwasd-bluetooth --tests --target aarch64-linux-android
```

The Android app target is built with Cargo Makepad as shown above. The ignored
desktop screenshot smoke test also includes the gamepad surface in portrait
and landscape. Run it in a graphical desktop session with an available GPU:

```text
cargo test -p openwasd --test screenshot_smoke -- --ignored
```

Run the Android-to-Mac BLE HID integration test with a Retroid/Android device
connected over USB and unlocked, and Bluetooth enabled on the Mac:

```text
cargo run --release --manifest-path tools/android-ble-hid-test/Cargo.toml
```

This Rust runner builds and installs OpenWASD on Android, checks that the app
renders and registers its HID GATT service, then sends touchpad movement,
left/middle/right clicks, and an `A` key press through Android touch input. It
uses `btleplug` to scan for the Android HID advertisement and requires exactly
one matching device. It then checks Android's GATT notification acknowledgements
and observes the Mac's pointer, mouse-button, and keyboard state through
CoreGraphics. The Mac must already be paired and connected to the Android device
as a Bluetooth HID host. The btleplug scan requires macOS Bluetooth permission.

If the app logs no HID report subscriptions, the Mac's system HID connection is
not active. Pair RP5 with the Mac as a Bluetooth HID device and make sure the
Mac shows it as connected before rerunning the test. A passing test requires
acknowledged mouse and keyboard reports plus corresponding host-side pointer,
button, and key events; a nearby advertisement or paired state alone does not
count.
Screenshots, Android logcat, build output, and host test output are saved under
`target/android-ble-hid-test/<timestamp>/`; `test-results.txt` records Android
app and Mac host results separately. The test force-stops OpenWASD and restores
the Android device's original screen rotation when it finishes. Set
`OPENWASD_ANDROID_ONLY=1` to run just the Android app checks.

Press **F12** in the running app to save the next rendered frame to
`screenshots/openwasd-<timestamp>.png`. To capture the first rendered frame
automatically, set `OPENWASD_SCREENSHOT_ON_STARTUP` to the output file path
before launching the app:

```text
OPENWASD_SCREENSHOT_ON_STARTUP=/tmp/openwasd.png cargo run -p openwasd
```

Run the handheld screenshot smoke test (it launches and closes the app for each
profile and saves verified PNGs in the system temp directory):

```text
cargo test -p openwasd --test screenshot_smoke -- --ignored --nocapture
```

The viewport profiles follow published display sizes: [Retroid Pocket 5]
(1920×1080 landscape and 1080×1920 portrait), [AYN Odin 2 Portal]
(1920×1080), [Steam Deck OLED] (1280×800), and [ANBERNIC RG556] in landscape
(1920×1080; its panel is listed as 1080×1920).

For local development, keep the Makepad and btleplug forks checked out beside this repository at `../makepad` and `../btleplug`. Android builds use cargo-makepad from the local Makepad checkout and btleplug from its local path; the app UI stays on the Makepad branch verified for Android rendering. Robius remains a Cargo dependency.

[Retroid Pocket 5]: https://www.goretroid.com/collections/retro-game-system/products/retroid-pocket-5-handheld
[AYN Odin 2 Portal]: https://www.ayntec.com/
[Steam Deck OLED]: https://www.steamdeck.com/en/tech/oled
[ANBERNIC RG556]: https://anbernic.com/a/shop/products/rg556
