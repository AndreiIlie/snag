use crate::{
    cli::{Command, Destination},
    run::{RunError, run},
};
use snag_core::{
    config::Config,
    export::encode_png,
    geometry::ScaleFactor,
    image::RgbaImage,
    platform::{
        CaptureError, ClipboardBackend, ClipboardError, FakeCapture, MemoryClipboard, Screenshot,
    },
};
use std::{
    env, fs, io,
    path::PathBuf,
    process,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

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
        |d| sleeps.push(d),
    )
    .unwrap();
    assert_eq!(fs::read(&output).unwrap(), expected);
    assert!(clipboard.png().is_none());

    let command = Command::parse_from(["snag", "full", "--copy", "--delay", "11"]).unwrap();
    run(command, &config, &mut capture, &mut clipboard, |d| {
        sleeps.push(d)
    })
    .unwrap();
    assert_eq!(clipboard.png(), Some(expected.as_slice()));
    assert_eq!(
        sleeps,
        [Duration::from_millis(50), Duration::from_millis(11)]
    );
    assert_eq!(capture.calls(), 2);
}

#[test]
fn capture_failure_preserves_destinations_and_gui_does_not_capture() {
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
        |_| {},
    )
    .unwrap_err();
    assert!(error.to_string().contains("capture denied"));
    assert!(!output.exists());
    assert_eq!(clipboard.png(), Some(b"previous".as_slice()));
    assert!(matches!(
        run(
            Command::Gui,
            &Config::default(),
            &mut capture,
            &mut clipboard,
            |_| {}
        ),
        Err(RunError::OverlayUnavailable)
    ));
    assert_eq!(capture.calls(), 1);
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
