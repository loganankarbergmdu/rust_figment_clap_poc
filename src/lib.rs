use std::fmt;

use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

// ---- Errors ----

/// Errors that can occur during configuration loading or validation.
#[derive(Debug)]
pub enum ConfigError {
    /// Error surfaced by figment while merging/deserializing TOML, env, or
    /// CLI-baseline layers (covers I/O errors reading `config.toml`, TOML
    /// parse errors, and type-mismatch deserialization errors).
    Figment(figment::Error),

    /// General validation error with a free-form description.
    Validation(String),

    /// Two options that cannot be used together.
    MutuallyExclusive(String, String),

    /// An option that depends on another option being set.
    Dependency(String),

    /// A numeric value fell outside its allowed range.
    InvalidRange(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Figment(err) => write!(f, "config error: {err}"),
            Self::Validation(msg) => write!(f, "validation error: {msg}"),
            Self::MutuallyExclusive(opt1, opt2) => {
                write!(f, "options '{opt1}' and '{opt2}' are mutually exclusive")
            }
            Self::Dependency(msg) => write!(f, "dependency error: {msg}"),
            Self::InvalidRange(msg) => write!(f, "invalid range: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<figment::Error> for ConfigError {
    fn from(err: figment::Error) -> Self {
        Self::Figment(err)
    }
}

/// A collection of validation failures, gathered all at once rather than
/// stopping at the first problem found.
#[derive(Debug)]
pub struct ConfigErrors(pub Vec<ConfigError>);

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "found {} configuration error(s):", self.0.len())?;
        for (i, err) in self.0.iter().enumerate() {
            writeln!(f, "  {}. {err}", i + 1)?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigErrors {}

// ---- Sub-configs ----

#[derive(Args, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DisplayConfig {
    #[arg(long = "width", default_value_t = 80)]
    pub width: u32,

    #[arg(long = "theme", default_value = "dark")]
    pub theme: String,

    /// Disable color output entirely. Cannot be combined with a non-default
    /// `theme`, since a theme other than "dark" implies color is wanted.
    #[arg(long = "no-color", default_value_t = false)]
    #[serde(default)]
    pub no_color: bool,
}

#[derive(Args, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TerminalConfig {
    #[arg(long = "shell", default_value = "bash")]
    pub shell: String,

    #[arg(long = "scrollback", default_value_t = 1000)]
    pub scrollback: u32,
}

#[derive(Args, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GeneralConfig {
    #[arg(long = "log-level", default_value = "info")]
    pub log_level: String,
}

// ---- Top-level flattened struct ----

#[derive(Parser, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[command(name = "myapp", version, about)]
pub struct Cli {
    #[command(flatten)]
    pub display: DisplayConfig,

    #[command(flatten)]
    pub terminal: TerminalConfig,

    #[command(flatten)]
    pub general: GeneralConfig,
}

/// Allowed range for `display.width`, regardless of which source (TOML, env,
/// or CLI) it came from.
pub const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 10..=500;

impl Cli {
    /// Validates business rules on the fully-resolved config, after TOML,
    /// env, and CLI have already been merged together. This is the only
    /// place these rules can live, since by the time we have a `Cli` we no
    /// longer know or care which source each field's value came from -- a
    /// bad `width` is a bad `width` whether it came from `config.toml`, an
    /// env var, or a CLI flag.
    ///
    /// Collects every violation found rather than stopping at the first one,
    /// so a user fixing their config sees all problems in a single run.
    pub fn validate(&self) -> Result<(), ConfigErrors> {
        let mut errors = Vec::new();

        if !WIDTH_RANGE.contains(&self.display.width) {
            errors.push(ConfigError::InvalidRange(format!(
                "display.width = {} is out of range ({}..={})",
                self.display.width,
                WIDTH_RANGE.start(),
                WIDTH_RANGE.end()
            )));
        }

        if self.display.no_color && self.display.theme != "dark" {
            errors.push(ConfigError::MutuallyExclusive(
                "display.no_color".to_string(),
                format!("display.theme = \"{}\"", self.display.theme),
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigErrors(errors))
        }
    }
}

/// Clears any argument whose value came from clap's `default_value` (not
/// genuinely supplied by the user via CLI or env), leaving a sparse
/// `ArgMatches` containing only explicitly-provided values.
///
/// This is the key trick that lets CLI-provided values act as the highest
/// priority layer without clap's own defaults unconditionally clobbering
/// TOML/env layers underneath them.
fn strip_defaults(matches: &ArgMatches) -> ArgMatches {
    let mut stripped = matches.clone();
    for id in matches.ids() {
        let key = id.as_str();
        if matches.value_source(key) == Some(ValueSource::DefaultValue) {
            let _ = stripped.try_clear_id(key);
        }
    }
    stripped
}

/// Loads config layering TOML -> env -> CLI (CLI wins, but only for
/// explicitly-supplied values; clap's own defaults act as the floor), then
/// validates the fully-resolved result.
///
/// Takes an explicit argv (including program name at index 0) so this can be
/// exercised deterministically in tests instead of reading `std::env::args()`.
pub fn load_config_from<I, T>(args: I) -> Result<Cli, ConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let matches = Cli::command().get_matches_from(args);
    let matches_no_defaults = strip_defaults(&matches);

    // clap-derived defaults become the lowest priority figment layer
    let cli_defaults =
        Cli::from_arg_matches(&matches).expect("matches were produced by Cli::command()");

    let figment = Figment::new()
        .merge(Serialized::defaults(cli_defaults)) // baseline = clap defaults
        .merge(Toml::file("config.toml")) // overrides baseline
        // NOTE: use a double underscore as the section/field separator, not a
        // single underscore. `log_level` (and any other snake_case field
        // name) contains an underscore itself, so splitting on a single "_"
        // would incorrectly turn `MYAPP_GENERAL_LOG_LEVEL` into the nested
        // path `general.log.level` instead of `general.log_level`.
        .merge(Env::prefixed("MYAPP_").split("__")); // overrides toml

    let mut config: Cli = figment.extract()?;

    // apply only args the user actually supplied on the CLI (final override)
    config
        .update_from_arg_matches(&matches_no_defaults)
        .expect("matches were derived from same Cli type");

    if let Err(errors) = config.validate() {
        // Flatten the collected validation errors into the single
        // ConfigError this function returns. We keep this as a distinct
        // variant-free path (rather than adding a `ConfigError::Multiple`
        // variant) by just wrapping the formatted message; callers that want
        // structured access to each individual error should call
        // `config.validate()` directly instead of going through
        // `load_config_from`.
        return Err(ConfigError::Validation(errors.to_string()));
    }

    Ok(config)
}

/// Loads config using the real process argv.
pub fn load_config() -> Result<Cli, ConfigError> {
    load_config_from(std::env::args())
}
