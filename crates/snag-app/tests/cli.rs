use std::{path::Path, process::Command};

#[test]
fn full_requires_a_destination_before_reading_config() {
    let result = Command::new(env!("CARGO_BIN_EXE_snag"))
        .arg("full")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("--output") && stderr.contains("--copy"),
        "{stderr}"
    );
}

#[test]
fn reports_unavailable_overlay_and_capture() {
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for (args, message) in [
        (&[][..], "interactive overlay"),
        (&["gui"][..], "interactive overlay"),
        (&["full", "--copy"][..], "WAYLAND_DISPLAY and DISPLAY"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_snag"))
            .args(args)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env("XDG_CONFIG_HOME", &config)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(message), "{stderr}");
    }
}

#[test]
fn clipboard_helper_reports_no_display_before_loading_config() {
    use snag_core::{export::encode_png, image::RgbaImage};
    use std::{io::Write, os::unix::process::CommandExt, process::Stdio};
    let mut child = Command::new(env!("CARGO_BIN_EXE_snag"))
        .process_group(0)
        .arg("--serve-clipboard")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let png = encode_png(&RgbaImage::new(1, 1, vec![255; 4]).unwrap(), None).unwrap();
    child.stdin.take().unwrap().write_all(&png).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let status = String::from_utf8(output.stdout).unwrap();
    assert!(
        status.starts_with("error: ") && status.contains("WAYLAND_DISPLAY"),
        "{status}"
    );
    assert_eq!(status.lines().count(), 1);
}

#[test]
fn clipboard_helper_must_run_alone() {
    for flag in ["full", "--copy", "--delay", "--help"] {
        let result = Command::new(env!("CARGO_BIN_EXE_snag"))
            .args(["--serve-clipboard", flag])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
    }
}
