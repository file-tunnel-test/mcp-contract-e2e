use std::{collections::BTreeMap, path::PathBuf};

use ore_mcp_config::{ConfigError, StrictConfig, is_environment_only_key};
use serde::Deserialize;

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct TypedConfig {
    test_root: String,
    test_count: i64,
    rust_log: String,
    api_url: String,
}

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli-flags.toml")
}

fn contract() -> StrictConfig {
    StrictConfig::new(contract_path())
}

#[test]
fn external_consumer_audits_and_applies_declared_defaults() {
    let config = contract();
    config.audit().expect("external flags contract must audit");

    let (resolved, typed): (_, TypedConfig) = config
        .resolve_typed(&["config-consumer".to_string()], &BTreeMap::new())
        .expect("declared defaults should resolve and coerce");

    assert_eq!(resolved.len(), 4);
    assert_eq!(resolved.get("TEST_ROOT"), Some("/default/root"));
    assert_eq!(resolved.get("TEST_COUNT"), Some("3"));
    assert_eq!(resolved.get("RUST_LOG"), Some("info,hyper=warn"));
    assert_eq!(
        resolved.get("API_URL"),
        Some("https://default.example.invalid")
    );
    assert_eq!(resolved.provided_keys().count(), 0);
    assert_eq!(
        typed,
        TypedConfig {
            test_root: "/default/root".to_string(),
            test_count: 3,
            rust_log: "info,hyper=warn".to_string(),
            api_url: "https://default.example.invalid".to_string(),
        }
    );
}

#[test]
fn external_consumer_applies_environment_then_argv_precedence() {
    let config = contract();
    let environment = BTreeMap::from([
        ("TEST_ROOT".to_string(), "/environment/root".to_string()),
        ("TEST_COUNT".to_string(), "5".to_string()),
        ("RUST_LOG".to_string(), "warn".to_string()),
        (
            "API_URL".to_string(),
            "https://environment.example.invalid".to_string(),
        ),
    ]);
    let argv = vec![
        "config-consumer".to_string(),
        "--root=/argv/root".to_string(),
        "--count=7".to_string(),
        "--log-filter=debug,hyper=warn".to_string(),
    ];

    let (resolved, typed): (_, TypedConfig) = config
        .resolve_typed(&argv, &environment)
        .expect("explicit environment and argv should resolve");

    assert_eq!(resolved.get("TEST_ROOT"), Some("/argv/root"));
    assert_eq!(resolved.get("TEST_COUNT"), Some("7"));
    assert_eq!(resolved.get("RUST_LOG"), Some("debug,hyper=warn"));
    assert_eq!(
        resolved.get("API_URL"),
        Some("https://environment.example.invalid")
    );
    assert_eq!(
        resolved.provided_keys().collect::<Vec<_>>(),
        vec!["RUST_LOG", "TEST_COUNT", "TEST_ROOT"]
    );
    assert_eq!(resolved.command(), None);
    assert!(resolved.subcommands().is_empty());
    assert_eq!(
        resolved
            .validated_log_filter("RUST_LOG", "info")
            .expect("bounded log filter should validate"),
        "debug,hyper=warn"
    );
    assert_eq!(typed.test_root, "/argv/root");
    assert_eq!(typed.test_count, 7);
    assert_eq!(typed.rust_log, "debug,hyper=warn");
    assert_eq!(typed.api_url, "https://environment.example.invalid");
}

#[test]
fn external_consumer_rejects_declared_sensitive_cli_keys_without_values() {
    let error = contract()
        .resolve(
            &[
                "config-consumer".to_string(),
                "--database-url=postgres://example.invalid/private-value".to_string(),
                "--api-token=test-secret-value".to_string(),
            ],
            &BTreeMap::new(),
        )
        .expect_err("credential-shaped argv keys must remain environment-only");

    let display = error.to_string();
    let debug = format!("{error:?}");
    match &error {
        ConfigError::SensitiveCliKeys { keys } => {
            assert_eq!(keys, &["API_TOKEN", "DATABASE_URL"]);
        }
        other => panic!("expected sensitive-key rejection, got {other:?}"),
    }
    for secret_fragment in ["postgres://", "private-value", "test-secret-value"] {
        assert!(!display.contains(secret_fragment));
        assert!(!debug.contains(secret_fragment));
    }
}

#[test]
fn external_consumer_redacts_unknown_option_values_and_positionals() {
    let unknown = contract()
        .resolve(
            &[
                "config-consumer".to_string(),
                "--unknown=private-option-value".to_string(),
            ],
            &BTreeMap::new(),
        )
        .expect_err("unknown option must fail closed");
    assert!(matches!(unknown, ConfigError::UnknownOptions { .. }));
    assert!(unknown.to_string().contains("unknown"));
    assert!(!unknown.to_string().contains("private-option-value"));

    let positional = contract()
        .resolve(
            &[
                "config-consumer".to_string(),
                "private/customer/path".to_string(),
            ],
            &BTreeMap::new(),
        )
        .expect_err("unexpected positional must fail closed");
    assert_eq!(
        positional,
        ConfigError::UnexpectedPositionals { count: 1 }
    );
    assert!(!positional.to_string().contains("customer"));
}

#[test]
fn external_consumer_summarizes_coercion_failure_without_raw_value() {
    let config = contract();
    let environment = BTreeMap::from([(
        "TEST_COUNT".to_string(),
        "not-a-private-number".to_string(),
    )]);
    let resolved = config
        .resolve(&["config-consumer".to_string()], &environment)
        .expect("explicit environment values resolve before typed coercion");
    let error = config
        .coerce::<TypedConfig>(&resolved)
        .expect_err("invalid integer must fail typed coercion");

    assert!(matches!(error, ConfigError::CoercionFailed { .. }));
    assert!(!error.to_string().contains("not-a-private-number"));
    assert!(!format!("{error:?}").contains("not-a-private-number"));
}

#[test]
fn external_consumer_rejects_invalid_filter_without_reflection() {
    let environment = BTreeMap::from([(
        "RUST_LOG".to_string(),
        "debug\nAuthorization: Bearer private-filter-value".to_string(),
    )]);
    let resolved = contract()
        .resolve(&["config-consumer".to_string()], &environment)
        .expect("environment filter is resolved before policy validation");
    let error = resolved
        .validated_log_filter("RUST_LOG", "info")
        .expect_err("control-bearing filter must fail");

    assert_eq!(
        error,
        ConfigError::InvalidLogFilter {
            key: "RUST_LOG".to_string()
        }
    );
    for secret_fragment in ["Authorization", "Bearer", "private-filter-value"] {
        assert!(!error.to_string().contains(secret_fragment));
        assert!(!format!("{error:?}").contains(secret_fragment));
    }
}

#[test]
fn external_consumer_debug_and_key_policy_never_expose_values_or_paths() {
    let environment = BTreeMap::from([
        ("TEST_ROOT".to_string(), "/private/customer/root".to_string()),
        ("API_TOKEN".to_string(), "private-environment-token".to_string()),
    ]);
    let config = contract();
    let resolved = config
        .resolve(&["config-consumer".to_string()], &environment)
        .expect("environment-only secrets may be resolved");

    let resolved_debug = format!("{resolved:?}");
    assert!(resolved_debug.contains("API_TOKEN"));
    assert!(resolved_debug.contains("TEST_ROOT"));
    assert!(!resolved_debug.contains("private-environment-token"));
    assert!(!resolved_debug.contains("customer"));

    let contract_debug = format!("{config:?}");
    assert!(contract_debug.contains("<configured>"));
    assert!(!contract_debug.contains("fixtures"));

    for key in [
        "API_TOKEN",
        "DATABASE_URL",
        "APP_REDIS_URL",
        "GPG_PASSPHRASE",
        "OTEL_EXPORTER_OTLP_HEADERS",
    ] {
        assert!(is_environment_only_key(key), "expected {key} to be protected");
    }
    for key in ["API_URL", "RUST_LOG", "ORG_ROOT", "SERVER_NAME"] {
        assert!(
            !is_environment_only_key(key),
            "expected {key} to remain product-policy controlled"
        );
    }
}
