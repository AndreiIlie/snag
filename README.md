# Snag

Snag captures your Linux desktop and lets you crop and annotate the image before saving or copying a PNG. A background helper keeps the image available to paste after the overlay closes.

## Installation

Build from a checkout with a current stable Rust toolchain (including Cargo) and a C linker.

```sh
git clone https://github.com/AndreiIlie/snag.git
cd snag
```

### Debian or Ubuntu package

Install Git, a C toolchain and `dpkg-dev` on the build machine. With Rust installed, run the following from the checkout.

```sh
sudo apt install build-essential dpkg-dev
deb=$(packaging/build-deb.sh) && sudo apt install "$deb"
```

The script prints the exact package path under `target/deb/` (or `$CARGO_TARGET_DIR/deb/`). The package installs the binary, Snag launcher, icon and copyright notices.

Two builds from the same commit produce byte-identical packages with the same architecture, toolchain, system libraries, packaging tools and `SOURCE_DATE_EPOCH`, even when the checkout and target directories differ. By default the script uses the commit timestamp. Build on the oldest release you intend to support because the package records its required glibc version. For example, a package requiring glibc 2.43 from Ubuntu 26.04 cannot run on Ubuntu 24.04 with glibc 2.39.

### Single binary

```sh
cargo build --release --locked
install -Dm755 target/release/snag ~/.local/bin/snag
```

Include `~/.local/bin` in your desktop session's `PATH` so launchers and shortcuts can find `snag`. To add the launcher and icon for your account, run these commands.

```sh
install -Dm644 packaging/snag.desktop "${XDG_DATA_HOME:-$HOME/.local/share}/applications/snag.desktop"
install -Dm644 packaging/snag.svg "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps/snag.svg"
```

### Desktop requirements

Wayland capture uses the Screenshot portal. Install `xdg-desktop-portal` and the backend for your session.

| Desktop | Portal packages |
| --- | --- |
| GNOME | `xdg-desktop-portal-gnome` |
| KDE Plasma | `xdg-desktop-portal-kde` |
| Sway | `xdg-desktop-portal-wlr` and `grim` for capture, `xdg-desktop-portal-gtk` for the save dialog |
| Hyprland | `xdg-desktop-portal-hyprland` and `grim` for capture, `xdg-desktop-portal-gtk` for the save dialog |

X11 captures directly. The interactive save dialog uses the FileChooser portal on both X11 and Wayland.

## Usage

| Command | Result |
| --- | --- |
| `snag` or `snag gui` | Open the annotation overlay. |
| `snag --delay 1500` | Wait 1.5 seconds before capturing and opening the overlay. |
| `snag gui --output shot.png --copy` | Save writes to the given path and also copies the PNG. |
| `snag full --copy` | Copy the full screen without opening the overlay. |
| `snag full --output shot.png` | Write the full screen to a PNG. Add `--copy` to copy it too. |

Drag to select the area to keep. Use **Enter** or **Ctrl+C** to copy, **Ctrl+S** to save and **Esc** to cancel. While editing a text annotation, Enter finishes editing first. Save opens a file chooser unless you supplied `--output`. Full-screen capture requires `--output` or `--copy`.

## Configuration

Create `$XDG_CONFIG_HOME/snag/config.toml`, or `~/.config/snag/config.toml` when using the default config directory. All settings are optional.

```toml
[capture]
delay_ms = 0

[save]
directory = "~/Pictures/Screenshots"
filename = "screenshot-%Y%m%d-%H%M%S"
```

`--delay` overrides `delay_ms`. The save directory presets the chooser's folder. Use an absolute path or one beginning with `~/`, and create the folder beforehand. Omit it to let the chooser pick its folder.

The filename template presets the suggested name using local time at capture. The default shown above produces names like `screenshot-20260709-140503.png`. Snag appends `.png`. Use `%Y` for year, `%m` for month, `%d` for day, `%H` for 24-hour hour, `%M` for minutes, `%S` for seconds and `%%` for a literal percent sign.

## Global Print shortcut

### GNOME

Open **Settings → Keyboard → View and Customize Shortcuts → Custom Shortcuts**. Add a shortcut named **Snag** with command `snag`. Choose **Set Shortcut**, press **Print**, confirm reassignment from the built-in screenshot tool if prompted, then choose **Add**. [GNOME shortcut guide](https://help.gnome.org/gnome-help/keyboard-shortcuts-set.html)

If GNOME denies a capture from the shortcut, check that the Snag desktop entry is installed. On systems where the portal identifies the app as `snag`, you can grant that app screenshot access for your account and retry. Run this in your desktop session if you want that persistent permission. [Portal permission store](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.PermissionStore.html)

```sh
dbus-send --session --print-reply --dest=org.freedesktop.impl.portal.PermissionStore \
  /org/freedesktop/impl/portal/PermissionStore org.freedesktop.impl.portal.PermissionStore.SetPermission \
  string:screenshot boolean:true string:screenshot string:snag array:string:yes
```

### KDE Plasma

In Plasma 6 open **System Settings → Keyboard → Shortcuts**. Find Spectacle and clear its **Print** shortcut. Choose **Add New → Application** and select Snag, or choose **Command or Script** and enter `snag`. Assign **Print** and apply. In Plasma 5.27, open **System Settings → Shortcuts** and use **Add Application** or **Add Command**.

### Sway

Add this binding to `~/.config/sway/config`, replacing any existing Print binding, then run `swaymsg reload`.

```text
bindsym Print exec snag
```

### Hyprland

In a Lua configuration (Hyprland 0.55 and later), bind Print to Snag as follows. [Hyprland bindings](https://wiki.hypr.land/0.55.0/configuring/basics/binds/)

```lua
hl.bind("Print", hl.dsp.exec_cmd("snag"))
```

For a hyprlang configuration, use this line in `hyprland.conf`.

```ini
bind = , Print, exec, snag
```

Snag is released under the [MIT license](LICENSE).
