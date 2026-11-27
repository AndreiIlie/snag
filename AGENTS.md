# Snag

`snag` is an open-source Linux screenshot, annotation, and clipboard tool built as a fast, reliable alternative to Flameshot.

## Goals and quality bar

- **Reliability first:** Never silently fail to capture and never lose clipboard contents when the screenshot overlay exits.
- **Primary target:** Ubuntu GNOME on Wayland (`ubuntu:GNOME`), with clean support across KDE Plasma, wlroots (Sway/Hyprland), and X11.
- **Fast and lean:** Instant startup, low RAM and CPU usage, minimal dependencies. Avoid unnecessary full-frame allocations or heavy GPU/GUI toolkits.
- **Open-source hygiene:** Keep the repository completely public-safe. Never include private employer, client, or colleague names, local machine paths, or internal workflow jargon in code, comments, commit messages, or docs.

## Architecture

Organize the repository as a Rust Cargo workspace split into a pure headless core and a thin Linux platform/UI shell:

- `crates/snag-core`: Pure, display-free Rust library (no Wayland/X11/D-Bus/windowing dependencies):
  - Image representation (RGBA), HiDPI coordinate transforms, cropping, and PNG encoding/export.
  - Annotation document model: arrow, rectangle, ellipse/circle, line, freehand pen, highlighter, pixelate/blur, text, numbered counter; color and stroke state; selection, move, and delete; full undo/redo stack.
  - Deterministic CPU renderer (`tiny-skia`, plus system font loading for text annotations) driving both interactive canvas frames (backdrop dim, crop handles, annotations, floating toolbar layout and hit-testing) and final cropped PNG export.
  - Configuration schema (`$XDG_CONFIG_HOME/snag/config.toml`) and output filename templating.
  - Platform trait abstractions (`ScreenCapture`, `ClipboardBackend`) with fake/in-memory implementations for headless tests.
- `crates/snag-app` (binary `snag`): Thin Linux platform and UI shell:
  - CLI modes (`snag` / `snag gui`, `snag full`, `snag --serve-clipboard`, output/copy/delay flags).
  - Screen capture backends: XDG Desktop Portal (`ashpd` / `org.freedesktop.portal.Screenshot`) as primary Wayland backend across GNOME/KDE/wlroots, with direct X11 (and optional wlroots) fallback behind `ScreenCapture`.
  - Persistent clipboard ownership: copying launches a short-lived detached background helper (`snag --serve-clipboard`) that receives PNG bytes over a pipe/stdin, owns the clipboard (`image/png`) after the UI closes, and exits cleanly once another application claims clipboard ownership (or a newer `snag` copy replaces it).
  - Fullscreen overlay window built on `winit` + `softbuffer` (pure CPU surface presentation, zero OpenGL/GPU init), forwarding input events to `snag-core` and presenting rendered frames.
  - File save dialog via XDG Desktop Portal (`ashpd`) when saving interactively without `--output`.

## Headless wall and verification rules

- Build and test environments have **no live Wayland or X11 compositor**.
- Every core behavior (canvas geometry, crop, annotation rendering, hit-testing, toolbar layout, undo/redo, PNG export, config/CLI parsing, backend selection, and clipboard-helper IPC/lifecycle) must be verified headlessly via `cargo test --workspace`.
- Keep modules small and composable so later features layer cleanly on top.
- Avoid heavy C/C++ GUI toolkits (no GTK, Qt, OpenGL, or wgpu).

## Checks

All changes must pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
