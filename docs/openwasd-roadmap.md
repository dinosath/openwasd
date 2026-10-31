# OpenWASD: research, architecture, and Phase 1 roadmap

Status: research and technical design. The implementation stack has since
moved from Makepad to Dioxus; the historical Makepad-specific design notes below
remain as context and should be revised as each roadmap phase is revisited.

This document combines the product capabilities of reWASD and BLEK into a
realistic roadmap for OpenWASD, while keeping the first implementation narrow:
cross-platform Bluetooth discovery, pairing, connection management, and a
cross-platform UI for connecting two devices.

## Current implementation note (2026-10)

The default `openwasd` application uses Dioxus 0.7 for its main navigation,
Bluetooth device discovery and connection, built-in input surfaces, custom
layout creation, and digital button editing. The old Makepad UI remains behind
the optional `legacy-makepad` feature during the transition. An Android APK has
been built, installed, and launched on an emulator and a Retroid Pocket 5.
btleplug's Android Java sources are included in the Dioxus APK through
Manganis FFI metadata. The emulator starts a BLE scan without the former Java
class-loading error; no nearby device was found in that check. On the Retroid,
the custom layout opened, Android registered the HID service, and btleplug
returned BLE peripherals during scanning. HID report delivery to a host still
needs a separate end-to-end check.

## 1. Executive summary

reWASD is primarily a Windows remapping and output-routing workspace. Its
important architectural ideas are Profiles, Configs, per-device Sub-configs,
device groups, explicit output-device selection, Slots, and an Apply/Remap
runtime state. It is dense, dark, hardware-oriented, and organized around an
active device preview.

BLEK is a focused Android-to-device Bluetooth HID remote. Its product promise
is serverless direct Bluetooth control: a phone becomes a keyboard, mouse,
multitouch trackpad, presenter, media remote, and custom remote. The product
does not replace reWASD's profile/remapping engine; it provides a valuable
companion/HID-output mode for OpenWASD.

The combined product should therefore have two distinct paths:

```text
Bluetooth source devices ─┐
                          ├─> normalized input ─> profiles/layers ─> outputs
Android BLEK-style HID ──┘                                      ├─ virtual HID
                                                                 ├─ physical HID
                                                                 └─ Bluetooth HID target
```

Phase 1 should not implement remapping, macros, or virtual gamepads. It should
prove the Bluetooth abstraction and lifecycle with two real connected devices.

## 2. Research basis

Primary references:

- reWASD help guide: <https://help.rewasd.com/>
- reWASD workspace: <https://help.rewasd.com/interface/workspace.html>
- reWASD controller remapping: <https://help.rewasd.com/how-to-remap/controller.html>
- reWASD device groups: <https://help.rewasd.com/how-to-remap/group-of-devices.html>
- reWASD virtual controller: <https://help.rewasd.com/basic-functions/virtual-controller.html>
- reWASD output settings: <https://help.rewasd.com/basic-functions/virtual-controller-settings.html>
- reWASD activators: <https://help.rewasd.com/basic-functions/activators.html>
- reWASD Bluetooth adapters: <https://help.rewasd.com/external-devices/bluetooth.html>
- BLEK Google Play listing: <https://play.google.com/store/apps/details?id=io.appground.blek&hl=en>

The BLEK-specific claims in this document are based on the public Play listing,
not on private implementation details or a source repository. The listing
advertises direct Bluetooth, no server software, Android 7+, Windows 8.1+,
Apple iOS/iPadOS, Android/Android TV, ChromeOS, and Steam Deck compatibility.

## 3. Competitive analysis

| Area | reWASD | BLEK | OpenWASD opportunity |
|---|---|---|---|
| Primary job | Remap many input sources into game/application outputs | Turn Android into a Bluetooth HID remote | One open engine for source normalization, remapping, and HID output |
| Connection model | Local devices plus optional external Bluetooth adapters | Direct phone-to-target Bluetooth, no server | Adapter abstraction with source and target roles |
| Inputs | Controllers, keyboards, mice, groups, gyro, touchpad | Phone touch, keyboard, voice, clipboard, scanner | Common `InputEvent` model with capability metadata |
| Outputs | Physical or virtual gamepads, keyboard, mouse, external controller | Bluetooth keyboard/mouse/presenter HID | Output graph: local HID, virtual HID, Bluetooth HID |
| Configuration | Profiles, Configs, Sub-configs, Layers, Slots | Custom layouts and modes | Profiles plus reusable device groups and output routes |
| Advanced behavior | Activators, combos, turbo, radial menu, shift, curves, vibration, gyro | Presenter, media, keep-alive, custom remote | Separate mapping and companion feature modules |
| UI strategy | Dense workspace, active device preview, Apply/Remap state | Mode-driven remote UI | Dense desktop workspace plus mobile mode screens |
| Platform posture | Windows-focused; virtual output and drivers are central | Android source, standard Bluetooth target | Portable core, platform adapters, honest capability matrix |

## 4. Feature matrix and ordering

| Capability | Phase | Core status | Platform dependency |
|---|---:|---|---|
| Bluetooth discovery | 1 | Device metadata and events | OS Bluetooth APIs |
| Pair/unpair and trusted devices | 1 | Persistent identity model | OS pairing permissions |
| Connect/disconnect/reconnect | 1 | Connection state machine | OS adapter behavior |
| Signal strength | 1 | Optional RSSI field | Not available uniformly |
| Live input detection | 2 | Normalized events | HID/gamepad APIs |
| BLEK keyboard/mouse HID output | 3 | HID report model | Android peripheral / target HID support |
| Profiles, mappings, layers | 4 | Existing core foundation | Storage and UI |
| Macro engine | 5 | Timing and cancellation | None in core; output adapters vary |
| Cross-device remapping | 6 | Routing graph | Source/output adapters |
| Virtual keyboard/mouse | 7 | Output trait | Accessibility/uinput/Windows driver |
| Virtual gamepad | 7 | Gamepad report model | ViGEm alternatives, uinput/UHID, entitlements |
| Android companion | 8 | Sync protocol | Android lifecycle and Bluetooth permissions |
| Motion, flick stick, radial menus | 9 | Advanced input/actions | Sensor and UI APIs |

## 5. Product architecture

### Target workspace

Start with fewer crates than the original proposal. Splitting every device type
before there is a stable event protocol would create churn. The following
workspace keeps boundaries meaningful and can be split later without changing
the public model.

```text
openwasd/
├── crates/
│   ├── openwasd-core          # IDs, state, normalized events, routing types
│   ├── openwasd-bluetooth     # adapter traits, discovery, pairing, sessions
│   ├── openwasd-input         # input normalization and capability probing
│   ├── openwasd-gamepad       # gamepad/HID report interpretation
│   ├── openwasd-profiles       # profiles, configs, layers, persistence
│   ├── openwasd-output         # output trait and output graph
│   ├── openwasd-macro-engine   # later: timing, sequences, cancellation
│   └── openwasd-sync           # later: companion protocol and pairing trust
├── apps/
│   ├── desktop/                # Makepad desktop shell and native adapters
│   └── android/                # Makepad Android shell and Android adapter
└── docs/
```

Dependency direction:

```text
                 ┌──────────────────┐
                 │ openwasd-ui/app  │
                 └────────┬─────────┘
                          │
       ┌──────────────────┼──────────────────┐
       ▼                  ▼                  ▼
 openwasd-profiles   openwasd-bluetooth   openwasd-output
       │                  │                  │
       └──────────────┬───┴──────────────────┘
                      ▼
               openwasd-core
```

Adapters must depend on the core model; core must not depend on Makepad,
Tokio, a Bluetooth library, or an operating-system driver.

### Module boundaries

`openwasd-core` owns stable product semantics: `DeviceId`, `ConnectionState`,
`DeviceKind`, `Transport`, `BluetoothDeviceInfo`, `InputEvent`, `OutputId`, and
serializable configuration models.

`openwasd-bluetooth` owns scanning, pairing, connection supervision, RSSI
updates, and transport packets. It converts platform-specific devices into
core metadata and never exposes `btleplug`, `bluer`, CoreBluetooth, or WinRT
types above the adapter boundary.

`openwasd-input` owns HID report decoding and normalization. It should not know
whether a report came over USB, Bluetooth, Android, or a virtual device.

`openwasd-output` owns an `OutputSink` abstraction. Virtual keyboard/gamepad
drivers and Bluetooth HID peripheral implementations are separate adapters.

`openwasd-profiles` owns profiles, configs, groups, layers, migrations, and
save/apply semantics. It should be usable without a running UI.

## 6. Recommended libraries and constraints

| Need | Candidate | Recommendation |
|---|---|---|
| Async runtime | Tokio | Use in adapter/service crates only; bridge events into Makepad's event loop |
| BLE central | btleplug | Good first cross-platform API for Windows/macOS/Linux; verify Android support before committing |
| Linux Bluetooth | bluer / BlueZ D-Bus | Prefer for Linux-specific central/peripheral capabilities and stable BlueZ behavior |
| Linux input | evdev | Read physical Linux input devices |
| Linux virtual input | uinput | Create virtual keyboard/mouse/gamepad where permissions allow |
| Linux UHID | UHID ioctl | Investigate for user-space HID devices with richer report descriptors |
| Cross-platform HID | hidapi | Useful for HID enumeration/feature reports, not a complete Bluetooth manager |
| Gamepad input | gilrs | Good gamepad abstraction; keep raw HID path available for unusual devices |
| Window/app context | winit | Use only if needed for process/window detection; Makepad already owns the UI window |
| Windows Bluetooth | WinRT / Windows bindings | Needed for capabilities not covered by btleplug |
| Windows virtual output | ViGEm alternatives / Windows HID APIs | Treat as a separate driver project; do not assume a drop-in replacement |
| macOS Bluetooth | CoreBluetooth bindings | Required for platform-specific BLE workflows and permissions |
| Android Bluetooth | Android Bluetooth APIs via JNI/NDK bindings | “Rust only” still requires the Android platform API boundary |
| Serialization | Serde + postcard/JSON | Serde models; postcard for compact sync packets, JSON for profiles/debugging |

Important feasibility constraint: no single Rust crate provides a complete,
uniform implementation of Bluetooth central mode, Bluetooth HID peripheral
mode, and virtual gamepad/keyboard output on every target OS. OpenWASD can keep
the product logic Rust-only, but platform adapters and virtual-device drivers
will necessarily bind to OS APIs and permissions.

## 7. Phase roadmap

### Phase 1 — Bluetooth foundation

Scope: discovery, pairing/trust, connection lifecycle, RSSI when available,
two-device connection UI, diagnostics. No remapping, macros, or virtual
controller.

Deliverable: connect two real devices and show both as connected in Makepad.

### Phase 2 — Input detection

Add gamepad, keyboard, and mouse capability detection; decode buttons, axes,
triggers, motion, and touchpad events; build a live input inspector.

### Phase 3 — BLEK-style companion output

Add Android keyboard, mouse, trackpad, presenter, media, keep-alive, voice and
clipboard actions, and custom layouts. The current Android Bluetooth HID slice
supports mouse and keyboard reports plus a gamepad layout that sends momentary
button and D-pad presses through a seven-byte HID gamepad report. It includes
16 buttons, a hat switch, and four centered axes; touch-layout button taps are
released automatically after 50 ms. This is a Bluetooth HID peripheral output
on Android, not a cross-platform virtual gamepad adapter. Validate the gamepad
descriptor against real macOS and other intended hosts before claiming broader
host compatibility. Continue with a single Android-to-desktop path before
generalizing.

An initial Android custom-controls editor now saves up to six ordered screen
buttons per layout, with keyboard, volume, mouse-button, gamepad-button and
Bluetooth-settings actions and optional learned hardware buttons. The custom
surface now has an Edit mode for adding, duplicating, removing, and reordering
buttons, with a separate detail view for labels, outputs, and learned hardware
buttons. In Edit mode, a compact notice explains that tapping a button opens
its editor and long-press dragging moves it; returning from the detail view
preserves Edit mode. The edit header shows the layout name, then switches to the
selected button name in its detail view. The detail editor provides Keyboard
shortcut, Function, Text, Hardware buttons, and Settings tabs. Keyboard shortcuts currently select
one HID key at a time; Function lists supported OpenWASD actions, and Text sends
ASCII using the US keyboard layout. Hardware button assignment has its own tab.
The Settings tab edits the button name and controls label/icon visibility, with
action-based default icons or a custom emoji/symbol. Edit controls are compact and draggable;
page titles, subtitles, icon buttons, and layout controls now share reusable
style components based on the Touchpad page toolbar and controls. The shared
top bar now uses the Gamepad toolbar's density-sized height on every page. On
the Layouts page, the subtitle stays inside the pinned top bar in smaller type,
and the layouts card uses a tighter top inset. The Add menu exposes Button,
Touchpad, Speech input, and Keyboard toggle. Only
Button creation is implemented; the other choices report their current lack of
custom-layout support. Long-press drag swaps buttons between grid slots, and
move controls remain available as an alternative outside the focused editor.
Controls aggregate held inputs and send releases when leaving the surface.
Buttons still occupy a fixed six-slot grid; free placement, styling, analog
controls, and background remapping remain future work. The legacy Makepad
Retroid Pocket 5 build and editor were checked on device, including adding,
removing, and dragging a control; Android queued press/release reports for a
connected Bazzite host. Those checks predate the Dioxus app and do not validate
its package or runtime behavior. Host-side input behavior and volume still
require independent verification.
The Layouts page also exposes a Custom controls entry on every platform; it
opens custom-layout creation regardless of the currently active input backend.
The top-bar plus opens a dedicated New layout page; the page returns to Layouts
from its back arrow or Cancel action. Creation asks only for a name, then saves
a custom layout with one default button named `clickme`. On a custom layout
surface, Edit and Settings remain available together; Edit exposes the Add
button menu.
The local Makepad and btleplug commits documented in the README must be
published before the legacy Makepad build can reproduce those earlier checks.

The Devices screen lists Linux USB hardware alongside Bluetooth. The original
Android companion bridge used ADB reverse forwarding; that implementation has
been replaced with Android Open Accessory mode over `nusb`, so runtime USB input
does not require USB debugging. Android opens the accessory through its USB
framework API, while Linux performs the AOA negotiation and bulk transfers in
Rust. Separate Linux uinput gamepad, keyboard and mouse devices are created per
session and removed when the session ends. The USB crate checks on Android and
Linux, the Android APK builds, and the Linux uinput output path is
hardware-tested. End-to-end testing on a Retroid Pocket 5 verified AOA
negotiation, accessory permission, a sustained connection, and an on-screen
gamepad button reaching Linux as a `BTN_SOUTH` press and release through uinput.
The stream now carries a synchronization marker with each report so the host
can attach or reconnect while the Android app is already running. Accessory-mode
support and Linux USB permissions vary by device and system configuration.
Device-specific Bluetooth input inspection remains unavailable.

### Phase 4 — Profiles and routing

Add Profiles, Configs, device groups, Sub-configs, Layers, trigger conditions,
context switching, migrations, and explicit Save versus Apply behavior.

### Phase 5 — Macro engine

Add recording/playback, delays, chords, sequences, toggles, cancellation,
timeouts, and safe-stop limits.

### Phase 6 — Remapping

Add gamepad-to-keyboard, gamepad-to-mouse, gamepad-to-gamepad,
keyboard-to-gamepad, and mouse-to-gamepad routing.

### Phase 7 — Virtual input

Add virtual keyboard, mouse, and gamepad adapters per OS. Maintain a capability
matrix rather than presenting unsupported output types as universally available.

### Phase 8 — Mobile companion

Add Android profile synchronization, remote layouts, device management,
Bluetooth control, and companion macros.

### Phase 9 — Advanced features

Add gyro, motion controls, flick stick, radial/touch menus, paddles, adaptive
triggers, vibration, and multi-device groups.

## 8. Phase 1 technical design

### 8.1 State model

```rust
pub type DeviceId = String;

pub enum DeviceKind {
    Gamepad,
    Keyboard,
    Mouse,
    Phone,
    Computer,
    Unknown,
}

pub enum Transport {
    BluetoothClassic,
    BluetoothLe,
    Usb,
    Wifi,
}

pub enum ConnectionState {
    Unknown,
    Discovered,
    Pairing,
    Paired,
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
    Disconnected,
    Failed { message: String },
}

pub struct BluetoothDeviceInfo {
    pub id: DeviceId,
    pub name: Option<String>,
    pub kind: DeviceKind,
    pub transports: Vec<Transport>,
    pub state: ConnectionState,
    pub rssi_dbm: Option<i16>,
    pub battery_percent: Option<u8>,
    pub services: Vec<String>,
    pub is_trusted: bool,
}
```

The ID must be stable across reconnects when the operating system provides a
stable identifier. MAC addresses should not be required on platforms that
privacy-rotate them; store an OS identity plus the adapter/transport context.

### 8.2 Events and commands

```rust
pub enum BluetoothEvent {
    ScanStarted,
    DeviceFound(BluetoothDeviceInfo),
    DeviceUpdated(BluetoothDeviceInfo),
    ScanFinished,
    PairingRequested { device_id: DeviceId },
    PairingChanged { device_id: DeviceId, trusted: bool },
    ConnectionChanged { device_id: DeviceId, state: ConnectionState },
    RssiUpdated { device_id: DeviceId, rssi_dbm: Option<i16> },
    Error { device_id: Option<DeviceId>, message: String },
}

pub enum BluetoothCommand {
    StartScan,
    StopScan,
    Pair(DeviceId),
    Unpair(DeviceId),
    Connect(DeviceId),
    Disconnect(DeviceId),
    Forget(DeviceId),
}
```

All commands must be idempotent. A second `Connect` on an already connected
device should return the current state rather than create another session.

### 8.3 Trait definitions

The traits should be async, but keep the public model independent of Tokio:

```rust
use async_trait::async_trait;

#[async_trait]
pub trait BluetoothAdapter: Send + Sync {
    async fn start_scan(&self) -> Result<(), BluetoothError>;
    async fn stop_scan(&self) -> Result<(), BluetoothError>;
    async fn known_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, BluetoothError>;
}

#[async_trait]
pub trait PairingService: Send + Sync {
    async fn pair(&self, device_id: &DeviceId) -> Result<(), BluetoothError>;
    async fn unpair(&self, device_id: &DeviceId) -> Result<(), BluetoothError>;
    async fn forget(&self, device_id: &DeviceId) -> Result<(), BluetoothError>;
}

#[async_trait]
pub trait ConnectionManager: Send + Sync {
    async fn connect(&self, device_id: &DeviceId) -> Result<(), BluetoothError>;
    async fn disconnect(&self, device_id: &DeviceId) -> Result<(), BluetoothError>;
    async fn reconnect(&self, device_id: &DeviceId) -> Result<(), BluetoothError>;
}

#[async_trait]
pub trait BluetoothDevice: Send + Sync {
    fn info(&self) -> &BluetoothDeviceInfo;
    async fn subscribe_events(&self) -> Result<(), BluetoothError>;
    async fn send(&self, data: &[u8]) -> Result<(), BluetoothError>;
    async fn close(&self) -> Result<(), BluetoothError>;
}
```

The `openwasd-bluetooth` crate now owns these adapter traits and lifecycle
events. Its default build includes a deterministic mock backend for tests, and
the desktop app enables a `btleplug` backend for live BLE discovery and
connect/disconnect. Pairing/unpairing is intentionally reported as OS-managed
because btleplug does not expose a portable pair API.

### 8.4 Tokio and Makepad integration

Use one Tokio runtime owned by the Bluetooth service, not one runtime per
device. The service publishes `BluetoothEvent`s to a bounded channel. The
Makepad app drains that channel from its event handler or a timer and updates
its `EngineState` on the UI thread.

```text
native adapter ─> Tokio BluetoothService ─> bounded event channel
                                             │
                                      Makepad event/timer
                                             │
                                      EngineState + redraw
```

Rules:

- Never block Makepad's event handler on Bluetooth I/O.
- Bound event and command queues to avoid runaway reconnect events.
- Cancel scan tasks explicitly on screen close or adapter shutdown.
- Use exponential reconnect backoff with a maximum duration.
- Preserve the last known device row while reconnecting.
- Surface permission errors distinctly from device-not-found errors.

### 8.5 Persistence

Phase 1 persistence only needs trusted-device metadata:

```rust
pub struct TrustedDevice {
    pub device_id: DeviceId,
    pub display_name: Option<String>,
    pub transport: Transport,
    pub auto_reconnect: bool,
    pub last_seen_unix_ms: u64,
}
```

Do not persist raw pairing keys in the profile format. Use the platform secure
storage/keyring later; Phase 1 may store only an OS reference and user
preference in an application data file.

## 9. Phase 1 Makepad UI

### Desktop screen

```text
┌──────────────────────────────────────────────────────────────────────┐
│ OpenWASD     Bluetooth foundation       Scanning…       Permissions   │
├───────────────────┬──────────────────────────────┬───────────────────┤
│ KNOWN DEVICES     │ AVAILABLE DEVICES            │ DEVICE DETAILS    │
│                   │                              │                   │
│ ● Laptop          │ ○ Living-room controller     │ Pixel companion   │
│   Connected       │   BLE HID · -54 dBm          │ Bluetooth LE       │
│                   │                              │ Services          │
│ ● Phone           │ ○ Wireless keyboard          │ HID, Battery      │
│   Connected       │   Classic · -67 dBm          │                   │
│                   │                              │ [Pair] [Connect]  │
│ [+ Add connection]│ [Scan] [Stop]                │ [Forget]          │
├───────────────────┴──────────────────────────────┴───────────────────┤
│ Connected devices:  [Laptop]  Connected  ↔  [Phone]  Connected       │
└──────────────────────────────────────────────────────────────────────┘
```

Interaction rules:

- Selecting a device updates the details panel without starting a connection.
- `Pair` is shown only when the platform allows a user-visible pairing action.
- `Connect` becomes `Disconnect` once connected.
- `Scan` changes to `Scanning…` and is disabled while a scan is active.
- RSSI is shown only when available; do not fabricate signal strength.
- The bottom connection strip is derived from state, not hardcoded labels.
- Empty states explain what action to take: enable Bluetooth, start scanning,
  or grant permission.

### Mobile companion screen

```text
┌─────────────────────────┐
│ OpenWASD                 │
│ Bluetooth               │
├─────────────────────────┤
│ Connected                │
│  Desktop                 │
│  Connected ·  -42 dBm    │
├─────────────────────────┤
│ Nearby devices           │
│  Keyboard                │
│  Gamepad                 │
│                         │
│ [Scan]                  │
└─────────────────────────┘
```

The mobile app should be a separate mode, not a compressed desktop workspace.
BLEK's strength is task-focused remote modes; preserve that simplicity.

## 10. Phase 1 implementation plan

1. Extract Bluetooth types from `openwasd-core` into a dedicated module/crate.
2. Add `async-trait`, Tokio, and a persistence abstraction.
3. Implement a fake adapter and deterministic state-machine tests first.
4. Implement Linux BlueZ/bluer discovery and connection path.
5. Implement macOS and Windows adapters behind feature/platform modules.
6. Add Android adapter integration as a separate app target; do not pretend the
   desktop adapter can run unchanged on Android.
7. Add Makepad device-list, details, scan, pair, connect, disconnect, and
   connected-devices views.
8. Add permission/error states and reconnect backoff.
9. Test with two real devices, including one BLE HID device and one phone or
   computer.
10. Record adapter capability gaps before starting Phase 2.

Suggested feature flags:

```toml
[features]
default = []
linux-bluez = ["dep:bluer"]
desktop-btleplug = ["dep:btleplug"]
android = []
```

Keep platform dependencies optional so core tests and UI development work
without Bluetooth hardware.

## 11. Phase 1 acceptance criteria

### Functional

- The app starts without Bluetooth hardware connected.
- The user can start and stop discovery.
- Discovered devices show stable ID, name when available, kind guess,
  transport, and RSSI when available.
- The user can select a device and see details without connecting it.
- Pair, unpair, connect, disconnect, forget, and reconnect are represented by
  explicit commands and states.
- At least two supported platforms have a working native adapter before the
  phase is marked complete; Linux should be the first reference adapter.
- Two real devices can be connected simultaneously and appear in the bottom
  connection strip.
- A disconnected device remains visible with a reconnect action.
- Permission failures are actionable and do not crash the app.

### Engineering

- Core tests cover state transitions, duplicate commands, reconnect backoff,
  and stale device events.
- Adapter tests use a fake backend and do not require hardware.
- UI tests verify scan, select, pair, connect, disconnect, and error states.
- Makepad's event loop never blocks on Bluetooth I/O.
- No UI device names, connection statuses, RSSI values, or counts are
  hardcoded; all are derived from engine state.
- Logs redact pairing material and stable identifiers unless diagnostics are
  explicitly enabled.

## 12. Risks and platform limitations

| Risk | Impact | Mitigation |
|---|---|---|
| BLE central APIs differ by OS | Discovery/connection behavior diverges | Normalize events; keep platform capability flags |
| BLE HID peripheral mode is not uniform | BLEK-style output may not work everywhere | Separate central and peripheral adapters; test Android first |
| Virtual gamepad drivers need privileged OS support | Phase 7 may be platform-specific | Ship keyboard/mouse and physical routing first |
| Android background restrictions | Companion disconnects or misses events | Foreground service and explicit lifecycle design |
| Privacy-rotating Bluetooth IDs | Trusted devices appear duplicated | Use OS identity plus adapter context and user confirmation |
| Pairing may be OS-mediated | App cannot always force pair/unpair | Expose “Open system Bluetooth settings” fallback |
| Makepad/Tokio event integration | UI stalls or race conditions | Single service runtime, bounded channels, UI-thread state updates |
| HID reports are vendor-specific | Gamepad normalization is incomplete | Preserve raw reports and ship descriptor diagnostics |
| Reconnect storms | Battery drain and noisy UI | Capped exponential backoff and user disable switch |
| Licensing/compatibility | Cannot copy proprietary behavior/assets | Reimplement concepts, use original OpenWASD terminology/assets |

## 13. Definition of done for the roadmap

The roadmap is complete when Phase 1 can connect two real devices reliably,
show truthful state in Makepad, and provide a stable adapter boundary for the
later HID, mapping, profile, and virtual-output phases. The first milestone is
not a partial reWASD clone; it is a trustworthy Bluetooth foundation on which
those features can be built.
