# Android btleplug packaging

The btleplug Android backend owns its Java JNI bridge under
`btleplug/src/droidplug/java/src/main/java`. OpenWASD does not keep a copy of
that platform implementation in the app crate. The local Makepad fork's
`cargo-makepad` discovers the resolved btleplug checkout with `cargo metadata`
and compiles the dependency-owned Java sources with Makepad's Android sources
before generating DEX.

Build and install `cargo-makepad` from the local Makepad checkout after changing
it:

```sh
cd ../makepad
git status --short
cargo install --path tools/cargo_makepad --force
cd ../openwasd
cargo makepad android run -p openwasd --release
```

The current Android backend reports connected BLE/GATT peripherals. Android
classic Bluetooth HID profile connections still need a native system-profile
query to appear in OpenWASD's connected-device list.

## BLE HID peripheral API

`ble-peripheral-rust` provides peripheral backends for Apple, Linux, and
Windows, but its published crate has no Android backend. `bluster` relies on
the D-Bus Android target, which an ordinary Android app cannot use to access
the platform Bluetooth stack. OpenWASD therefore uses the Android GATT/HID
implementation already exposed by the btleplug fork instead of adding a
second BLE stack. The OpenWASD Rust adapter exposes start/stop, mouse and
keyboard report, and connected-host-name methods for this peripheral.

The Android HID bridge is maintained directly in the btleplug fork. Its JNI
compatibility fixes must be committed in that checkout when required. The APK
manifest template declares Android 12+ scan, connect, and advertise permissions.
The peripheral start method requests connect and advertise permissions when
Android 12+ has not granted them yet. The current UI does not invoke the
peripheral API yet, so this is build-verified transport support rather than an
end-to-end touchpad flow.
