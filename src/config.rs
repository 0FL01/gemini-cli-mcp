use std::{
    collections::BTreeSet,
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::error::AppError;

const DEFAULT_TIMEOUT_SECS: u64 = 600;
const DEFAULT_MAX_TIMEOUT_SECS: u64 = 1800;

#[derive(Debug, Clone)]
pub struct AppConfig {
    gemini_bin: String,
    default_timeout_secs: u64,
    max_timeout_secs: u64,
    allowed_models: Option<BTreeSet<String>>,
    allowed_cwd_prefixes: Option<Vec<PathBuf>>,
    working_dir: PathBuf,
}

impl AppConfig {
    pub fn new(
        gemini_bin: String,
        default_timeout_secs: u64,
        max_timeout_secs: u64,
        allowed_models: Option<BTreeSet<String>>,
        allowed_cwd_prefixes: Option<Vec<PathBuf>>,
        working_dir: PathBuf,
    ) -> Self {
        Self {
            gemini_bin,
            default_timeout_secs,
            max_timeout_secs,
            allowed_models,
            allowed_cwd_prefixes,
            working_dir,
        }
    }

    pub fn from_env() -> Result<Self, AppError> {
        let gemini_bin = env::var("GEMINI_BIN").unwrap_or_else(|_| "gemini".to_string());
        let default_timeout_secs =
            parse_u64_env("GEMINI_DEFAULT_TIMEOUT_SECS", DEFAULT_TIMEOUT_SECS)?;
        let max_timeout_secs = parse_u64_env("GEMINI_MAX_TIMEOUT_SECS", DEFAULT_MAX_TIMEOUT_SECS)?;
        if default_timeout_secs > max_timeout_secs {
            return Err(AppError::InvalidConfiguration(
                "GEMINI_DEFAULT_TIMEOUT_SECS must be <= GEMINI_MAX_TIMEOUT_SECS".to_string(),
            ));
        }

        let allowed_models = env::var("GEMINI_ALLOWED_MODELS").ok().and_then(|value| {
            let models = value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>();
            if models.is_empty() {
                None
            } else {
                Some(models)
            }
        });

        let allowed_cwd_prefixes = env::var_os("GEMINI_ALLOWED_CWD_PREFIXES")
            .map(|value| env::split_paths(&value).collect::<Vec<_>>())
            .filter(|paths| !paths.is_empty())
            .map(|paths| {
                paths
                    .into_iter()
                    .map(|path| canonicalize_existing_dir(&path))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let working_dir = env::current_dir().map_err(|error| {
            AppError::InvalidConfiguration(format!(
                "failed to determine current working directory: {error}"
            ))
        })?;

        Ok(Self::new(
            gemini_bin,
            default_timeout_secs,
            max_timeout_secs,
            allowed_models,
            allowed_cwd_prefixes,
            working_dir,
        ))
    }

    pub fn gemini_bin(&self) -> &str {
        &self.gemini_bin
    }

    pub fn default_timeout(&self) -> Duration {
        Duration::from_secs(self.default_timeout_secs)
    }

    pub fn resolve_timeout(&self, requested_secs: Option<u64>) -> Result<Duration, AppError> {
        let timeout_secs = requested_secs.unwrap_or(self.default_timeout_secs);
        if timeout_secs == 0 {
            return Err(AppError::InvalidParams(
                "timeout_secs must be greater than zero".to_string(),
            ));
        }
        if timeout_secs > self.max_timeout_secs {
            return Err(AppError::TimeoutTooLarge {
                requested_secs: timeout_secs,
                max_secs: self.max_timeout_secs,
            });
        }
        Ok(Duration::from_secs(timeout_secs))
    }

    pub fn resolve_model(&self, requested: Option<&str>) -> Result<Option<String>, AppError> {
        let model = requested
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        if let (Some(allowed), Some(model_name)) = (&self.allowed_models, &model)
            && !allowed.contains(model_name)
        {
            return Err(AppError::ModelNotAllowed {
                model: model_name.clone(),
                allowed: allowed.iter().cloned().collect(),
            });
        }

        Ok(model)
    }

    pub fn resolve_cwd(&self, requested: Option<&str>) -> Result<PathBuf, AppError> {
        let raw = requested.map(str::trim).filter(|value| !value.is_empty());
        let candidate = match raw {
            Some(path) => {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    path
                } else {
                    self.working_dir.join(path)
                }
            }
            None => self.working_dir.clone(),
        };

        let cwd = canonicalize_existing_dir(&candidate)?;

        if let Some(allowed_prefixes) = &self.allowed_cwd_prefixes
            && !allowed_prefixes
                .iter()
                .any(|prefix| cwd.starts_with(prefix))
        {
            return Err(AppError::WorkingDirectoryNotAllowed {
                cwd,
                allowed_prefixes: allowed_prefixes.clone(),
            });
        }

        Ok(cwd)
    }

    pub fn resolve_binary_path(&self) -> Option<PathBuf> {
        resolve_binary_path(self.gemini_bin())
    }
}

fn parse_u64_env(name: &str, default_value: u64) -> Result<u64, AppError> {
    match env::var(name) {
        Ok(value) => value.parse::<u64>().map_err(|error| {
            AppError::InvalidConfiguration(format!("failed to parse {name} as u64: {error}"))
        }),
        Err(env::VarError::NotPresent) => Ok(default_value),
        Err(env::VarError::NotUnicode(_)) => Err(AppError::InvalidConfiguration(format!(
            "{name} must be valid unicode"
        ))),
    }
}

fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf, AppError> {
    let metadata = path
        .metadata()
        .map_err(|_| AppError::InvalidWorkingDirectory(path.to_path_buf()))?;
    if !metadata.is_dir() {
        return Err(AppError::InvalidWorkingDirectory(path.to_path_buf()));
    }
    path.canonicalize()
        .map_err(|_| AppError::InvalidWorkingDirectory(path.to_path_buf()))
}

fn resolve_binary_path(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return command_path.exists().then(|| command_path.to_path_buf());
    }

    let path_value = env::var_os("PATH")?;
    env::split_paths(&path_value).find_map(|entry| find_executable_in_dir(&entry, command))
}

fn find_executable_in_dir(dir: &Path, command: &str) -> Option<PathBuf> {
    let direct = dir.join(command);
    if is_executable_candidate(&direct) {
        return Some(direct);
    }

    #[cfg(windows)]
    {
        for extension in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{command}.{extension}"));
            if is_executable_candidate(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

fn is_executable_candidate(path: &Path) -> bool {
    path.is_file() && !path.as_os_str().is_empty() && path.file_name() != Some(OsStr::new(""))
}
