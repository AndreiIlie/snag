use snag_core::platform::{ClipboardBackend, ClipboardError};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

pub struct HelperClipboard {
    pub program: PathBuf,
}

impl ClipboardBackend for HelperClipboard {
    fn set_png(&mut self, png: &[u8]) -> Result<(), ClipboardError> {
        self.launch(png).map_err(|e| ClipboardError(e.to_string()))
    }
}

impl HelperClipboard {
    fn launch(&self, png: &[u8]) -> super::Result<()> {
        let mut child = Command::new(&self.program)
            .arg("--serve-clipboard")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let sent = child.stdin.take().expect("piped stdin").write_all(png);
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("piped stdout")).read_line(&mut line)?;
        match line.strip_suffix('\n') {
            Some("ready") => Ok(sent?),
            Some(status) if status.starts_with("error: ") => Err(status[7..].into()),
            _ if line.is_empty() => {
                let reason = match child.try_wait()? {
                    Some(status) => {
                        format!("helper exited with {status} before reporting ownership")
                    }
                    None => "helper closed its status pipe before reporting ownership".into(),
                };
                Err(reason.into())
            }
            _ => Err("invalid clipboard helper status".into()),
        }
    }
}
