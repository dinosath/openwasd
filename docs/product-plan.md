# OpenWASD Product Plan

Status: Discovery and product scope
Audience: Maintainers, contributors, designers, and community partners
Planning horizon: v1 through ecosystem expansion

## 1. Product vision

### Mission

OpenWASD makes every input device adaptable. It gives people an open, cross-platform way to remap controllers, keyboards, mice, touch surfaces, and networked companion devices into the controls that fit their game, body, workflow, and imagination.

### Positioning

OpenWASD is an open-source Rust input customization platform: the flexibility of a remapper, the composability of a plugin system, the immediacy of a macro pad, and the reach of a companion-screen network. Its core model is platform-independent; platform adapters provide the input and virtual-device capabilities that each operating system permits.

### Product promise

- Configure an input setup once and use it across supported devices and operating systems.
- Make advanced configuration understandable to beginners and powerful for experts.
- Treat phones and tablets as first-class extensions of a controller, not merely remote controls.
- Keep profiles portable, inspectable, shareable, and safe to disable.
- Be transparent about permissions, drivers, network traffic, and anti-cheat limitations.

### Guiding principles

1. Local-first: basic remapping must work without an account or cloud service.
2. Safe by default: profiles are scoped, reversible, visible, and easy to stop.
3. Capability-aware: the UI explains what a platform or device can and cannot do.
4. Open formats: user-owned profiles, widgets, and mappings should be exportable.
5. Progressive complexity: simple remaps are immediate; layers, conditions, and automation are discoverable later.
6. Accessibility is a core use case, not a premium mode.
7. Networked input is explicit: pairing, permissions, latency, and trust are always visible.

### Scope boundaries

OpenWASD is not initially a game launcher, game database, game telemetry cheat, cloud gaming service, or general-purpose automation tool. It may integrate with those ecosystems later, but its center of gravity is input routing and user-configured control surfaces.

## 2. Target users and jobs to be done

| Persona | Job to be done | Success signal |
|---|---|---|
| Casual gamer | Fix an awkward layout or use one controller with multiple games | A profile can be created and activated in minutes |
| Competitive gamer | Reduce friction while preserving predictable, low-latency input | Reliable mappings, clear latency, fast profile switching |
| Streamer | Add scene controls, chat actions, and visible control surfaces without leaving the game | Companion actions are safe, labeled, and stream-friendly |
| Accessibility user | Operate games with alternate inputs, holds, toggles, chords, and assisted controls | A setup can be personalized without specialist hardware |
| Steam Deck user | Adapt controls per game while preserving handheld ergonomics and battery life | Profiles work offline and survive sleep/resume |
| Mobile gamer | Combine a telescopic controller with touch-only game actions | Touch widgets complement rather than fight the game |
| Power user | Build conditional layers, macros, virtual devices, and reusable assets | Complex setups remain inspectable and portable |
| Hardware enthusiast | Extend unsupported or unusual devices | Community adapters are discoverable and safe |
| Developer / tester | Generate reproducible input scenarios | Profiles can be versioned, logged, and replayed with consent |

## 3. Product model

OpenWASD should use a small set of user-facing concepts:

- Device: a physical input source or connected companion device.
- Action: a button, axis, key, mouse operation, touch gesture, macro, or system command.
- Mapping: the relationship between an input and one or more actions.
- Layer: an alternate mapping set activated by a button, mode, condition, or context.
- Profile: the complete configuration for a game, application, device group, or general use.
- Virtual device: an output presented to the operating system, such as a virtual gamepad, keyboard, mouse, or MIDI-like control surface where supported.
- Companion: a trusted phone or tablet that can send input, display status, and render widgets or overlays.
- Asset: a shareable profile, widget, macro, theme, icon pack, overlay, device definition, or plugin.

### Connectivity model

Connectivity is a product capability, not an implementation detail. OpenWASD should support three primary transport families:

| Transport | Intended uses | Product considerations |
|---|---|---|
| Bluetooth | Wireless controllers, telescopic controllers, phones, tablets, and short-range companion links | Pairing, battery state, reconnect behavior, latency, and OS-specific Bluetooth permissions |
| USB | Wired controllers, phones/tablets in tethered modes, diagnostics, and reliable low-latency input | Hot plug, device identity, permissions, cable/accessory compatibility, and Android USB host behavior |
| Wi-Fi | Companion screens, Android-to-PC, Android-to-Mac, Android-to-Android, tablet dashboards, and networked input routing | Secure pairing, local discovery, encryption, latency, jitter, packet loss, firewall behavior, and offline LAN operation |

The UI should show the active transport, link health, estimated latency where measurable, battery state where available, and a clear reconnect or stop action.

## 4. Complete feature inventory

Complexity is relative product complexity: S, M, L, or XL. Platform dependencies identify where support is likely to vary.

### Device discovery and input

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Device discovery | Find connected controllers, keyboards, mice, touch devices, and companions over Bluetooth, USB, and Wi-Fi | Immediate visibility into available hardware and connection path | M | OS enumeration; Bluetooth, USB, and network permissions |
| Connectivity management | Pair, authorize, connect, disconnect, reconnect, and forget devices across Bluetooth, USB, and Wi-Fi | Gives users predictable control over every link | L | OS pairing APIs, USB permissions, local-network discovery |
| Device identity | Remember devices by stable identity and show connection state | Prevents mappings moving to the wrong device | M | HID metadata quality |
| Generic HID support | Support common gamepad, keyboard, mouse, and axis reports | Broad initial hardware coverage | L | OS HID APIs; device quirks |
| Controller diagnostics | Show buttons, axes, hats, ranges, dead zones, and raw events | Makes unknown hardware usable | M | Raw-event access varies |
| Telescopic controller support | Detect GameSir G8, Backbone, Razer Kishi, EasySMX, and generic Android HID classes | Makes mobile controls a first-class source | L | Android HID and vendor behavior |
| Hot plug and reconnect | Preserve active configuration through Bluetooth, USB, and Wi-Fi disconnects, reconnects, and sleep | Fewer failed sessions | M | OS lifecycle APIs and transport behavior |
| Input normalization | Present consistent buttons, axes, triggers, and gestures to the mapping engine | Portable profiles across devices | L | Hardware-specific reports |
| Input recording | Record a user-approved event sequence for inspection or replay | Helps diagnose and create macros | M | Event capture permissions |

### Mapping and control behavior

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Button remapping | Map physical buttons to other buttons or keys | Core customization | M | OS input APIs |
| Axis remapping | Swap, invert, split, combine, or scale axes | Better ergonomics and device compatibility | L | Axis semantics and virtual output |
| Dead zones and curves | Tune dead zones, sensitivity, response curves, and trigger thresholds | Precise control | M | Normalized axis support |
| Chords | Use simultaneous or sequential inputs as one action | More actions without more hardware | M | Event timing |
| Toggle and hold modes | Convert hold to toggle, toggle to hold, or tap/long-press | Accessibility and comfort | M | Timing state machine |
| Rapid-fire / turbo | Repeat an action while held or toggled | Useful for supported single-player use cases | M | Timing; game policy concerns |
| Layer switching | Momentarily or persistently switch mapping layers | Enables many controls on compact hardware | L | Profile state and UI feedback |
| Modes and contexts | Select behavior based on active app, profile, device, or companion state | Keeps setups relevant | L | Process detection and permissions |
| Haptics and LEDs | Trigger or configure supported feedback | Makes modes discoverable | L | Vendor and OS APIs |
| Output translation | Convert gamepad input to keyboard, mouse, or other supported output | Enables games with mismatched control models | L | Virtual-device privileges |
| Input smoothing | Filter noisy axes or touch events | Better feel on imperfect hardware | M | Sampling and latency trade-offs |

### Macros and automation

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Macro recorder | Capture and edit button, key, mouse, and delay sequences | Easy automation authoring | M | Capture permissions |
| Macro editor | Edit timing, repeats, branches, waits, and cancellation behavior | Predictable advanced automation | L | Core engine; safety rules |
| Macro cancellation | Stop on release, panic key, timeout, profile change, or disconnect | Prevents runaway actions | M | Core engine |
| Conditional macros | Branch on layer, input state, companion control, or allowed app context | Powerful workflows | XL | Context signals and policy |
| Accessibility sequences | Sticky keys, repeated presses, dwell, switch scanning, and tremor filtering | Expands participation | L | OS accessibility permissions |
| Macro library | Store, tag, duplicate, and audit macros | Reuse and maintenance | M | Local storage |
| Rate and safety limits | Cap repetition, duration, and concurrent actions | Responsible automation | M | Core policy |
| Profile triggers | Activate a profile by app, shortcut, device, companion, or manual selection | Reduces setup friction | L | OS process APIs |

### Profiles, layers, and configuration

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Profile creation | Build a named configuration for a game or use case | Organizes setups | S | None beyond storage |
| Profile inheritance | Reuse a base profile with game or device overrides | Avoids repetitive work | L | Profile format |
| Profile import/export | Move configurations as human-readable packages | Portability and open collaboration | M | Stable schema |
| Version history | Compare, restore, and label profile revisions | Recovery and experimentation | M | Local storage |
| Layer editor | Visually define layer inputs, outputs, and activation rules | Makes advanced control understandable | L | Core mapping model |
| Conflict detection | Explain duplicate outputs, unreachable actions, and overlapping layers | Prevents silent errors | L | Static analysis |
| Profile permissions | Declare device, app, network, and macro capabilities | Builds trust for shared assets | L | OS permission model |
| Backup and migration | Export settings and restore on another machine | Protects user investment | M | File access |

### Virtual devices and routing

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Virtual gamepad | Expose mapped output as a virtual controller | Supports games with fixed input expectations | XL | Drivers, entitlements, kernel or system extensions |
| Virtual keyboard | Emit mapped keyboard events | Enables controller-to-keyboard layouts | L | Accessibility/input injection permissions |
| Virtual mouse | Emit cursor and click events from controller or touch | Enables pointer games and couch use | L | Accessibility/input injection permissions |
| Multi-output routing | Send different actions to multiple outputs | Supports hybrid control schemes | XL | OS device APIs |
| Device chaining | Route one device through another profile or companion | Extends hardware capability | XL | Network and lifecycle reliability |
| Network input transport | Pair devices and send normalized input over Wi-Fi using a secure local-network connection | Android-to-PC, Android-to-Mac, tablet-to-PC, and Android-to-Android scenarios | L | Pairing, discovery, encryption, latency, firewall |
| USB companion transport | Use USB tethering or direct USB connection for companion input and configuration where supported | Provides a reliable alternative when Wi-Fi is unavailable or congested | L | USB modes, Android host/device roles, platform permissions |
| Bluetooth companion transport | Use Bluetooth for nearby companion input and control surfaces where bandwidth permits | Enables wireless low-power companion scenarios | L | Bluetooth profiles, pairing, bandwidth, mobile background limits |
| Bluetooth HID gamepad output | Present an Android companion as a Bluetooth gamepad with button, hat-switch, and axis reports | Lets touch layouts control hosts that expect a physical gamepad | M | Android HID peripheral support and host report compatibility |
| Output monitoring | Show what the target game or OS receives | Debugging and trust | M | Output API visibility |

### Companion screen and overlays

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Companion pairing | Pair a phone or tablet over Bluetooth, USB, or Wi-Fi with explicit trust and revocation | Secure extension device setup across connectivity options | L | Bluetooth pairing, USB permissions, local network, QR/deep link |
| Companion controller | Use touch, sensors, and buttons as input | Adds controls anywhere | L | Mobile OS input and networking |
| Companion macro pad | Render labeled macro buttons and layers | More actions than physical hardware allows | L | Mobile UI and transport |
| Companion status screen | Show profile, layer, connection, battery, and warnings | Keeps state understandable | M | Companion app |
| Widget canvas | Arrange buttons, sliders, radial menus, text, images, and indicators | Custom control surfaces | XL | Cross-device rendering model |
| Overlay renderer | Display game-specific controls or information over companion or desktop | Contextual controls | XL | Desktop/mobile overlay permissions |
| Touch zones | Define zones with tap, hold, swipe, and multi-touch semantics | Precise touch-only actions | L | Touch event APIs |
| Gamepad-plus-touch mode | Keep a telescopic gamepad as the primary source while adding touch widgets | Solves hardware button limits | L | Android controller behavior |
| Companion-to-companion mode | Use one Android device as controller/display for another over Wi-Fi, Bluetooth, or USB where supported | New mobile workflows | XL | Android networking, Bluetooth, USB roles, and permissions |
| Tablet dashboard | Use a large screen for dense controls and information | Better for simulation and strategy | L | Tablet layout and power |

### Platform, administration, and trust

| Feature | Description | User value | Complexity | Platform dependencies |
|---|---|---|---|---|
| Windows adapter | Windows input discovery, injection, and virtual output capabilities | Primary desktop reach | XL | Driver/signing and security APIs |
| macOS adapter | macOS input and permitted virtual output capabilities | MacBook use cases | XL | Accessibility/Input Monitoring, DriverKit limitations |
| Linux adapter | X11/Wayland-aware input and virtual-device options | Open desktop and Steam Deck reach | XL | udev, uinput, compositor policy |
| Android adapter | HID, accessibility, overlay, network, and lifecycle integration | Mobile controller and companion use | XL | Android version/vendor restrictions |
| Steam Deck mode | Handheld-friendly UI, offline use, sleep/resume, gaming-mode integration | First-class Deck experience | L | SteamOS and compositor constraints |
| ARM Linux support | Build and runtime support for ARM boards and handhelds | Broadens device ecosystem | L | Packaging and hardware availability |
| iOS research track | Evaluate controller, network, and companion capabilities | Future Apple mobile reach | XL | Strict sandbox and background/overlay restrictions |
| Permission center | Explain and manage OS permissions | Prevents confusing setup failures | M | Per-platform permission APIs |
| Safe stop / panic action | Immediately disable mappings and virtual outputs | User control and risk reduction | S | Adapter shutdown behavior |
| Diagnostics and logs | Consent-based logs for device, transport, and profile behavior | Faster support and debugging | M | Local storage and privacy |
| Localization and theming | Translate UI and support light/dark/high-contrast themes | Wider adoption | M | Dioxus and asset pipeline |

## 5. MVP definition (v1)

The MVP should prove that OpenWASD is a dependable remapper, not attempt to solve the entire companion ecosystem.

### Required for v1

- Desktop-first support for Windows and Linux, with macOS discovery and remapping scoped according to validated OS capabilities.
- Cross-platform desktop UI with device list, event tester, profile list, mapping editor, layer indicator, and safe-stop control.
- Generic gamepad, keyboard, and mouse discovery where permitted.
- Bluetooth and USB connectivity for local devices, plus Wi-Fi connectivity for local companion and network-input scenarios where supported.
- Button remapping, axis inversion/sensitivity, dead zones, tap/hold, toggle, and one momentary layer.
- Controller-to-controller and controller-to-keyboard mappings where the platform can safely provide output.
- Profile creation, activation, import/export, backup, and recovery.
- Hot plug, reconnect, clear errors, and diagnostics.
- Explicit permissions and a transparent capability matrix.
- Basic macro recording with delays, repeat limits, cancellation, and a visible active-state indicator.
- A stable, documented profile format and versioning policy.

### Explicitly out of v1

Virtual kernel-level devices on every platform, game telemetry, cloud accounts, public marketplace, iOS implementation, arbitrary scripting, stealth behavior, anti-cheat bypasses, and a full overlay editor. These may be researched, but they should not delay a trustworthy first release.

### MVP acceptance outcomes

- A new user can connect a controller, remap a button, save a profile, and activate it in under five minutes.
- A power user can create a layer and bounded macro without editing files.
- A disconnected device or stopped profile cannot leave input stuck.
- A user can identify why a feature is unavailable on their OS.
- Profiles can be backed up and restored without an account.

## 6. Roadmap

Dates should be set after hardware and OS spikes. The phases below are capability gates, not promises of calendar delivery.

### Phase 1: dependable remapping release

Focus: a polished, local-first v1 for desktop and Steam Deck-oriented Linux.

- Stabilize the core profile, mapping, layer, macro, and diagnostics concepts.
- Complete Windows and Linux adapters; validate macOS input discovery and permitted output paths.
- Add controller normalization for common XInput, DirectInput, SDL, HID, and Steam Deck patterns.
- Ship signed/reproducible packages, crash-safe profile storage, and clear permission onboarding.
- Add accessibility foundations: sticky/toggle behavior, hold-to-toggle, dwell timing, remapable panic action, and large/high-contrast controls.
- Publish contribution documentation for device reports, profile assets, and platform testing.

Exit criterion: stable everyday remapping with no dependence on cloud services and a supportable failure model.

### Phase 2: platform and companion expansion

Focus: make OpenWASD useful across phones, tablets, macOS, Android, and more hardware.

- Android companion app with QR pairing, Wi-Fi transport, Bluetooth transport where appropriate, USB fallback where supported, controller mode, macro pad, status screen, and touch zones.
- Telescopic controller profiles for GameSir G8, Backbone, Razer Kishi, EasySMX, and generic Android HID.
- Tablet layouts for racing, flight, simulation, RTS, and accessibility control panels.
- Virtual keyboard/mouse and gamepad output on platforms where supported and responsibly packaged.
- Profile triggers, device groups, inheritance, conflict detection, and profile sharing via files.
- Steam Deck gaming-mode-friendly companion workflow and sleep/resume hardening.
- ARM Linux builds and hardware validation on selected devices.
- macOS output research and a documented capability boundary.

Exit criterion: a user can keep a physical gamepad connected while using an Android phone or tablet as a trusted extension with predictable latency.

### Phase 3: ecosystem and advanced experiences

Focus: a safe open platform for community-created control surfaces and context-aware experiences.

- Widget/overlay authoring, themes, icons, and responsive layouts.
- Game-aware profile discovery based on explicit user selection and community metadata, not invasive telemetry.
- Plugin SDK and permissioned capability model.
- Local or hosted marketplace with review, versioning, signatures, compatibility, and rollback.
- Companion-to-companion Android scenarios and richer synchronized displays.
- Haptics, LEDs, sensor input, accessibility switch scanning, and multi-device routing.
- Optional integrations with streaming software, launchers, chat, OBS scenes, and simulation dashboards.
- iOS feasibility implementation only where platform policy permits a useful experience.

Exit criterion: third parties can create and share useful assets without requiring unsafe arbitrary code or maintainers to manually merge every device definition.

## 7. Companion Screen platform

### Product concept

A companion screen is a trusted phone or tablet session associated with a profile. It may be an input source, display, macro pad, HUD, dashboard, or all of these at once. The physical gamepad remains the primary low-latency device when appropriate; the companion adds controls that do not fit on the device.

The platform should support compact presets first, then a declarative widget canvas. Every companion session needs visible connection state, latency, battery, active profile/layer, and a stop/disconnect action.

### Interaction categories and brainstorm (60 ideas)

1. Extra virtual face buttons outside the physical gamepad layout.
2. Side-screen action strips for a telescopic controller.
3. Tap, hold, double-tap, and long-press widgets.
4. Programmable touch zones mapped to normalized axes.
5. Swipe-to-cycle weapon or inventory controls.
6. Radial menus for abilities, tools, or emotes.
7. MMO hotbars with cooldown coloring.
8. RPG inventory quick actions.
9. Potion, healing, and consumable panels.
10. Quest objective and map shortcut panel.
11. Companion creature or party-member commands.
12. Racing telemetry: speed, gear, lap, tire, and fuel indicators.
13. Racing pit-stop and radio control panel.
14. Flight-sim cockpit instrument buttons.
15. Trim, autopilot, radio, and autopilot mode controls.
16. Space-sim power distribution and targeting panel.
17. RTS command palette and unit-group buttons.
18. Strategy-game build queue and production controls.
19. Minecraft quick-build and hotbar extension.
20. Building-game blueprint and rotate controls.
21. Stealth-game visibility and gadget panel.
22. Fighting-game training macros and move reference panel.
23. Rhythm-game timing calibration and lane controls.
24. Sports-game playbook and formation panel.
25. Fishing-game reel, tension, and lure controls.
26. Farming-game tool and crop shortcut panel.
27. Simulation-game camera and instrument dashboard.
28. Photo-mode camera and filter controls.
29. Streaming scene, source, and microphone controls.
30. Chat moderation buttons with confirmation states.
31. Music-player controls while gaming.
32. Discord push-to-talk and mute surface.
33. Screenshot, clip, and replay controls.
34. Game timer, speedrun split, and reset controls.
35. Accessibility panel for alternate mappings and dwell input.
36. One-handed control surface with large targets.
37. Switch-scanning companion UI.
38. Eye-gaze or head-tracking companion input bridge where supported by other apps.
39. Haptic confirmation tester and intensity controls.
40. Color-blind-safe status indicators using shapes and labels.
41. Profile/layer switcher with physical-device feedback.
42. Battery and connection dashboard for all devices.
43. Latency and packet-loss monitor.
44. Device calibration wizard displayed on the tablet.
45. Training mode that labels the active output.
46. Contextual control cards selected by the active profile.
47. Custom icon and image buttons.
48. Multi-page workspaces with swipe navigation.
49. Lockable play mode to prevent accidental editing.
50. Parent/child safe mode with bounded macros and hidden settings.
51. Local multiplayer second-player controller.
52. Android-to-Android controller for a second phone or tablet.
53. Tablet as a shared spectator map or event dashboard.
54. Cooperative puzzle-game shared control panel.
55. LAN party profile and pairing station.
56. Board-game and tabletop companion controls.
57. Virtual keyboard with game-specific key clusters.
58. Touch mousepad plus scroll and gesture modifiers.
59. Macro search palette with recently used actions.
60. Context-sensitive tutorial and onboarding hints.

### Companion modes

- Controller mode: touch, sensors, and connected mobile hardware produce normalized input.
- Extension mode: physical controller remains primary; companion supplies extra actions.
- Macro-pad mode: large, labeled actions with bounded automation.
- HUD mode: read-only status and game-independent dashboards.
- Overlay mode: controls rendered over or beside a game where the OS allows it.
- Mirror mode: show active mappings, layers, and device states for learning or accessibility.
- Tablet console mode: dense, multi-page control surface for simulation and strategy.
- Companion-to-companion mode: one mobile device acts as a control surface for another through the host.

### Companion guardrails

Pairing must use a visible code or QR flow, revocable trust, and local-network disclosure. Assets must declare whether they send input, display data, access files, or use network services. The user must be able to lock a surface, disable macros, and stop all sessions from either endpoint.

## 8. Plugin system opportunities

Plugins should extend capabilities without owning the core safety model. Prefer declarative extensions and permissioned APIs over arbitrary code execution.

Good plugin candidates:

- Device definitions and report normalizers.
- Platform adapters and virtual-output providers.
- Companion widgets, layouts, themes, and icon packs.
- Profile triggers and application detectors.
- Macro nodes and bounded automation actions.
- Haptic, LED, sensor, and battery integrations.
- Stream and chat integrations.
- Simulation telemetry connectors, only with explicit opt-in and game policy review.
- Accessibility input providers such as switches or alternative pointers.
- Importers/exporters for existing profile formats.
- Community marketplace providers and offline catalogs.
- Diagnostics exporters and test fixtures.

Plugins should have declared permissions, versioned APIs, compatibility metadata, provenance, revocation, sandboxing where feasible, and a clear distinction between trusted native adapters and untrusted content assets.

## 9. Marketplace possibilities

The marketplace can begin as a signed, downloadable asset index rather than a mandatory hosted service. Asset types include profiles, device definitions, widgets, overlays, companion apps, macros, icon packs, themes, calibration presets, accessibility layouts, tutorial packs, and plugin packages.

Required marketplace qualities:

- Compatibility by OS, device, game/application, OpenWASD version, and output type.
- Human-readable permissions and behavior summaries.
- Version history, changelog, author identity, license, and source link.
- Community ratings plus issue reporting, moderation, malware scanning, and takedown process.
- Offline import/export, local collections, pinning, and rollback.
- Review labels such as community, verified, accessibility-focused, and experimental.
- No hidden macros, network calls, or executable behavior in a profile asset.

Potential business models for hosted infrastructure include donations, sponsorship, optional paid creator assets, and organization plans; the core app and local workflow should remain useful without a paid account.

## 10. Accessibility

Accessibility should influence the profile model, editor, runtime, and companion surfaces.

- Remap every action, including profile switching and safe stop.
- Tap, hold, toggle, sticky, repeat, dwell, and debounce behaviors.
- Chords and sequential alternatives for limited mobility.
- Switch scanning, auto-scan timing, and one-switch activation.
- Analog-to-digital conversion and digital-to-analog emulation.
- Tremor filtering, smoothing, hysteresis, and accidental-touch rejection.
- Adjustable macro timing with bounded maximum duration.
- One-handed and left/right mirrored layouts.
- Large targets, spacing presets, reduced motion, high contrast, color-independent states, and dyslexia-friendly text options.
- Screen-reader labels and accessible focus order in the desktop and companion editors.
- Spoken or haptic layer/profile announcements.
- Visual timer and progress cues for holds, cooldowns, and macros.
- Fatigue-aware alternation between inputs where user-configured.
- Emergency stop reachable without precision movement.
- Clear permission explanations and keyboard-only configuration.
- Offline operation and exportable settings for support workers or caregivers.
- Safe, bounded automation presets with no implication that automation replaces player agency.

## 11. Competitive analysis

| Product | Strengths | Gaps / constraints | OpenWASD opportunity |
|---|---|---|---|
| reWASD | Deep remapping, layers, virtual devices, polished Windows experience | Proprietary, Windows-centered, paid, limited open ecosystem | Cross-platform core, open profiles, companion screens, auditable safety |
| Steam Input | Broad controller compatibility, Steam integration, community layouts | Best inside Steam; complexity and limited general-purpose companion model | Standalone profiles, phone/tablet surfaces, broader app workflows |
| BetterJoy | Useful Nintendo controller bridge | Narrower purpose and platform scope | General remapping and device extension with a reusable adapter model |
| Controller Companion | Couch keyboard/mouse navigation | Less focused on rich game profiles and open device ecosystem | Input translation plus layers, macros, accessibility, and companions |
| DS4Windows | Strong DualShock/DualSense support and virtual Xbox output | Windows-specific; driver and maintenance complexity | Multi-platform, device-neutral core, explicit capability boundaries |
| Input Remapper | Linux-friendly remapping and automation | Smaller ecosystem and less companion/virtual-device breadth | Preserve Linux openness while adding polished profiles and mobile extensions |
| AntiMicroX | Accessible controller-to-keyboard/mouse mappings | Limited modern companion and ecosystem experience | Make advanced concepts approachable and network-capable |

Differentiation should come from the combination, not one isolated feature: portable open profiles, a platform-aware Rust core, telescopic-controller workflows, companion screens, accessible interaction design, and a permissioned community ecosystem.

## 12. Risks and mitigations

| Risk | Impact | Mitigation / product decision |
|---|---|---|
| Virtual-device driver complexity | Delayed or fragile platform support | Treat adapters as capability modules; ship useful non-virtual modes first |
| Windows signing and security software | Installation friction and false positives | Reproducible builds, transparent binaries, signed releases, clear docs |
| macOS Input Monitoring and Accessibility restrictions | Incomplete output or background behavior | Publish exact support matrix; pursue permitted user-space paths; avoid promises |
| iOS sandbox and background/overlay limits | Companion mode may be restricted | Research only after Android validates the model; design graceful reduced modes |
| Linux X11/Wayland fragmentation | Different behavior across desktops | Capability detection, compositor-specific documentation, focused test matrix |
| Android vendor fragmentation | HID, overlays, battery, and background limits vary | Foreground-service guidance, reconnect design, device profiles, graceful fallback |
| Apple ecosystem limitations | Cross-device scenarios may not map cleanly | Separate product promise from platform availability |
| Latency and packet loss | Companion input feels unreliable | Local-first transport, visible telemetry, bounded buffering, physical fallback |
| Anti-cheat detection or policy violation | Users may be banned or distrust product | No stealth or bypass features; educate users; default to transparent, bounded behavior |
| Macro abuse | Reputational and competitive harm | Rate limits, app-context warnings, user-visible active state, disable by default in competitive contexts |
| Arbitrary plugin code | Security and supply-chain exposure | Permission model, signing, sandboxing, review, and separable native adapters |
| Profile conflicts | Silent wrong input | Static conflict analysis, test mode, rollback, and output monitor |
| Scope explosion | Slow delivery and unclear product | Gate roadmap by core remapping quality and validated companion use cases |
| Hardware maintenance burden | Community fragmentation | Generic HID baseline, device-definition format, contributor tests, compatibility labels |
| Accessibility regressions | Excludes core users | Accessibility acceptance criteria and dedicated testing community |
| Power and thermal use on mobile | Poor companion experience | Low-frequency HUD mode, screen dimming, battery warnings, reconnect strategy |
| Legal and licensing issues | Takedowns or incompatible assets | Clear asset licenses, trademarks policy, and no redistribution of proprietary game data |

## 13. Final prioritization matrix

### Must Have

Bluetooth and USB device connectivity; Wi-Fi connectivity for local companion scenarios; core device discovery; generic gamepad/keyboard/mouse input; controller diagnostics; input normalization; button and axis remapping; dead zones and curves; tap/hold/toggle; one momentary layer; profile creation and activation; import/export and backup; hot plug/reconnect; safe stop; permission center; diagnostics; bounded macro recording; Windows and Linux baseline; Steam Deck-friendly operation; accessible editor basics; stable open profile format.

### Should Have

macOS baseline; telescopic controller definitions; profile triggers; profile inheritance; conflict detection; virtual keyboard and mouse where supported; virtual gamepad adapters where supportable; device groups; haptics and LEDs; Android companion pairing; controller mode; macro-pad mode; touch zones; companion status screen; tablet layouts; ARM Linux packaging; profile sharing; compatibility metadata; plugin permissions; signed asset packages; accessibility switch scanning and tremor filtering.

### Could Have

Companion HUDs; game-specific widgets; radial menus; MMO hotbars; cooldown displays; racing telemetry; flight cockpit panels; RTS command palettes; streaming integrations; chat controls; screenshot/clip controls; overlay rendering; companion-to-companion Android mode; multi-output routing; sensors; richer haptic patterns; themes and icon packs; marketplace ratings; creator monetization; simulation connectors; local multiplayer surfaces; profile recommendation; tutorial packs.

### Future research

iOS implementation beyond permitted companion modes; deep game telemetry; automatic game-state detection; arbitrary scripting; cloud synchronization; remote internet routing; kernel-level or stealth virtual devices; anti-cheat-sensitive automation; AI-generated profiles; eye-gaze/head-tracking integrations; cross-device shared worlds; commercial hardware certification; hosted marketplace as a required service.

## 14. Planning metrics and validation questions

Measure whether the product is becoming dependable and extensible, not merely feature-rich:

- Time to first successful remap.
- Percentage of sessions with successful device discovery and reconnect.
- Profile activation failures by platform and cause.
- Macro cancellation and safe-stop reliability.
- Companion round-trip latency, jitter, and reconnect rate.
- Percentage of imported profiles that validate without conflicts.
- Accessibility task completion with keyboard-only and alternative-input testing.
- Number of community device definitions and assets that pass compatibility checks.
- Crash-free sessions and support issues per supported platform.

Before architecture begins, validate which virtual-output techniques are legally and technically supportable on each target OS; which Android telescopic devices expose standard HID reports; whether users prefer local file sharing or a catalog; and whether companion widgets solve real game problems better than simply displaying a second copy of the desktop UI.

## 15. Recommended next planning artifacts

1. A platform capability matrix for Windows, macOS, Linux, Android, SteamOS, ARM Linux, and iOS.
2. A prioritized set of user-story acceptance tests for the MVP.
3. A device compatibility test plan covering common and generic HID controllers.
4. A companion-screen concept prototype specification with three reference experiences: Android-to-PC extension, tablet flight dashboard, and accessibility control surface.
5. A threat model for profiles, plugins, pairing, macros, and marketplace assets.
6. A community contribution guide defining device definitions, profile packages, and support expectations.
