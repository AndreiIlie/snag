use super::{Owner, Png, Result, wayland, x11};
use crate::session::{NO_DISPLAY, Session};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::fd::AsFd,
    process::{Child, Command},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Wayland,
    X11,
}

fn candidates(session: Session) -> &'static [Backend] {
    match session {
        Session::Wayland { x11: true } => &[Backend::Wayland, Backend::X11],
        Session::Wayland { x11: false } => &[Backend::Wayland],
        Session::X11 => &[Backend::X11],
        Session::None => &[],
    }
}

fn detach(status: &File) -> Result<Option<Child>> {
    rustix::stdio::dup2_stdout(File::options().write(true).open("/dev/null")?)?;
    if rustix::process::getpgrp() == rustix::process::getpid() {
        // Interactive shells can launch the helper as a process group leader.
        // Only a non-leader can start a new session.
        let child = Command::new(std::env::current_exe()?)
            .arg("--serve-clipboard")
            .stdout(status.try_clone()?)
            .spawn()?;
        return Ok(Some(child));
    }
    rustix::process::setsid()?;
    Ok(None)
}

pub fn run() -> Result<()> {
    // Keep only the status writer connected to the parent. Dropping it signals EOF.
    let mut status = File::from(io::stdout().as_fd().try_clone_to_owned()?);
    match detach(&status) {
        Ok(Some(mut child)) => {
            drop(status);
            let exit = child.wait()?;
            return if exit.success() {
                Ok(())
            } else {
                Err(format!("clipboard helper exited with {exit}").into())
            };
        }
        Ok(None) => (),
        Err(e) => {
            writeln!(status, "error: {e}")?;
            return Err(e);
        }
    }
    serve(
        io::stdin().lock(),
        status,
        Session::current(),
        |backend, png| match backend {
            Backend::Wayland => wayland::claim(png),
            Backend::X11 => x11::claim(png),
        },
    )
}

fn serve(
    mut input: impl Read,
    mut status: impl Write,
    session: Session,
    mut claim: impl FnMut(Backend, Png) -> Result<Box<dyn Owner>>,
) -> Result<()> {
    let result = (|| -> Result<Box<dyn Owner>> {
        let mut bytes = Vec::new();
        input.read_to_end(&mut bytes)?;
        if bytes.is_empty() {
            return Err("empty PNG input".into());
        }
        let png = Png::new(bytes);
        let mut failures = Vec::new();
        for &backend in candidates(session) {
            match claim(backend, png.clone()) {
                Ok(owner) => return Ok(owner),
                Err(e) => failures.push(format!("{backend:?}: {e}")),
            }
        }
        if failures.is_empty() {
            return Err(NO_DISPLAY.into());
        }
        Err(failures.join("; ").into())
    })();
    let owner = match result {
        Ok(owner) => owner,
        Err(e) => {
            writeln!(
                status,
                "error: {}",
                e.to_string().replace(['\r', '\n'], " ")
            )?;
            status.flush()?;
            return Err(e);
        }
    };
    writeln!(status, "ready")?;
    status.flush()?;
    drop(status);
    owner.wait_until_lost()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixStream, sync::mpsc, thread};

    struct FakeOwner(mpsc::Receiver<()>);

    impl Owner for FakeOwner {
        fn wait_until_lost(self: Box<Self>) -> Result<()> {
            self.0.recv()?;
            Ok(())
        }
    }

    #[test]
    fn session_selects_clipboard_candidates_in_order() {
        for (session, expected) in [
            (
                Session::Wayland { x11: true },
                &[Backend::Wayland, Backend::X11][..],
            ),
            (Session::Wayland { x11: false }, &[Backend::Wayland]),
            (Session::X11, &[Backend::X11]),
            (Session::None, &[]),
        ] {
            assert_eq!(candidates(session), expected);
        }
    }

    #[test]
    fn confirms_claim_before_ready_and_exits_on_ownership_loss() {
        let (mut reader, status) = UnixStream::pair().unwrap();
        let (attempted, attempting) = mpsc::channel();
        let (confirm, confirmed) = mpsc::channel();
        let (replace, lost) = mpsc::channel();
        let mut lost = Some(lost);
        let helper = thread::spawn(move || {
            serve(
                &b"png"[..],
                status,
                Session::Wayland { x11: true },
                |backend, png| {
                    attempted.send(backend).unwrap();
                    assert_eq!(png.as_slice(), b"png");
                    if backend == Backend::Wayland {
                        return Err("unavailable".into());
                    }
                    confirmed.recv().unwrap();
                    Ok(Box::new(FakeOwner(lost.take().unwrap())))
                },
            )
            .unwrap();
        });
        assert_eq!(attempting.recv().unwrap(), Backend::Wayland);
        assert_eq!(attempting.recv().unwrap(), Backend::X11);
        reader.set_nonblocking(true).unwrap();
        assert_eq!(
            reader.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        reader.set_nonblocking(false).unwrap();
        confirm.send(()).unwrap();
        let mut line = String::new();
        reader.read_to_string(&mut line).unwrap();
        assert_eq!(line, "ready\n");
        assert!(!helper.is_finished());
        replace.send(()).unwrap();
        helper.join().unwrap();
    }

    #[test]
    fn reports_claim_errors_and_refuses_empty_input() {
        let mut status = Vec::new();
        let session = Session::Wayland { x11: true };
        assert!(
            serve(&b"png"[..], &mut status, session, |backend, _| {
                Err(match backend {
                    Backend::Wayland => "missing protocol",
                    Backend::X11 => "unreachable",
                }
                .into())
            })
            .is_err()
        );
        assert_eq!(
            status,
            b"error: Wayland: missing protocol; X11: unreachable\n"
        );
        status.clear();
        assert!(serve(&[][..], &mut status, session, |_, _| panic!("empty claim")).is_err());
        assert_eq!(status, b"error: empty PNG input\n");
    }
}
