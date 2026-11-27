use crate::cli::{Command, Destination};
use snag_core::{
    config::Config,
    export::{ExportError, encode_png},
    geometry::PixelRect,
    platform::{CaptureError, ClipboardBackend, ClipboardError, ScreenCapture, Screenshot},
    toolbar::Action,
};
use std::{error::Error, fmt, fs, path::PathBuf, time::Duration};

pub type Choice = (Action, Option<PixelRect>);

pub trait OverlayWindow {
    fn choose(self, shot: &Screenshot) -> Result<Choice, Box<dyn Error>>;
}

pub trait SaveDialog {
    fn path(self) -> Result<Option<PathBuf>, Box<dyn Error>>;
}

#[derive(Debug)]
pub enum RunError {
    Cancelled,
    Overlay(Box<dyn Error>),
    Capture(CaptureError),
    Export(ExportError),
    Destinations {
        save: Option<String>,
        clipboard: Option<ClipboardError>,
    },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Overlay(e) => write!(f, "could not display the overlay: {e}"),
            Self::Capture(e) => write!(f, "screen capture failed: {e}"),
            Self::Export(e) => e.fmt(f),
            Self::Destinations { save, clipboard } => {
                if let Some(error) = save {
                    f.write_str(error)?;
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
    overlay: impl OverlayWindow,
    dialog: impl SaveDialog,
    sleep: impl FnOnce(Duration),
) -> Result<(), RunError> {
    let delay = match command {
        Command::Full { delay_ms, .. } | Command::Gui { delay_ms, .. } => {
            delay_ms.unwrap_or(config.capture.delay_ms)
        }
        Command::ServeClipboard => unreachable!("helper mode is dispatched before capture"),
    };
    if delay > 0 {
        sleep(Duration::from_millis(delay));
    }
    let shot = capture.capture().map_err(RunError::Capture)?;
    let (crop, file, copy) = match command {
        Command::Full { destination, .. } => match destination {
            Destination::File { path, copy } => (None, Ok(Some(path)), copy),
            Destination::Clipboard => (None, Ok(None), true),
        },
        Command::Gui { output, copy, .. } => {
            let (action, crop) = overlay.choose(&shot).map_err(RunError::Overlay)?;
            match action {
                Action::Cancel => return Err(RunError::Cancelled),
                Action::Copy => (crop, Ok(None), true),
                Action::Save => {
                    let path = match output {
                        Some(path) => Ok(Some(path)),
                        None => match dialog.path() {
                            Ok(None) => return Err(RunError::Cancelled),
                            result => {
                                result.map_err(|e| format!("could not select a file to save: {e}"))
                            }
                        },
                    };
                    (crop, path, copy)
                }
            }
        }
        Command::ServeClipboard => unreachable!(),
    };
    let png = encode_png(&shot.image, crop).map_err(RunError::Export)?;
    deliver(&png, file, copy, clipboard)
}

fn deliver(
    png: &[u8],
    file: Result<Option<PathBuf>, String>,
    copy: bool,
    clipboard: &mut impl ClipboardBackend,
) -> Result<(), RunError> {
    let save = file
        .and_then(|path| {
            if let Some(path) = path {
                fs::write(&path, png)
                    .map_err(|e| format!("could not save {}: {e}", path.display()))?;
            }
            Ok(())
        })
        .err();
    let clipboard = if copy {
        clipboard.set_png(png).err()
    } else {
        None
    };
    match (save, clipboard) {
        (None, None) => Ok(()),
        (save, clipboard) => Err(RunError::Destinations { save, clipboard }),
    }
}
