use figment::Jail;
use figment_clap_together::{ConfigError, WIDTH_RANGE, load_config_from};

/// Sanity check the range constant itself so the tests below stay honest if
/// it's ever changed.
#[test]
fn width_range_is_10_to_500() {
    assert_eq!(*WIDTH_RANGE.start(), 10);
    assert_eq!(*WIDTH_RANGE.end(), 500);
}

// ---------------------------------------------------------------------
// InvalidRange: display.width out of range, from every possible source.
// ---------------------------------------------------------------------

#[test]
fn invalid_range_error_from_toml() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = 999
            "#,
        )?;

        let err = load_config_from(["myapp"]).expect_err("999 is out of range");
        match err {
            ConfigError::Validation(msg) => {
                assert!(
                    msg.contains("invalid range") && msg.contains("999"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Validation error, got: {other}"),
        }

        Ok(())
    });
}

#[test]
fn invalid_range_error_from_env() {
    Jail::expect_with(|jail| {
        jail.set_env("MYAPP_DISPLAY__WIDTH", "1");

        let err = load_config_from(["myapp"]).expect_err("1 is below the minimum of 10");
        match err {
            ConfigError::Validation(msg) => {
                assert!(msg.contains("invalid range"), "unexpected message: {msg}");
            }
            other => panic!("expected Validation error, got: {other}"),
        }

        Ok(())
    });
}

#[test]
fn invalid_range_error_from_cli() {
    Jail::expect_with(|_jail| {
        let err = load_config_from(["myapp", "--width", "1000"]).expect_err("1000 is out of range");
        match err {
            ConfigError::Validation(msg) => {
                assert!(msg.contains("invalid range"), "unexpected message: {msg}");
            }
            other => panic!("expected Validation error, got: {other}"),
        }

        Ok(())
    });
}

#[test]
fn width_at_range_boundaries_is_valid() {
    Jail::expect_with(|_jail| {
        let low = load_config_from(["myapp", "--width", "10"]).unwrap();
        assert_eq!(low.display.width, 10);

        let high = load_config_from(["myapp", "--width", "500"]).unwrap();
        assert_eq!(high.display.width, 500);

        Ok(())
    });
}

#[test]
fn width_just_outside_boundaries_is_invalid() {
    Jail::expect_with(|_jail| {
        assert!(load_config_from(["myapp", "--width", "9"]).is_err());
        assert!(load_config_from(["myapp", "--width", "501"]).is_err());

        Ok(())
    });
}

// ---------------------------------------------------------------------
// MutuallyExclusive: --no-color combined with a non-default theme.
// ---------------------------------------------------------------------

#[test]
fn mutually_exclusive_error_from_cli() {
    Jail::expect_with(|_jail| {
        let err = load_config_from(["myapp", "--no-color", "--theme", "neon"])
            .expect_err("no_color + non-dark theme should conflict");
        match err {
            ConfigError::Validation(msg) => {
                assert!(
                    msg.contains("mutually exclusive") && msg.contains("no_color"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Validation error, got: {other}"),
        }

        Ok(())
    });
}

#[test]
fn mutually_exclusive_error_from_toml() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            no_color = true
            theme = "neon"
            "#,
        )?;

        let err = load_config_from(["myapp"])
            .expect_err("no_color + non-dark theme from toml should conflict");
        match err {
            ConfigError::Validation(msg) => {
                assert!(
                    msg.contains("mutually exclusive"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Validation error, got: {other}"),
        }

        Ok(())
    });
}

#[test]
fn no_color_alone_is_fine() {
    Jail::expect_with(|_jail| {
        // --no-color with the (default) "dark" theme is not a conflict.
        let cfg = load_config_from(["myapp", "--no-color"]).unwrap();
        assert!(cfg.display.no_color);
        assert_eq!(cfg.display.theme, "dark");

        Ok(())
    });
}

#[test]
fn custom_theme_alone_is_fine() {
    Jail::expect_with(|_jail| {
        // A non-default theme without --no-color is not a conflict.
        let cfg = load_config_from(["myapp", "--theme", "neon"]).unwrap();
        assert!(!cfg.display.no_color);
        assert_eq!(cfg.display.theme, "neon");

        Ok(())
    });
}

// ---------------------------------------------------------------------
// Multiple errors collected at once (not just the first one found).
// ---------------------------------------------------------------------

#[test]
fn multiple_validation_errors_are_all_reported_together() {
    Jail::expect_with(|_jail| {
        // Both an out-of-range width AND a no_color/theme conflict at once.
        let err = load_config_from(["myapp", "--width", "999", "--no-color", "--theme", "neon"])
            .expect_err("both violations should surface");

        match err {
            ConfigError::Validation(msg) => {
                assert!(
                    msg.contains("2 configuration error"),
                    "expected both errors counted: {msg}"
                );
                assert!(msg.contains("invalid range"), "missing range error: {msg}");
                assert!(
                    msg.contains("mutually exclusive"),
                    "missing exclusivity error: {msg}"
                );
            }
            other => panic!("expected Validation error, got: {other}"),
        }

        Ok(())
    });
}

// ---------------------------------------------------------------------
// Figment error: malformed TOML surfaces as ConfigError::Figment, not a
// panic and not confused with a Validation error.
// ---------------------------------------------------------------------

#[test]
fn figment_error_on_malformed_toml() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display
            width = 100
            "#,
        )?;

        let err = load_config_from(["myapp"]).expect_err("malformed toml must fail");
        match err {
            ConfigError::Figment(_) => {}
            other => panic!("expected Figment error, got: {other}"),
        }

        Ok(())
    });
}

#[test]
fn figment_error_on_wrong_type_in_toml() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "config.toml",
            r#"
            [display]
            width = "not a number"
            "#,
        )?;

        let err = load_config_from(["myapp"]).expect_err("width must be a number");
        match err {
            ConfigError::Figment(_) => {}
            other => panic!("expected Figment error, got: {other}"),
        }

        Ok(())
    });
}

// ---------------------------------------------------------------------
// Display formatting sanity checks for each ConfigError variant, since
// main.rs relies on `{e}` (Display) to print user-facing error messages.
// ---------------------------------------------------------------------

#[test]
fn error_display_messages_are_human_readable() {
    use figment_clap_together::{ConfigError as CE, ConfigErrors};

    let range_err = CE::InvalidRange("display.width = 999 is out of range (10..=500)".into());
    assert_eq!(
        range_err.to_string(),
        "invalid range: display.width = 999 is out of range (10..=500)"
    );

    let exclusive_err = CE::MutuallyExclusive("a".into(), "b".into());
    assert_eq!(
        exclusive_err.to_string(),
        "options 'a' and 'b' are mutually exclusive"
    );

    let dependency_err = CE::Dependency("foo requires bar".into());
    assert_eq!(
        dependency_err.to_string(),
        "dependency error: foo requires bar"
    );

    let validation_err = CE::Validation("something went wrong".into());
    assert_eq!(
        validation_err.to_string(),
        "validation error: something went wrong"
    );

    let errors = ConfigErrors(vec![
        CE::InvalidRange("x".into()),
        CE::MutuallyExclusive("y".into(), "z".into()),
    ]);
    let rendered = errors.to_string();
    assert!(rendered.contains("found 2 configuration error(s)"));
    assert!(rendered.contains("1. invalid range: x"));
    assert!(rendered.contains("2. options 'y' and 'z' are mutually exclusive"));
}
