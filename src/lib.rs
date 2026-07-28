use clap::{parser::ValueSource, ArgMatches, Args, CommandFactory, FromArgMatches, Parser};
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

// ---- Sub-configs ----

#[derive(Args, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DisplayConfig {
    #[arg(long = "width", default_value_t = 80)]
    pub width: u32,

    #[arg(long = "theme", default_value = "dark")]
    pub theme: String,
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
/// explicitly-supplied values; clap's own defaults act as the floor).
///
/// Takes an explicit argv (including program name at index 0) so this can be
/// exercised deterministically in tests instead of reading `std::env::args()`.
pub fn load_config_from<I, T>(args: I) -> Result<Cli, figment::Error>
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

    Ok(config)
}

/// Loads config using the real process argv.
pub fn load_config() -> Result<Cli, figment::Error> {
    load_config_from(std::env::args())
}
