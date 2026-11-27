use std::{env, ffi::OsStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    Wayland { x11: bool },
    X11,
    None,
}

impl Session {
    pub fn current() -> Self {
        Self::detect(
            env::var_os("WAYLAND_DISPLAY").as_deref(),
            env::var_os("DISPLAY").as_deref(),
        )
    }

    fn detect(wayland: Option<&OsStr>, display: Option<&OsStr>) -> Self {
        let x11 = display.is_some_and(|v| !v.is_empty());
        if wayland.is_some_and(|v| !v.is_empty()) {
            Self::Wayland { x11 }
        } else if x11 {
            Self::X11
        } else {
            Self::None
        }
    }
}

pub const NO_DISPLAY: &str = "no display session (WAYLAND_DISPLAY and DISPLAY are unset or empty)";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_session_from_nonempty_display_names() {
        for (wl, x, expected) in [
            (Some("wayland-0"), None, Session::Wayland { x11: false }),
            (
                Some("wayland-0"),
                Some(":0"),
                Session::Wayland { x11: true },
            ),
            (None, Some(":0"), Session::X11),
            (Some(""), Some(":0"), Session::X11),
            (Some(""), Some(""), Session::None),
            (None, None, Session::None),
        ] {
            assert_eq!(
                Session::detect(wl.map(OsStr::new), x.map(OsStr::new)),
                expected
            );
        }
    }
}
