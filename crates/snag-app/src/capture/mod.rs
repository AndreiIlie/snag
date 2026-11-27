mod portal;
mod x11;

use crate::session::{NO_DISPLAY, Session};
use snag_core::platform::{CaptureError, ScreenCapture, Screenshot};

#[derive(Debug, PartialEq, Eq)]
pub enum Capture {
    Portal,
    X11,
    Unavailable,
}

impl From<Session> for Capture {
    fn from(value: Session) -> Self {
        match value {
            Session::Wayland { .. } => Self::Portal,
            Session::X11 => Self::X11,
            Session::None => Self::Unavailable,
        }
    }
}

impl ScreenCapture for Capture {
    fn capture(&mut self) -> Result<Screenshot, CaptureError> {
        match self {
            Self::Portal => {
                portal::capture().map_err(|e| CaptureError(format!("Screenshot portal: {e}")))
            }
            Self::X11 => x11::capture().map_err(|e| CaptureError(format!("X11: {e}"))),
            Self::Unavailable => Err(CaptureError(NO_DISPLAY.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wayland_always_uses_portal() {
        for x11 in [false, true] {
            assert_eq!(Capture::from(Session::Wayland { x11 }), Capture::Portal);
        }
        assert_eq!(Capture::from(Session::X11), Capture::X11);
        assert_eq!(Capture::from(Session::None), Capture::Unavailable);
    }
}
