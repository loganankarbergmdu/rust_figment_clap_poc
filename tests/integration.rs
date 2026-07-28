use figment::Jail;
use figment_clap_together::load_config_from;

/// No TOML, no env, no CLI args beyond program name -> pure clap defaults.
#[test]
fn defaults_when_nothing_provided() {
    Jail::expect_with(|_jail| {
        let cfg = load_config_from(["myapp"]).unwrap();

        assert_eq!(cfg.display.width, 80);
        assert_eq!(cfg.display.theme, "dark");
        assert_eq!(cfg.terminal.shell, "bash");
        assert_eq!(cfg.terminal.scrollback, 1000);
        assert_eq!(cfg.general.log_level, "info");

        Ok(())
    });
}

/// TOML values should override clap's baked-in defaults when nothing else
/// is provided.
#[test]
fn toml_overrides_clap_defaults() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 120

            [terminal]
            shell = "zsh"
            "#,
        )?;

        let cfg = load_config_from(["myapp"]).unwrap();

        assert_eq!(cfg.display.width, 120, "toml should override clap default");
        assert_eq!(
            cfg.display.theme, "dark",
            "untouched field keeps clap default"
        );
        assert_eq!(
            cfg.terminal.shell, "zsh",
            "toml should override clap default"
        );
        assert_eq!(
            cfg.terminal.scrollback, 1000,
            "untouched field keeps clap default"
        );

        Ok(())
    });
}

/// Env vars should override TOML values.
#[test]
fn env_overrides_toml() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 120
            "#,
        )?;
        jail.set_env("MYAPP_DISPLAY__WIDTH", "200");

        let cfg = load_config_from(["myapp"]).unwrap();

        assert_eq!(cfg.display.width, 200, "env should override toml");

        Ok(())
    });
}

/// The core touchy case #1: an explicit CLI flag must win over TOML/env,
/// even for fields not touched by the CLI at all (those should still come
/// from TOML/env, not silently reset to clap defaults).
#[test]
fn explicit_cli_flag_overrides_toml_and_env_selectively() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 120
            theme = "light"

            [terminal]
            shell = "zsh"
            "#,
        )?;
        jail.set_env("MYAPP_TERMINAL__SCROLLBACK", "5000");

        // Only --width is passed on the CLI; everything else should come
        // from TOML/env, NOT get reset to clap's compiled-in defaults.
        let cfg = load_config_from(["myapp", "--width", "400"]).unwrap();

        assert_eq!(cfg.display.width, 400, "explicit CLI flag wins");
        assert_eq!(
            cfg.display.theme, "light",
            "untouched by CLI, comes from toml"
        );
        assert_eq!(
            cfg.terminal.shell, "zsh",
            "untouched by CLI, comes from toml"
        );
        assert_eq!(
            cfg.terminal.scrollback, 5000,
            "untouched by CLI, comes from env"
        );

        Ok(())
    });
}

/// The core touchy case #2 (the one you were specifically worried about):
/// if the user explicitly passes a CLI flag whose value happens to be
/// IDENTICAL to clap's default_value, it must still be treated as an
/// explicit override -- NOT stripped out as if it were "just the default".
#[test]
fn explicit_cli_value_matching_default_still_overrides_toml() {
    Jail::expect_with(|jail| {
        // TOML says width = 500, clap's default_value_t for width is 80.
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 500
            "#,
        )?;

        // User explicitly types --width 80, which happens to equal clap's
        // default. This must NOT be confused with "user didn't pass --width".
        let cfg = load_config_from(["myapp", "--width", "80"]).unwrap();

        assert_eq!(
            cfg.display.width, 80,
            "explicit --width 80 must override toml's 500, even though 80 == clap default"
        );

        Ok(())
    });
}

/// Mirror case: if the CLI flag is genuinely NOT passed, TOML must be free
/// to set the field to a value different from clap's default -- clap's
/// default must not "win" just because it's the fallback.
#[test]
fn omitted_cli_flag_never_clobbers_toml_with_clap_default() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 500
            "#,
        )?;

        // --width is not passed at all.
        let cfg = load_config_from(["myapp"]).unwrap();

        assert_eq!(
            cfg.display.width, 500,
            "clap's unused default_value must not clobber toml's explicit value"
        );

        Ok(())
    });
}

/// Full layering sanity check across all three sources at once, touching
/// every sub-config, to make sure flatten + figment + clap compose
/// correctly end-to-end.
#[test]
fn full_layering_across_all_sources() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 100
            theme = "solarized"

            [terminal]
            shell = "fish"
            scrollback = 2000

            [general]
            log_level = "warn"
            "#,
        )?;
        jail.set_env("MYAPP_GENERAL__LOG_LEVEL", "debug");
        jail.set_env("MYAPP_TERMINAL__SHELL", "nu");

        // CLI explicitly overrides only theme.
        let cfg = load_config_from(["myapp", "--theme", "solarized"]).unwrap();

        assert_eq!(cfg.display.width, 100, "from toml");
        assert_eq!(
            cfg.display.theme, "solarized",
            "from explicit cli (equals toml value too)"
        );
        assert_eq!(cfg.terminal.shell, "nu", "env overrides toml");
        assert_eq!(cfg.terminal.scrollback, 2000, "from toml");
        assert_eq!(cfg.general.log_level, "debug", "env overrides toml");

        Ok(())
    });
}
