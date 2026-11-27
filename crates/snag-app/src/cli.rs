use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "snag", about = "Capture and annotate screenshots")]
struct Args {
    #[command(subcommand)]
    mode: Option<Mode>,

    /// Write the captured PNG to this file.
    #[arg(long, global = true, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Copy the captured PNG to the clipboard.
    #[arg(long, global = true)]
    copy: bool,

    /// Wait this many milliseconds before capturing.
    #[arg(long, global = true, value_name = "MS")]
    delay: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum Mode {
    /// Open the interactive overlay.
    Gui,
    /// Capture the full screen.
    Full,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Gui,
    Full {
        destination: Destination,
        delay_ms: Option<u64>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Destination {
    File { path: PathBuf, copy: bool },
    Clipboard,
}

impl Command {
    pub fn parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let args = Args::try_parse_from(args)?;
        if !matches!(args.mode, Some(Mode::Full)) {
            return Ok(Self::Gui);
        }
        let destination = match (args.output, args.copy) {
            (Some(path), copy) => Destination::File { path, copy },
            (None, true) => Destination::Clipboard,
            (None, false) => {
                let mut cli = Args::command();
                cli.build();
                return Err(cli
                    .find_subcommand_mut("full")
                    .expect("full subcommand")
                    .error(
                        ErrorKind::MissingRequiredArgument,
                        "full requires --output <PATH> or --copy",
                    ));
            }
        };
        Ok(Self::Full {
            destination,
            delay_ms: args.delay,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes_and_flags() {
        assert_eq!(Command::parse_from(["snag"]).unwrap(), Command::Gui);
        assert_eq!(
            Command::parse_from(["snag", "gui", "--output", "shot", "--delay", "2", "--copy"])
                .unwrap(),
            Command::Gui
        );
        let cmd =
            Command::parse_from(["snag", "--copy", "full", "--output", "shot", "--delay", "0"])
                .unwrap();
        assert_eq!(
            cmd,
            Command::Full {
                destination: Destination::File {
                    path: "shot".into(),
                    copy: true
                },
                delay_ms: Some(0)
            }
        );
    }

    #[test]
    fn invalid_arguments_exit_two() {
        for args in [
            &["snag", "full"][..],
            &["snag", "--delay", "soon"],
            &["snag", "full", "--delay", "-1", "--copy"],
        ] {
            assert_eq!(Command::parse_from(args).unwrap_err().exit_code(), 2);
        }
    }
}
