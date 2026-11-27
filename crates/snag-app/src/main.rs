mod capture;
mod cli;
mod clipboard;
mod run;
mod session;
#[cfg(test)]
mod tests;

use cli::Command;
use snag_core::config::{Config, config_path};
use std::{env, error::Error, process::ExitCode, thread};

fn execute(command: Command) -> Result<(), Box<dyn Error>> {
    if command == Command::ServeClipboard {
        return clipboard::serve::run();
    }
    let config = Config::load(&config_path()?)?;
    run::run(
        command,
        &config,
        &mut capture::Capture::from(session::Session::current()),
        &mut clipboard::launch::HelperClipboard {
            program: env::current_exe()?,
        },
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
