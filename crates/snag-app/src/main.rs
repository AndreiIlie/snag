mod cli;
mod run;
#[cfg(test)]
mod tests;

use cli::Command;
use snag_core::{
    config::{Config, config_path},
    platform::{CaptureError, ClipboardBackend, ClipboardError, ScreenCapture, Screenshot},
};
use std::{env, error::Error, process::ExitCode, thread};

struct UnavailableBackend;

impl ScreenCapture for UnavailableBackend {
    fn capture(&mut self) -> Result<Screenshot, CaptureError> {
        Err(CaptureError(
            "no screen capture backend in this build".into(),
        ))
    }
}

impl ClipboardBackend for UnavailableBackend {
    fn set_png(&mut self, _: &[u8]) -> Result<(), ClipboardError> {
        Err(ClipboardError("no clipboard backend in this build".into()))
    }
}

fn execute(command: Command) -> Result<(), Box<dyn Error>> {
    let config = Config::load(&config_path()?)?;
    run::run(
        command,
        &config,
        &mut UnavailableBackend,
        &mut UnavailableBackend,
        thread::sleep,
    )?;
    Ok(())
}

fn main() -> ExitCode {
    let command = Command::parse_from(env::args_os()).unwrap_or_else(|e| e.exit());
    match execute(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("snag: {e}");
            ExitCode::FAILURE
        }
    }
}
