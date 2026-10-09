// The README examples use TOML.
#![cfg_attr(feature = "toml", doc = include_str!("../README.md"))]

use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(any(feature = "toml", feature = "json", feature = "yaml"))]
use figment::providers::Format as _;
#[cfg(feature = "json")]
use figment::providers::Json;
#[cfg(feature = "toml")]
use figment::providers::Toml;
#[cfg(feature = "yaml")]
use figment::providers::Yaml;
use figment::providers::{Env, Serialized};
use figment::{Figment, Provider};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use bon;
pub use figment;
pub use serde;

/// Makes a struct a configuration struct: see "Configuration structs" in the crate docs.
pub use bonfig_macros::settings;

/// A figment error: it names the key and its source (file path or env var).
pub type Error = Box<figment::Error>;
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Loads configuration in layers, from low to high priority:
///
/// 1. The base value: `T::default()` for [`Loader::load`].
/// 2. Optional files, in the order you add them. A missing file is skipped.
///    The extension picks the format: `toml`, `json`, `yaml` or `yml`,
///    each behind the Cargo feature of the same name.
/// 3. The named file: `config_path`, else the path in `{PREFIX}_CONFIG_FILE`.
///    A missing named file is an error.
/// 4. Env vars `{PREFIX}__A__B` for the key `a.b`.
///
/// All layers merge: maps merge key by key, lists and scalars replace.
#[derive(Debug, Clone, bon::Builder)]
#[builder(on(String, into))]
pub struct Loader {
    #[builder(field)]
    files: Vec<PathBuf>,
    /// `WR` reads values from `WR__A__B` and the file path from `WR_CONFIG_FILE`.
    env_prefix: Option<String>,
    /// The named file, for example from a `--config` flag. It wins over `{PREFIX}_CONFIG_FILE`.
    #[builder(into)]
    config_path: Option<PathBuf>,
}

impl<S: loader_builder::State> LoaderBuilder<S> {
    /// Adds an optional file. A missing file is skipped.
    pub fn file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(path.into());
        self
    }

    /// Adds optional files. Accepts an `Option`, for example from [`xdg_config_file`].
    pub fn files<P: Into<PathBuf>>(mut self, paths: impl IntoIterator<Item = P>) -> Self {
        self.files.extend(paths.into_iter().map(Into::into));
        self
    }
}

impl Loader {
    /// Loads `T` over `T::default()` and logs the result with `Debug`.
    /// Use [`Secret`] for fields that must not appear in the log.
    pub fn load<T>(&self) -> Result<T>
    where
        T: Default + Serialize + DeserializeOwned + fmt::Debug,
    {
        let value: T = self.figment(&T::default())?.extract()?;
        #[cfg(feature = "tracing")]
        tracing::info!(config = ?value, "configuration loaded");
        Ok(value)
    }

    /// Loads `T` from a cascade of sections over `base`, general first.
    ///
    /// `load_focused(&["redis", "cache.redis"], settings.cache.redis)` starts
    /// from the loaded value, applies `[redis]`, then applies `[cache.redis]` again
    /// so that its own keys win.
    pub fn load_focused<T>(&self, focuses: &[&str], base: T) -> Result<T>
    where
        T: Serialize + DeserializeOwned,
    {
        let sources = self.sources()?;
        let figment = focuses
            .iter()
            .fold(Figment::from(Serialized::defaults(base)), |figment, key| {
                figment.merge(sources.focus(key))
            });
        Ok(figment.extract()?)
    }

    /// All layers over `base`, for extra providers such as CLI flags.
    pub fn figment<T: Serialize>(&self, base: &T) -> Result<Figment> {
        Ok(Figment::from(Serialized::defaults(base)).merge(self.sources()?))
    }

    /// Layers 2 to 4, without a base value.
    fn sources(&self) -> Result<Figment> {
        let mut figment = Figment::new();
        for path in &self.files {
            // shortcut: an unreadable parent directory also skips the file, match on fs::metadata errors if that must fail
            if path.is_file() {
                figment = merge_file(figment, path)?;
            }
        }
        if let Some(path) = self.named_file() {
            if !path.is_file() {
                return Err(Box::new(
                    format!("configuration file not found: {}", path.display()).into(),
                ));
            }
            figment = merge_file(figment, &path)?;
        }
        if let Some(prefix) = &self.env_prefix {
            figment = figment.merge(Env::prefixed(&format!("{prefix}__")).split("__"));
        }
        Ok(figment)
    }

    fn named_file(&self) -> Option<PathBuf> {
        self.config_path.clone().or_else(|| {
            let prefix = self.env_prefix.as_ref()?;
            std::env::var_os(format!("{prefix}_CONFIG_FILE"))
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
    }
}

/// Merges the file at `path` with the format from its extension.
// Without format features, only the error path is left.
#[cfg_attr(
    not(any(feature = "toml", feature = "json", feature = "yaml")),
    allow(unused_variables, clippy::needless_pass_by_value)
)]
fn merge_file(figment: Figment, path: &Path) -> Result<Figment> {
    #[cfg(feature = "tracing")]
    tracing::info!(path = %path.display(), "merge configuration file");
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    // file_exact: file() searches parent directories for a relative path.
    match ext.as_str() {
        #[cfg(feature = "toml")]
        "toml" => return Ok(figment.merge(Toml::file_exact(path))),
        #[cfg(feature = "json")]
        "json" => return Ok(figment.merge(Json::file_exact(path))),
        #[cfg(feature = "yaml")]
        "yaml" | "yml" => return Ok(figment.merge(Yaml::file_exact(path))),
        _ => {}
    }
    Err(Box::new(
        format!(
            "unsupported configuration file: {}. Use .toml, .json or .yaml \
             with the bonfig feature of the same name",
            path.display()
        )
        .into(),
    ))
}

/// Builds a configuration from one source over `Self::default()`.
/// It reads no env vars and no files, unless the provider does.
///
/// Implemented for every `T: Default + Serialize + DeserializeOwned`:
/// `use bonfig::FromSource as _;` and call `Settings::from_toml(s)`.
pub trait FromSource: Sized {
    /// Merges any figment provider: a string or a file in some format, or env vars.
    fn from_provider(provider: impl Provider) -> Result<Self>;

    /// Merges any `Serialize` value, for example a `serde_json::Value`, a map, or a struct.
    fn from_serialize(value: impl Serialize) -> Result<Self> {
        Self::from_provider(Serialized::defaults(value))
    }

    #[cfg(feature = "toml")]
    fn from_toml(toml: &str) -> Result<Self> {
        Self::from_provider(Toml::string(toml))
    }

    #[cfg(feature = "json")]
    fn from_json(json: &str) -> Result<Self> {
        Self::from_provider(Json::string(json))
    }

    #[cfg(feature = "yaml")]
    fn from_yaml(yaml: &str) -> Result<Self> {
        Self::from_provider(Yaml::string(yaml))
    }
}

impl<T: Default + Serialize + DeserializeOwned> FromSource for T {
    fn from_provider(provider: impl Provider) -> Result<Self> {
        Ok(Figment::from(Serialized::defaults(Self::default()))
            .merge(provider)
            .extract()?)
    }
}

/// Merges a partial value onto `base`: maps merge key by key, the rest replaces.
/// Use it for runtime updates, for example a JSON `serde_json::Value` from a client.
pub fn patch<T>(base: &T, changes: impl Serialize) -> Result<T>
where
    T: Serialize + DeserializeOwned,
{
    Ok(Figment::from(Serialized::defaults(base))
        .merge(Serialized::defaults(changes))
        .extract()?)
}

/// `$XDG_CONFIG_HOME/{app}/config.toml`, else `$HOME/.config/{app}/config.toml`.
/// Returns `None` when neither variable is set.
#[must_use]
pub fn xdg_config_file(app: &str) -> Option<PathBuf> {
    // shortcut: XDG layout on every OS, use the etcetera crate if native Windows paths are needed
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join(app).join("config.toml"))
}

/// A value that `Debug` prints as `***`. Serde sees the real value, because
/// figment serializes the defaults. So log configuration with `Debug`, not JSON.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    pub const fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Implements `Default` as `Self::builder().build()` for `bon::Builder` structs,
/// so that `#[builder(default = ...)]` is the only place for defaults.
/// It compiles only when every field has a default.
#[macro_export]
macro_rules! default_from_builder {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ::core::default::Default for $ty {
                fn default() -> Self {
                    Self::builder().build()
                }
            }
        )+
    };
}
