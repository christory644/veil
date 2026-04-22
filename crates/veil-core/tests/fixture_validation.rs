//! Tests that validate the test fixture files exist and have correct content.
//!
//! These tests will FAIL until the config parsing infrastructure is built
//! (the fixture files exist but no Veil config parser uses them yet).
//! The TOML structural validation tests pass because they only check that
//! the files are syntactically valid/invalid TOML.

use std::path::PathBuf;

fn testdata_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/testdata").join(name)
}

// =========================================================================
// Config TOML fixtures: structural validation
// =========================================================================

#[test]
fn valid_config_toml_exists_and_parses() {
    let path = testdata_path("valid_config.toml");
    assert!(path.exists(), "valid_config.toml must exist at {}", path.display());
    let content = std::fs::read_to_string(&path).expect("should read valid_config.toml");
    let parsed: toml::Value =
        toml::from_str(&content).expect("valid_config.toml must be valid TOML");
    // Should have a [general] section
    assert!(parsed.get("general").is_some(), "valid_config.toml should have a [general] section");
}

#[test]
fn valid_config_toml_has_expected_sections() {
    let path = testdata_path("valid_config.toml");
    let content = std::fs::read_to_string(&path).expect("should read valid_config.toml");
    let parsed: toml::Value = toml::from_str(&content).expect("should parse as TOML");

    let expected_sections =
        ["general", "sidebar", "terminal", "keybindings", "aggregator", "appearance"];
    for section in &expected_sections {
        assert!(
            parsed.get(section).is_some(),
            "valid_config.toml should have a [{section}] section"
        );
    }
}

#[test]
fn minimal_config_toml_exists_and_parses() {
    let path = testdata_path("minimal_config.toml");
    assert!(path.exists(), "minimal_config.toml must exist at {}", path.display());
    let content = std::fs::read_to_string(&path).expect("should read minimal_config.toml");
    let parsed: toml::Value =
        toml::from_str(&content).expect("minimal_config.toml must be valid TOML");
    assert!(parsed.get("general").is_some(), "minimal_config.toml should have a [general] section");
}

#[test]
fn invalid_config_toml_exists_and_fails_to_parse() {
    let path = testdata_path("invalid_config.toml");
    assert!(path.exists(), "invalid_config.toml must exist at {}", path.display());
    let content = std::fs::read_to_string(&path).expect("should read invalid_config.toml");
    let result = toml::from_str::<toml::Value>(&content);
    assert!(result.is_err(), "invalid_config.toml must fail to parse as TOML");
}

#[test]
fn unknown_fields_config_toml_exists_and_parses() {
    let path = testdata_path("unknown_fields_config.toml");
    assert!(path.exists(), "unknown_fields_config.toml must exist at {}", path.display());
    let content = std::fs::read_to_string(&path).expect("should read unknown_fields_config.toml");
    let parsed: toml::Value =
        toml::from_str(&content).expect("unknown_fields_config.toml must be valid TOML");
    // Should have both known and unknown sections
    assert!(parsed.get("general").is_some(), "should have [general] section");
    assert!(
        parsed.get("future_section").is_some(),
        "should have [future_section] for forward compat testing"
    );
}

// =========================================================================
// Workspace state JSON fixture
// =========================================================================

#[test]
fn workspace_state_json_exists_and_is_valid_json() {
    let path = testdata_path("workspace_state.json");
    assert!(path.exists(), "workspace_state.json must exist at {}", path.display());
    let content = std::fs::read_to_string(&path).expect("should read workspace_state.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("workspace_state.json must be valid JSON");
    // Should have a workspaces array
    assert!(
        parsed.get("workspaces").is_some(),
        "workspace_state.json should have a 'workspaces' field"
    );
    let workspaces = parsed["workspaces"].as_array().expect("workspaces should be an array");
    assert!(!workspaces.is_empty(), "workspace_state.json should have at least one workspace");
}

#[test]
fn workspace_state_json_has_active_workspace_id() {
    let path = testdata_path("workspace_state.json");
    let content = std::fs::read_to_string(&path).expect("should read");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("should parse");
    assert!(
        parsed.get("active_workspace_id").is_some(),
        "workspace_state.json should have 'active_workspace_id' field"
    );
}

#[test]
fn workspace_state_json_has_sidebar_state() {
    let path = testdata_path("workspace_state.json");
    let content = std::fs::read_to_string(&path).expect("should read");
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("should parse");
    let sidebar = parsed.get("sidebar").expect("should have 'sidebar' field");
    assert!(sidebar.get("visible").is_some(), "sidebar should have 'visible' field");
    assert!(sidebar.get("active_tab").is_some(), "sidebar should have 'active_tab' field");
}
