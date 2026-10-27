# OpenWASD agent guidance

## Project goal

OpenWASD is an open, cross-platform input customization workspace. It should
help people adapt controllers, keyboards, mice, touch surfaces, and companion
devices to the games and workflows they care about. Users should be able to
discover and connect devices, configure portable profiles and mappings, route
inputs to supported outputs, and understand each platform's capabilities and
limits. Keep the product local-first, safe to stop, accessible, and honest about
permissions and unsupported features.

The project is being built in cycles. The near-term goal is to make the core
device and transport foundations dependable, then add input inspection,
profiles, mapping, and output capabilities as complete, usable increments. Use
the [product plan](docs/product-plan.md) for product direction and the
[roadmap](docs/openwasd-roadmap.md) for phase ordering and technical context.
The roadmap is guidance; update it when implementation evidence changes the
sequence or scope.

## Rust and platform boundaries

- OpenWASD-maintained application, library, adapter, and tool logic must be
  written in Rust. Do not add project-owned Java, Kotlin, Swift, Objective-C,
  C, or C++ implementation code.
- Put OS-specific Rust integration in the crate responsible for that domain.
  For example, Bluetooth behavior belongs in `openwasd-bluetooth` or its
  Bluetooth dependency such as `btleplug`; add a focused adapter crate when a
  separate platform boundary is warranted.
- Keep `openwasd-core` platform-neutral. It owns shared product models and
  behavior, not UI, device-library, or operating-system details.
- Keep the app crate focused on UI and orchestration. It should call stable
  crate APIs instead of implementing OS-specific behavior.
- Prefer Rust bindings and established Rust dependencies. If an OS requires
  native glue, keep it inside the responsible dependency boundary; do not copy
  that source into the app or duplicate it in OpenWASD crates. Keep licensing
  and packaging requirements documented.
- Expose platform capabilities through shared Rust interfaces and report
  unsupported or unavailable capabilities explicitly rather than pretending
  every platform behaves the same.


## External dependency development

- Make framework and platform-integration changes in the dependency's checked-out
  local fork (the current local clone is `../makepad`; check out `../btleplug`
  only when btleplug source changes are required). Inspect its remotes, branch,
  and working tree first; preserve unrelated changes.
- Make changes directly in the owning repository, validate them there, and commit
  the working change in that repository. Update OpenWASD's dependency reference
  only when the committed fork revision needs to be selected.
- The tracked btleplug dependency must use the Git repository
  `https://github.com/dinosath/btleplug` on branch
  `codex/openwasd-bluetooth-features`. Never commit a local btleplug path in a
  Cargo manifest or lockfile. If btleplug source edits are required, use the
  sibling checkout `../btleplug` and switch the dependency to that path only in
  the local working tree while developing; restore the Git dependency and
  regenerate `Cargo.lock` before committing OpenWASD changes.
- Never create or use patch files (`.patch`/`.diff`) or `git apply` to carry
  dependency changes. Do not keep patch-based dependency instructions in docs.
- For this local development workspace, run Android builds with `cargo-makepad`
  from the local Makepad checkout (`../makepad/tools/cargo_makepad`). Keep the
  app UI dependency on its known-compatible Makepad branch until the local
  Makepad checkout passes the Android rendering check; do not silently swap
  framework revisions.

## Working in cycles

For each cycle:

1. Read the relevant product-plan and roadmap sections, inspect the current
   implementation, and identify the next user-visible capability or defect.
2. Define a narrow vertical slice with observable acceptance criteria. Prefer
   a small end-to-end capability over a broad scaffold or unrelated cleanup.
3. Keep domain behavior in its owning crate and preserve platform-neutral
   interfaces. Avoid speculative abstractions until a real second use requires
   them.
4. Complete the slice, validate it with the appropriate existing checks and
   available target hardware/platforms, and state any validation gaps clearly.
5. Update the relevant documentation with the delivered capability, changed
   phase/status, and known limitations so the next cycle starts from current
   facts.

Do not claim a capability is complete based only on UI, compilation, or a
nearby device advertisement when the product behavior requires a real
connection, input, or output. Preserve safe-stop behavior and ensure a stopped
profile or disconnected device cannot leave input stuck.
