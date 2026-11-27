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
        (&["full", "--copy"][..], "no screen capture backend"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_snag"))
            .args(args)
            .env("XDG_CONFIG_HOME", &config)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(message), "{stderr}");
    }
}
