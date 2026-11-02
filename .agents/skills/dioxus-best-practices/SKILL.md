---
name: dioxus-best-practices
description: Apply OpenWASD's Dioxus 0.7 UI conventions when building, reviewing, or refactoring the app interface.
---

# Dioxus best practices for OpenWASD

Use this skill for UI work in `crates/openwasd-app`.

## Reuse the interface

- Keep the app shell and top bar shared across screens. Put shared navigation, title, and page actions in one component and express screen differences through props and state.
- Keep layout and control-surface rendering in reusable components. If a control surface is available from more than one route or screen, render the same layout-view component in each place instead of recreating its RSX or styles.
- Extract a component when repeated UI or behavior needs to stay consistent. Keep one-off, screen-specific markup local.
- Put UI behavior in Rust components and styles in `crates/openwasd-app/assets`; use Dioxus 0.7 RSX and signals.

## State and platform behavior

- Keep view state in Dioxus `Signal`s and derive visible UI from that state. Keep device, transport, and input behavior in its owning crate; the app orchestrates stable crate APIs.
- Prefer semantic controls and accessible names for icon-only buttons. Keep tap targets usable on touch screens and avoid relying on hover.
- For drag, resize, and other gestures, support pointer/touch input on Android WebView as well as mouse input on desktop. Native HTML drag-and-drop alone may not provide a usable touch interaction; provide and verify a touch-compatible path.
- Keep shared layouts responsive and preserve the same visual button treatment between view and edit modes. Editing affordances should enable the operation without replacing the surface control with an unrelated visual control.
- Keep headers compact, reserve the available screen area for the active surface, and use consistent edge spacing across screens.

## Validation

- Read the relevant parts of `AGENTS.md`, product plan, and roadmap before changing app behavior.
- Build with Dioxus CLI (`dx`). When an Android device is connected, install and inspect the app on that device; confirm touch interactions there rather than treating a desktop build as sufficient.
- Report any platform or hardware validation that could not be completed.

## References

- [Dioxus 0.7 components](https://dioxuslabs.com/learn/0.7/essentials/ui/components/)
- [Dioxus 0.7 signals](https://dioxuslabs.com/learn/0.7/essentials/basics/signals/)
- [Dioxus 0.7 event handlers](https://dioxuslabs.com/learn/0.7/essentials/basics/event_handlers/)
