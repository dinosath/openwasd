# OpenWASD

OpenWASD is a cross-platform input control workspace built around a shared Rust engine, Dioxus UI, and Robius platform foundations.

## Current implementation

- `crates/openwasd-core`: portable profiles, layers, mappings, device state, transport types, and custom-control report aggregation.
- `crates/openwasd-app`: Dioxus desktop and Android application shell, with shared Bluetooth service orchestration. The former Makepad UI is retained as an optional legacy binary during migration.
- Dioxus navigation for Home, Devices, and Layouts, plus keyboard, gamepad, touchpad, and custom control surfaces.
- Layout creation asks for a name and starts with a `clickme` gamepad button. Custom layouts can add up to six buttons and edit their labels and supported digital actions.
- Bluetooth discovery, RSSI, device selection, pairing, connection-state updates, and Android HID report commands use the existing `openwasd-bluetooth` service.
- Profile mapping workspace, virtual device routing, USB companion UI, and full custom-control editor remain in the legacy Makepad implementation and have not yet been ported to Dioxus.

The Bluetooth service uses the [`dinosath/btleplug` fork](https://github.com/dinosath/btleplug/tree/codex/openwasd-bluetooth-features) for BLE central scanning, GATT connections, and the `Central::connected_peripherals()` API on supported platforms, including macOS and Android. Its Android BLE HID-over-GATT support handles mouse and keyboard reports; OpenWASD's Android adapter exposes start/stop, report, and connected-host methods, and the touchpad and click controls send HID reports through that adapter.

### Android controls over USB on Linux (development)

The Linux Devices screen lists attached USB devices. Select an Android device
to request Android Open Accessory mode and connect its OpenWASD app directly
over USB. This runtime transport uses `nusb`; it does not use ADB or require
USB debugging. Android accessory-mode support depends on the device. The Linux
user also needs permission to access the USB device and `/dev/uinput`.

Once connected, each Android companion gets its own Linux virtual gamepad,
keyboard and mouse. Select it again to disconnect. Those virtual devices are
removed on disconnect or app exit. Select **View incoming input** to inspect
recent reports. Open a gamepad, keyboard, touchpad or custom layout on the
Android device; reports use USB while the session is active, otherwise the
existing Bluetooth HID route is used.

Installing or launching the Android app can use ADB and may require accepting
Android's USB debugging prompt. That is a development and deployment step; it
is not needed for the app's USB accessory connection.
Open Accessory mode is a device-side USB feature, not generic USB HID. Analog
control widgets, configurable multi-route profiles, and device-specific
Bluetooth input inspection remain future work.

With `/dev/uinput` access and permission to read the resulting input devices,
run the Linux virtual-gamepad hardware check:

```sh
cargo test -p openwasd-output -- --ignored
```

### Custom companion controls

On Android, open **Layouts → New layout**, select **Custom controls**, and
create a layout. Open its settings to add, label, reorder, and remove up to six
buttons. Select a button and choose a keyboard key, volume key, gamepad button,
mouse button, or Android Bluetooth settings shortcut. **Assign hardware button**
learns a physical button for that control; the on-screen button remains usable.
Changes are saved on the device. Open the layout and connect a Bluetooth HID
host to send input; controls release when the layout loses focus or closes.

This first editor uses a fixed grid, not free placement, and supports digital
buttons rather than analog sticks or arbitrary widgets. Hardware mappings work
while the custom layout is in the foreground, and depend on Android delivering
the button to the app. Volume output needs the updated btleplug HID descriptor;
hosts that cached the previous descriptor may need the device re-paired.
The custom editor and HID report checks above were performed with the legacy
Makepad shell. Separately, the Dioxus Android APK has been built and launched
on an emulator and Retroid Pocket 5. Its btleplug Java classes are packaged;
both devices start BLE scanning without the previous missing Java-class error.
The emulator found no peripherals; Retroid scans returned BLE peripherals.
Android also registered the HID service while opening a custom layout. HID
report delivery to a host still needs an end-to-end check. The packaging fix
is in btleplug commit `0c5c38d`, published on the fork branch and pinned by
`Cargo.lock`.

## Run

Run the Dioxus desktop app locally (Linux requires the Dioxus desktop WebKitGTK,
GTK, and DBus development packages):

```text
cargo run -p openwasd
```

Run on an Android device connected over USB with USB debugging enabled. Install
the Dioxus Android requirements (JDK, SDK, NDK, and Android Rust target), then
confirm ADB lists the device as `device` and serve the app:

```text
adb devices
dx serve --android
```

### Run on an Android emulator (AVD)

Install the Android CLI using the [official installation instructions](https://developer.android.com/tools/agents/android-cli/download), then point Dioxus and Android tools at the same SDK. Dioxus Android packaging may require SDK packages newer than those included in older local SDK bundles:

```sh
export ANDROID_SDK_ROOT="$HOME/Android/Sdk"
export PATH="$ANDROID_SDK_ROOT/platform-tools:$ANDROID_SDK_ROOT/emulator:$PATH"

# Install an emulator and an Android system image using the Android CLI.
android --sdk="$ANDROID_SDK_ROOT" sdk install emulator platform-tools system-images/android-33/google_apis/arm64-v8a

# Create and start a medium phone AVD. Skip create if you already have one.
android --sdk="$ANDROID_SDK_ROOT" emulator create --profile=medium_phone
android --sdk="$ANDROID_SDK_ROOT" emulator list
android --sdk="$ANDROID_SDK_ROOT" emulator start medium_phone
adb devices -l
```

Start the app on the running AVD:

```sh
dx serve --android
```

For a release bundle, use `dx bundle --platform android --release` after the
required SDK packages are installed and licensed.

AVDs are useful for checking Android startup, layout rendering, rotation, and touch input. Treat Bluetooth HID pairing and report delivery as hardware checks: Android's emulator documentation lists Bluetooth among unsupported virtual hardware, while newer emulator networking docs describe Bluetooth simulation. Verify gamepad reports with a physical Android device and a Bluetooth host before relying on emulator results ([emulator limitations](https://developer.android.com/studio/run/advanced-emulator-usage), [emulator networking](https://developer.android.com/studio/run/emulator-networking)).

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

The Android Rust target can be checked with Cargo. The ignored desktop
screenshot smoke test still targets the former Makepad shell and is not yet
valid for the default Dioxus UI; its harness needs to be ported before use.

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

The Dioxus app builds for Android and has been installed and checked on an
emulator and Retroid Pocket 5. The old Makepad binary remains available with
`--features legacy-makepad --bin openwasd-makepad` during migration. OpenWASD
resolves btleplug from the Git branch linked above; its Android Java sources
are packaged through Manganis FFI metadata. Check out `../btleplug` only when
btleplug source changes are required, and keep local path overrides out of
committed Cargo files. Robius remains a Cargo dependency.

[Retroid Pocket 5]: https://www.goretroid.com/collections/retro-game-system/products/retroid-pocket-5-handheld
[AYN Odin 2 Portal]: https://www.ayntec.com/
[Steam Deck OLED]: https://www.steamdeck.com/en/tech/oled
[ANBERNIC RG556]: https://anbernic.com/a/shop/products/rg556
