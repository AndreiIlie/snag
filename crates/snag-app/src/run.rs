use crate::cli::{Command, Destination};
use snag_core::{
    config::Config,
    export::{ExportError, encode_png},
    platform::{CaptureError, ClipboardBackend, ClipboardError, ScreenCapture},
};
use std::{error::Error, fmt, fs, io, path::PathBuf, time::Duration};

#[derive(Debug)]
pub enum RunError {
    OverlayUnavailable,
    Capture(CaptureError),
    Export(ExportError),
    Destinations {
        file: Option<(PathBuf, io::Error)>,
        clipboard: Option<ClipboardError>,
    },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OverlayUnavailable => {
                f.write_str("the interactive overlay is not available in this build")
            }
            Self::Capture(e) => write!(f, "screen capture failed: {e}"),
            Self::Export(e) => e.fmt(f),
            Self::Destinations { file, clipboard } => {
                if let Some((path, e)) = file {
                    write!(f, "could not save {}: {e}", path.display())?;
                    if clipboard.is_some() {
                        writeln!(f)?;
                    }
                }
                if let Some(e) = clipboard {
                    write!(f, "could not copy PNG to clipboard: {e}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for RunError {}

pub fn run(
    command: Command,
    config: &Config,
    capture: &mut impl ScreenCapture,
    clipboard: &mut impl ClipboardBackend,
    sleep: impl FnOnce(Duration),
) -> Result<(), RunError> {
    let Command::Full {
        destination,
        delay_ms,
    } = command
    else {
        return Err(RunError::OverlayUnavailable);
    };
    let delay = delay_ms.unwrap_or(config.capture.delay_ms);
    if delay > 0 {
        sleep(Duration::from_millis(delay));
    }
    let shot = capture.capture().map_err(RunError::Capture)?;
    let png = encode_png(&shot.image, None).map_err(RunError::Export)?;
    let (file_error, copy) = match destination {
        Destination::File { path, copy } => (fs::write(&path, &png).err().map(|e| (path, e)), copy),
        Destination::Clipboard => (None, true),
    };
    let clipboard_error = if copy {
        clipboard.set_png(&png).err()
    } else {
        None
    };
    match (file_error, clipboard_error) {
        (None, None) => Ok(()),
        (file, clipboard) => Err(RunError::Destinations { file, clipboard }),
    }
}
