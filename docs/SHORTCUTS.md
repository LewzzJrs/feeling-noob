# SHORTCUTS

Shortcuts taken in the code (tech debt). This is not the keyboard shortcut list; that is in `SPEC.md` section 6.

Each entry: file, what was skipped, when it needs fixing.

## Phase 2

| # | File | What was skipped | When to fix |
|---|---|---|---|
| S1 | `crates/fnoob-engine/src/device.rs` | Only the `f32` sample format is supported. Devices with other formats are rejected with an error. WASAPI shared mode is almost always `f32`. | When a device gets rejected. |
| S2 | `crates/fnoob-engine/src/device.rs` | Always uses the device's default config (sample rate, channels, buffer size). Buffer size cannot be chosen yet. | Phase 5, when recording latency is measured. |
| S3 | `crates/fnoob-engine/src/device.rs` | Stream errors only keep the `ErrorKind`; the detailed message is lost. No automatic reconnect; the user clicks "Reopen device" in Settings. | Phase 8, or earlier if it happens often. |
| S4 | `crates/fnoob-app/src/app.rs` | Tempo, metronome, and the 4/4 meter live in the `App` struct, not in a project model. | Phase 3 (`Project` model). |
| S5 | `crates/fnoob-app/src/app.rs` | No keymap yet; transport only works through buttons. The position display shows bar.beat only. | Phase 3. |
| S6 | `crates/fnoob-app/src/main.rs` | The window icon is still the default `eframe` icon. | Phase 8 (original icon). |
| S7 | `crates/fnoob-core/src/fsutil.rs`, `settings.rs` (tests) | Tests use folders in `%TEMP%` named by PID instead of the `tempfile` crate. | If tests start colliding when run in parallel. |
