use serde::Deserialize;
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
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CaptureConfig {
    pub delay_ms: u64,
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
        toml::from_str(text)
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
