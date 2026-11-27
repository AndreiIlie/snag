use crate::{
    cli::{Command, Destination},
    run::{Choice, OverlayWindow, RunError, SaveDialog, run},
};
use snag_core::{
    config::Config,
    export::{decode_png, encode_png},
    geometry::{PixelRect, ScaleFactor},
    image::RgbaImage,
    platform::{
        CaptureError, ClipboardBackend, ClipboardError, FakeCapture, MemoryClipboard, Screenshot,
    },
    toolbar::Action,
};
use std::{
    env, fs, io,
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

struct Unused;
impl OverlayWindow for Unused {
    fn choose(self, _: &Screenshot) -> Result<Choice, Box<dyn std::error::Error>> {
        panic!("unexpected overlay")
    }
}
impl SaveDialog for Unused {
    fn path(self) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        panic!("unexpected save dialog")
    }
}
impl OverlayWindow for Choice {
    fn choose(self, _: &Screenshot) -> Result<Choice, Box<dyn std::error::Error>> {
        Ok(self)
    }
}
impl SaveDialog for Option<PathBuf> {
    fn path(self) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        Ok(self)
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!("snag-test-{}-{id}", process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("could not create test directory: {e}"),
            }
        }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn screenshot() -> Screenshot {
    Screenshot {
        image: RgbaImage::new(2, 1, vec![255, 0, 0, 128, 8, 9, 10, 255]).unwrap(),
        scale: ScaleFactor::ONE,
    }
}

fn file_command(path: PathBuf, copy: bool) -> Command {
    Command::Full {
        destination: Destination::File { path, copy },
        delay_ms: None,
    }
}

#[test]
fn loads_config_with_defaults_and_reports_file_errors_by_path() {
    let dir = TempDir::new();
    let path = dir.0.join("config.toml");
    assert_eq!(Config::load(&path).unwrap().capture.delay_ms, 0);
    for (text, delay) in [
        ("", 0),
        ("[capture]\n", 0),
        ("[capture]\ndelay_ms = 25", 25),
    ] {
        fs::write(&path, text).unwrap();
        assert_eq!(Config::load(&path).unwrap().capture.delay_ms, delay);
    }
    for text in ["[capture", "other = 1", "[capture]\ndelay = 1"] {
        fs::write(&path, text).unwrap();
        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains(path.to_str().unwrap()), "{error}");
    }
    assert!(
        Config::load(&dir.0)
            .unwrap_err()
            .to_string()
            .contains(dir.0.to_str().unwrap())
    );
}

#[test]
fn saves_and_copies_the_same_png_and_honors_delay_precedence() {
    let dir = TempDir::new();
    let output = dir.0.join("capture.png");
    fs::write(&output, vec![0; 1000]).unwrap();
    let shot = screenshot();
    let expected = encode_png(&shot.image, None).unwrap();
    let mut capture = FakeCapture::new(Ok(shot));
    let mut clipboard = MemoryClipboard::default();
    let config = Config::parse("[capture]\ndelay_ms = 50").unwrap();
    let mut sleeps = Vec::new();
    run(
        file_command(output.clone(), false),
        &config,
        &mut capture,
        &mut clipboard,
        Unused,
        Unused,
        |d| sleeps.push(d),
    )
    .unwrap();
    assert_eq!(fs::read(&output).unwrap(), expected);
    assert!(clipboard.png().is_none());

    let command = Command::parse_from(["snag", "full", "--copy", "--delay", "11"]).unwrap();
    run(
        command,
        &config,
        &mut capture,
        &mut clipboard,
        Unused,
        Unused,
        |d| sleeps.push(d),
    )
    .unwrap();
    assert_eq!(clipboard.png(), Some(expected.as_slice()));
    assert_eq!(
        sleeps,
        [Duration::from_millis(50), Duration::from_millis(11)]
    );
    assert_eq!(capture.calls(), 2);
}

#[test]
fn capture_failure_preserves_destinations_and_skips_the_overlay() {
    let dir = TempDir::new();
    let output = dir.0.join("capture.png");
    let mut capture = FakeCapture::new(Err(CaptureError("capture denied".into())));
    let mut clipboard = MemoryClipboard::default();
    clipboard.set_png(b"previous").unwrap();
    let error = run(
        file_command(output.clone(), true),
        &Config::default(),
        &mut capture,
        &mut clipboard,
        Unused,
        Unused,
        |_| {},
    )
    .unwrap_err();
    assert!(error.to_string().contains("capture denied"));
    assert!(!output.exists());
    assert_eq!(clipboard.png(), Some(b"previous".as_slice()));
    assert!(matches!(
        run(
            Command::parse_from(["snag"]).unwrap(),
            &Config::default(),
            &mut capture,
            &mut clipboard,
            Unused,
            Unused,
            |_| {}
        ),
        Err(RunError::Capture(_))
    ));
    assert_eq!(capture.calls(), 2);
}

#[test]
fn attempts_both_destinations_and_reports_each_failure() {
    let dir = TempDir::new();
    let missing = dir.0.join("missing/shot.png");
    let output = dir.0.join("shot.png");
    let shot = screenshot();
    let expected = encode_png(&shot.image, None).unwrap();
    let mut capture = FakeCapture::new(Ok(shot));
    let mut clipboard = MemoryClipboard::default();
    let mut save_and_copy = |path, clipboard: &mut MemoryClipboard| {
        run(
            file_command(path, true),
            &Config::default(),
            &mut capture,
            clipboard,
            Unused,
            Unused,
            |_| {},
        )
        .unwrap_err()
        .to_string()
    };
    let error = save_and_copy(missing.clone(), &mut clipboard);
    assert!(error.contains(missing.to_str().unwrap()));
    assert_eq!(clipboard.png(), Some(expected.as_slice()));
    clipboard.error = Some(ClipboardError("clipboard unavailable".into()));
    assert!(save_and_copy(output.clone(), &mut clipboard).contains("clipboard unavailable"));
    assert_eq!(fs::read(output).unwrap(), expected);
    let error = save_and_copy(missing.clone(), &mut clipboard);
    assert!(error.contains(missing.to_str().unwrap()) && error.contains("clipboard unavailable"));
}

#[test]
fn gui_copy_encodes_the_selected_capture_pixels_and_honors_delay() {
    let shot = screenshot();
    let mut capture = FakeCapture::new(Ok(shot.clone()));
    let mut clipboard = MemoryClipboard::default();
    let config = Config::parse("[capture]\ndelay_ms = 51").unwrap();
    for (crop, delay_ms, wait) in [(PixelRect::new(0, 0, 1, 1), Some(12), 12), (None, None, 51)] {
        let mut slept = None;
        run(
            Command::Gui {
                output: None,
                copy: false,
                delay_ms,
            },
            &config,
            &mut capture,
            &mut clipboard,
            (Action::Copy, crop),
            Unused,
            |d| slept = Some(d),
        )
        .unwrap();
        assert_eq!(slept, Some(Duration::from_millis(wait)));
        let image = decode_png(clipboard.png().unwrap()).unwrap();
        assert_eq!(
            image,
            crop.map_or_else(|| shot.image.clone(), |r| shot.image.crop(r).unwrap())
        );
    }
}

#[test]
fn gui_save_uses_output_or_the_selected_path_and_can_also_copy() {
    let dir = TempDir::new();
    let file = dir.0.join("as typed");
    let shot = screenshot();
    let crop = PixelRect::new(1, 0, 2, 1);
    let png = encode_png(&shot.image, crop).unwrap();
    let mut capture = FakeCapture::new(Ok(shot));
    let mut clipboard = MemoryClipboard::default();
    let gui = |output, copy| Command::Gui {
        output,
        copy,
        delay_ms: None,
    };
    let save = (Action::Save, crop);
    run(
        gui(None, false),
        &Config::default(),
        &mut capture,
        &mut clipboard,
        save,
        Some(file.clone()),
        |_| {},
    )
    .unwrap();
    assert_eq!(fs::read(&file).unwrap(), png);
    assert!(clipboard.png().is_none());
    fs::remove_file(&file).unwrap();
    run(
        gui(Some(file.clone()), true),
        &Config::default(),
        &mut capture,
        &mut clipboard,
        save,
        Unused,
        |_| {},
    )
    .unwrap();
    assert_eq!(fs::read(&file).unwrap(), png);
    assert_eq!(clipboard.png(), Some(png.as_slice()));
    clipboard.error = Some(ClipboardError("lost clipboard".into()));
    let missing = dir.0.join("missing/shot");
    let error = run(
        gui(Some(missing.clone()), true),
        &Config::default(),
        &mut capture,
        &mut clipboard,
        save,
        Unused,
        |_| {},
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains(missing.to_str().unwrap()) && error.contains("lost clipboard"));
}

#[test]
fn cancelling_the_overlay_or_dialog_preserves_destinations() {
    let dir = TempDir::new();
    let path = dir.0.join("existing.png");
    fs::write(&path, b"original").unwrap();
    let mut capture = FakeCapture::new(Ok(screenshot()));
    let mut clipboard = MemoryClipboard::default();
    clipboard.set_png(b"previous").unwrap();
    for (action, output) in [(Action::Cancel, Some(path.clone())), (Action::Save, None)] {
        let error = run(
            Command::Gui {
                output,
                copy: true,
                delay_ms: None,
            },
            &Config::default(),
            &mut capture,
            &mut clipboard,
            (action, None),
            None,
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(error, RunError::Cancelled));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(clipboard.png(), Some(&b"previous"[..]));
    }
}

#[test]
fn launcher_delivers_full_payload_and_returns_while_helper_is_alive() {
    use crate::clipboard::launch::HelperClipboard;
    use rustix::process::{Pid, Signal, WaitOptions, kill_process, waitpid};
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new();
    let program = dir.0.join("helper");
    fs::write(
        &program,
        r#"#!/bin/sh
test "$1" = --serve-clipboard || exit 2
cat > "$0.png" || exit 1
echo $$ > "$0.pid"
echo ready
exec sleep 10
"#,
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    let mut clipboard = HelperClipboard {
        program: program.clone(),
    };
    let payload: Vec<u8> = (0..2_000_000).map(|i| (i % 251) as u8).collect();
    clipboard.set_png(&payload).unwrap();
    let pid = fs::read_to_string(program.with_extension("pid")).unwrap();
    let pid = Pid::from_raw(pid.trim().parse().unwrap()).unwrap();
    assert!(waitpid(Some(pid), WaitOptions::NOHANG).unwrap().is_none());
    kill_process(pid, Signal::TERM).unwrap();
    waitpid(Some(pid), WaitOptions::empty()).unwrap();
    assert_eq!(fs::read(program.with_extension("png")).unwrap(), payload);
}

#[test]
fn launcher_reports_helper_errors_and_silent_exit() {
    use crate::clipboard::launch::HelperClipboard;
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let mut helper = HelperClipboard {
        program: dir.0.join("helper"),
    };
    for (script, expected) in [
        (
            "echo 'error: selection unavailable'",
            "selection unavailable",
        ),
        ("exit 1", "before reporting ownership"),
    ] {
        fs::write(
            &helper.program,
            format!("#!/bin/sh\ncat >/dev/null\n{script}\n"),
        )
        .unwrap();
        fs::set_permissions(&helper.program, fs::Permissions::from_mode(0o700)).unwrap();
        let error = helper.set_png(b"png").unwrap_err();
        assert!(error.0.contains(expected), "{error}");
    }
}
