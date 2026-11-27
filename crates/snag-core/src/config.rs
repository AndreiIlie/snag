use crate::filename::FilenameTemplate;
use serde::{Deserialize, de::Error as _};
use std::{
    env,
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub capture: CaptureConfig,
    pub save: SaveConfig,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CaptureConfig {
    pub delay_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SaveConfig {
    pub directory: Option<PathBuf>,
    pub filename: FilenameTemplate,
}

#[derive(Debug)]
pub enum ConfigError {
    NoConfigDirectory,
    Read {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDirectory => {
                f.write_str("could not locate the config directory from XDG_CONFIG_HOME or HOME")
            }
            Self::Read { path, source } => write!(f, "could not read {}: {source}", path.display()),
            Self::Parse { path, source } => {
                write!(f, "invalid config in {}: {source}", path.display())
            }
        }
    }
}
impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NoConfigDirectory => None,
            Self::Read { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    resolve_path(
        env::var_os("XDG_CONFIG_HOME").as_deref().map(Path::new),
        env::var_os("HOME").as_deref().map(Path::new),
    )
}

fn resolve_path(xdg: Option<&Path>, home: Option<&Path>) -> Result<PathBuf, ConfigError> {
    let base = xdg
        .filter(|p| p.is_absolute())
        .map(Path::to_owned)
        .or_else(|| home.filter(|p| p.is_absolute()).map(|p| p.join(".config")))
        .ok_or(ConfigError::NoConfigDirectory)?;
    Ok(base.join("snag/config.toml"))
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        Self::parse_with_home(text, env::var_os("HOME").as_deref().map(Path::new))
    }

    fn parse_with_home(text: &str, home: Option<&Path>) -> Result<Self, toml::de::Error> {
        let mut config: Self = toml::from_str(text)?;
        if let Some(dir) = &mut config.save.directory {
            if dir.as_os_str().as_encoded_bytes().starts_with(b"~/") {
                let home = home.filter(|path| path.is_absolute()).ok_or_else(|| {
                    toml::de::Error::custom(
                        "save.directory starts with ~/ but HOME is not absolute",
                    )
                })?;
                *dir = home.join(dir.strip_prefix("~").expect("path starts with ~/"));
            } else if !dir.is_absolute() {
                return Err(toml::de::Error::custom(
                    "save.directory must be absolute or start with ~/",
                ));
            }
        }
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_save_presets_when_parsing_config() {
        let home = Some(Path::new("/user"));
        assert_eq!(Config::parse("[save]").unwrap().save, SaveConfig::default());
        for (dir, expected) in [
            ("~/Pictures", "/user/Pictures"),
            ("~/", "/user"),
            ("/shots", "/shots"),
        ] {
            let text = format!("[save]\ndirectory = '{dir}'");
            assert_eq!(
                Config::parse_with_home(&text, home)
                    .unwrap()
                    .save
                    .directory
                    .as_deref(),
                Some(Path::new(expected))
            );
        }
        for (dir, home) in [
            ("", home),
            ("relative", home),
            ("~", home),
            ("~/shots", None),
            ("~/shots", Some(Path::new("relative"))),
        ] {
            assert!(
                Config::parse_with_home(&format!("[save]\ndirectory = '{dir}'"), home).is_err()
            );
        }
        assert!(Config::parse("[save]\nfilename = '%x'").is_err());
        assert!(Config::parse("[save]\nother = 1").is_err());
    }

    #[test]
    fn resolves_only_absolute_config_directories() {
        let home = Some(Path::new("/user"));
        assert_eq!(
            resolve_path(Some(Path::new("/xdg")), home).unwrap(),
            Path::new("/xdg/snag/config.toml")
        );
        for xdg in [None, Some(Path::new("")), Some(Path::new("relative"))] {
            assert_eq!(
                resolve_path(xdg, home).unwrap(),
                Path::new("/user/.config/snag/config.toml")
            );
        }
        assert!(resolve_path(None, None).is_err());
    }
}
