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
