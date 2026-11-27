use crate::{geometry::ScaleFactor, image::RgbaImage};
use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq)]
pub struct Screenshot {
    pub image: RgbaImage,
    pub scale: ScaleFactor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureError(pub String);

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CaptureError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardError(pub String);

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for ClipboardError {}

pub trait ScreenCapture {
    fn capture(&mut self) -> Result<Screenshot, CaptureError>;
}

pub trait ClipboardBackend {
    fn set_png(&mut self, png: &[u8]) -> Result<(), ClipboardError>;
}

pub struct FakeCapture {
    result: Result<Screenshot, CaptureError>,
    calls: usize,
}

impl FakeCapture {
    pub fn new(result: Result<Screenshot, CaptureError>) -> Self {
        Self { result, calls: 0 }
    }
    pub fn calls(&self) -> usize {
        self.calls
    }
}

impl ScreenCapture for FakeCapture {
    fn capture(&mut self) -> Result<Screenshot, CaptureError> {
        self.calls += 1;
        self.result.clone()
    }
}

#[derive(Default)]
pub struct MemoryClipboard {
    png: Option<Vec<u8>>,
    pub error: Option<ClipboardError>,
}

impl MemoryClipboard {
    pub fn png(&self) -> Option<&[u8]> {
        self.png.as_deref()
    }
}

impl ClipboardBackend for MemoryClipboard {
    fn set_png(&mut self, png: &[u8]) -> Result<(), ClipboardError> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        self.png = Some(png.to_vec());
        Ok(())
    }
}
