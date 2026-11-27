use crate::cli::{Command, Destination};
use crate::clock::Clock;
use snag_core::{
    annotation::Document,
    config::Config,
    export::{ExportError, encode_annotated_png},
    geometry::PixelRect,
    platform::{CaptureError, ClipboardBackend, ClipboardError, ScreenCapture, Screenshot},
    toolbar::Action,
};
use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Choice {
    pub action: Action,
    pub crop: Option<PixelRect>,
    pub annotations: Document,
}

pub trait OverlayWindow {
    fn choose(self, shot: &Screenshot) -> Result<Choice, Box<dyn Error>>;
}

pub trait SaveDialog {
    fn path(self, directory: Option<&Path>, name: &str) -> Result<Option<PathBuf>, Box<dyn Error>>;
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
    clock: Clock<impl FnOnce(Duration)>,
) -> Result<(), RunError> {
    let delay = match command {
        Command::Full { delay_ms, .. } | Command::Gui { delay_ms, .. } => {
            delay_ms.unwrap_or(config.capture.delay_ms)
        }
        Command::ServeClipboard => unreachable!("helper mode is dispatched before capture"),
    };
    if delay > 0 {
        (clock.sleep)(Duration::from_millis(delay));
    }
    let shot = capture.capture().map_err(RunError::Capture)?;
    let captured_at = (clock.now)();
    let (crop, annotations, file, copy) = match command {
        Command::Full { destination, .. } => match destination {
            Destination::File { path, copy } => (None, Document::default(), Ok(Some(path)), copy),
            Destination::Clipboard => (None, Document::default(), Ok(None), true),
        },
        Command::Gui { output, copy, .. } => {
            let Choice {
                action,
                crop,
                annotations,
            } = overlay.choose(&shot).map_err(RunError::Overlay)?;
            match action {
                Action::Cancel => return Err(RunError::Cancelled),
                Action::Copy => (crop, annotations, Ok(None), true),
                Action::Save => {
                    let path = match output {
                        Some(path) => Ok(Some(path)),
                        None => match (clock.local_time)(captured_at)
                            .map_err(|e| format!("could not read local capture time: {e}"))
                            .and_then(|time| {
                                let name = config.save.filename.render(&time);
                                dialog
                                    .path(config.save.directory.as_deref(), &name)
                                    .map_err(|e| format!("could not select a file to save: {e}"))
                            }) {
                            Ok(None) => return Err(RunError::Cancelled),
                            result => result,
                        },
                    };
                    (crop, annotations, path, copy)
                }
            }
        }
        Command::ServeClipboard => unreachable!(),
    };
    let png = encode_annotated_png(&shot.image, &annotations, crop).map_err(RunError::Export)?;
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
