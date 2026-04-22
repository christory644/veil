#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Returns the workspace root directory (two levels up from this crate's manifest dir).
    fn workspace_root() -> PathBuf {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().expect("crates/ dir").parent().expect("workspace root").to_path_buf()
    }

    // =========================================================================
    // Unit 1: Workspace Cargo.toml and Crate Skeletons
    // =========================================================================

    /// The 7 crates that must exist under crates/.
    const EXPECTED_CRATES: &[&str] = &[
        "veil",
        "veil-core",
        "veil-ghostty",
        "veil-ui",
        "veil-pty",
        "veil-aggregator",
        "veil-socket",
    ];

    /// Library crates (have src/lib.rs).
    const LIBRARY_CRATES: &[&str] =
        &["veil-core", "veil-ghostty", "veil-ui", "veil-pty", "veil-aggregator", "veil-socket"];

    /// The single binary crate (has src/main.rs).
    const BINARY_CRATE: &str = "veil";

    #[test]
    fn workspace_cargo_toml_exists() {
        let root = workspace_root();
        let cargo_toml = root.join("Cargo.toml");
        assert!(cargo_toml.exists(), "expected {} to exist", cargo_toml.display());
    }

    #[test]
    fn workspace_cargo_toml_declares_all_crate_members() {
        let root = workspace_root();
        let cargo_toml = root.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml).unwrap_or_default();

        for krate in EXPECTED_CRATES {
            let member_path = format!("crates/{krate}");
            assert!(
                content.contains(&member_path),
                "workspace Cargo.toml must list member '{}', but it was not found in:\n{}",
                member_path,
                content
            );
        }
    }

    #[test]
    fn workspace_cargo_toml_has_workspace_dependencies() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();

        assert!(
            content.contains("[workspace.dependencies]"),
            "workspace Cargo.toml must contain [workspace.dependencies] section"
        );

        let expected_deps = ["thiserror", "anyhow", "tracing", "serde", "tokio"];
        for dep in &expected_deps {
            assert!(
                content.contains(dep),
                "workspace Cargo.toml must declare '{}' in [workspace.dependencies]",
                dep
            );
        }
    }

    #[test]
    fn workspace_cargo_toml_has_clippy_lints() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();

        assert!(
            content.contains("[workspace.lints.clippy]"),
            "workspace Cargo.toml must contain [workspace.lints.clippy] section"
        );
    }

    #[test]
    fn all_crate_directories_exist() {
        let root = workspace_root();
        for krate in EXPECTED_CRATES {
            let crate_dir = root.join("crates").join(krate);
            assert!(
                crate_dir.exists(),
                "expected crate directory {} to exist",
                crate_dir.display()
            );
        }
    }

    #[test]
    fn all_crates_have_cargo_toml() {
        let root = workspace_root();
        for krate in EXPECTED_CRATES {
            let cargo_toml = root.join("crates").join(krate).join("Cargo.toml");
            assert!(cargo_toml.exists(), "expected {} to exist", cargo_toml.display());
        }
    }

    #[test]
    fn library_crates_have_lib_rs() {
        let root = workspace_root();
        for krate in LIBRARY_CRATES {
            let lib_rs = root.join("crates").join(krate).join("src").join("lib.rs");
            assert!(lib_rs.exists(), "expected {} to exist", lib_rs.display());
        }
    }

    #[test]
    fn binary_crate_has_main_rs() {
        let root = workspace_root();
        let main_rs = root.join("crates").join(BINARY_CRATE).join("src").join("main.rs");
        assert!(main_rs.exists(), "expected {} to exist", main_rs.display());
    }

    #[test]
    fn library_crates_deny_unsafe_code() {
        let root = workspace_root();
        // All library crates except veil-ghostty must have #![deny(unsafe_code)]
        let deny_unsafe_crates: Vec<&str> =
            LIBRARY_CRATES.iter().filter(|&&c| c != "veil-ghostty").copied().collect();

        for krate in &deny_unsafe_crates {
            let lib_rs = root.join("crates").join(krate).join("src").join("lib.rs");
            let content = fs::read_to_string(&lib_rs).unwrap_or_default();
            assert!(
                content.contains("#![deny(unsafe_code)]"),
                "{}/src/lib.rs must contain #![deny(unsafe_code)]",
                krate
            );
        }
    }

    #[test]
    fn library_crates_warn_missing_docs() {
        let root = workspace_root();
        for krate in LIBRARY_CRATES {
            let lib_rs = root.join("crates").join(krate).join("src").join("lib.rs");
            let content = fs::read_to_string(&lib_rs).unwrap_or_default();
            assert!(
                content.contains("#![warn(missing_docs)]"),
                "{}/src/lib.rs must contain #![warn(missing_docs)]",
                krate
            );
        }
    }

    #[test]
    fn binary_crate_denies_unsafe_code() {
        let root = workspace_root();
        let main_rs = root.join("crates").join(BINARY_CRATE).join("src").join("main.rs");
        let content = fs::read_to_string(&main_rs).unwrap_or_default();
        assert!(
            content.contains("#![deny(unsafe_code)]"),
            "veil/src/main.rs must contain #![deny(unsafe_code)]"
        );
    }

    #[test]
    fn veil_ghostty_denies_unsafe_op_in_unsafe_fn() {
        let root = workspace_root();
        let lib_rs = root.join("crates").join("veil-ghostty").join("src").join("lib.rs");
        let content = fs::read_to_string(&lib_rs).unwrap_or_default();
        assert!(
            content.contains("#![deny(unsafe_op_in_unsafe_fn)]"),
            "veil-ghostty/src/lib.rs must contain #![deny(unsafe_op_in_unsafe_fn)]"
        );
        // It must NOT have deny(unsafe_code) since it needs unsafe for FFI
        assert!(
            !content.contains("#![deny(unsafe_code)]"),
            "veil-ghostty/src/lib.rs must NOT contain #![deny(unsafe_code)]"
        );
    }

    #[test]
    fn all_crates_inherit_workspace_lints() {
        let root = workspace_root();
        for krate in EXPECTED_CRATES {
            let cargo_toml = root.join("crates").join(krate).join("Cargo.toml");
            let content = fs::read_to_string(&cargo_toml).unwrap_or_default();
            assert!(
                content.contains("[lints]") && content.contains("workspace = true"),
                "{}/Cargo.toml must contain [lints] workspace = true",
                krate
            );
        }
    }

    #[test]
    fn library_crates_depend_on_veil_core() {
        let root = workspace_root();
        // All library crates except veil-core itself should depend on veil-core
        let dependent_crates: Vec<&str> =
            LIBRARY_CRATES.iter().filter(|&&c| c != "veil-core").copied().collect();

        for krate in &dependent_crates {
            let cargo_toml = root.join("crates").join(krate).join("Cargo.toml");
            let content = fs::read_to_string(&cargo_toml).unwrap_or_default();
            assert!(content.contains("veil-core"), "{}/Cargo.toml must depend on veil-core", krate);
        }
    }

    #[test]
    fn binary_crate_depends_on_all_library_crates() {
        let root = workspace_root();
        let cargo_toml = root.join("crates").join(BINARY_CRATE).join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml).unwrap_or_default();

        for krate in LIBRARY_CRATES {
            assert!(content.contains(krate), "veil/Cargo.toml must depend on '{}'", krate);
        }
    }

    // =========================================================================
    // Unit 2: Linting Configuration
    // =========================================================================

    #[test]
    fn rustfmt_toml_exists() {
        let root = workspace_root();
        let path = root.join("rustfmt.toml");
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    #[test]
    fn clippy_toml_exists() {
        let root = workspace_root();
        let path = root.join("clippy.toml");
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    // =========================================================================
    // Unit 3: build.rs Scaffold for libghosty
    // =========================================================================

    #[test]
    fn veil_ghostty_has_build_rs() {
        let root = workspace_root();
        let build_rs = root.join("crates").join("veil-ghostty").join("build.rs");
        assert!(build_rs.exists(), "expected {} to exist", build_rs.display());
    }

    #[test]
    fn veil_ghostty_build_rs_sets_no_libghosty_cfg() {
        let root = workspace_root();
        let build_rs = root.join("crates").join("veil-ghostty").join("build.rs");
        let content = fs::read_to_string(&build_rs).unwrap_or_default();
        assert!(
            content.contains("no_libghosty"),
            "veil-ghostty/build.rs must set 'no_libghosty' cfg flag when libghosty is not found"
        );
    }

    // =========================================================================
    // Unit 4: License Files
    // =========================================================================

    #[test]
    fn license_mit_exists() {
        let root = workspace_root();
        let path = root.join("LICENSE-MIT");
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    #[test]
    fn license_apache_exists() {
        let root = workspace_root();
        let path = root.join("LICENSE-APACHE");
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    #[test]
    fn license_mit_contains_mit_text() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("LICENSE-MIT")).unwrap_or_default();
        assert!(
            content.contains("MIT License") || content.contains("Permission is hereby granted"),
            "LICENSE-MIT must contain standard MIT license text"
        );
    }

    #[test]
    fn license_apache_contains_apache_text() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("LICENSE-APACHE")).unwrap_or_default();
        assert!(
            content.contains("Apache License") && content.contains("Version 2.0"),
            "LICENSE-APACHE must contain standard Apache-2.0 license text"
        );
    }

    #[test]
    fn workspace_cargo_toml_license_field_is_dual() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
        assert!(
            content.contains("MIT OR Apache-2.0"),
            "workspace Cargo.toml must set license to 'MIT OR Apache-2.0'"
        );
    }

    // =========================================================================
    // Unit 5: GitHub Actions CI
    // =========================================================================

    #[test]
    fn ci_workflow_exists() {
        let root = workspace_root();
        let path = root.join(".github").join("workflows").join("ci.yml");
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    #[test]
    fn ci_workflow_is_valid_yaml_with_matrix() {
        let root = workspace_root();
        let path = root.join(".github").join("workflows").join("ci.yml");
        let content = fs::read_to_string(&path).unwrap_or_default();

        // Verify it covers all 3 OS targets
        assert!(
            content.contains("macos-latest") || content.contains("macos"),
            "CI workflow must include macOS in the matrix"
        );
        assert!(
            content.contains("ubuntu-latest") || content.contains("ubuntu"),
            "CI workflow must include Ubuntu in the matrix"
        );
        assert!(
            content.contains("windows-latest") || content.contains("windows"),
            "CI workflow must include Windows in the matrix"
        );
    }

    #[test]
    fn ci_workflow_includes_quality_gate_steps() {
        let root = workspace_root();
        let path = root.join(".github").join("workflows").join("ci.yml");
        let content = fs::read_to_string(&path).unwrap_or_default();

        assert!(content.contains("cargo fmt"), "CI workflow must include 'cargo fmt' step");
        assert!(content.contains("cargo clippy"), "CI workflow must include 'cargo clippy' step");
        assert!(content.contains("cargo build"), "CI workflow must include 'cargo build' step");
        assert!(content.contains("cargo test"), "CI workflow must include 'cargo test' step");
    }

    #[test]
    fn ci_workflow_triggers_on_correct_branches() {
        let root = workspace_root();
        let path = root.join(".github").join("workflows").join("ci.yml");
        let content = fs::read_to_string(&path).unwrap_or_default();

        assert!(content.contains("main"), "CI workflow must trigger on 'main' branch");
        assert!(content.contains("ralph-loop"), "CI workflow must trigger on 'ralph-loop' branch");
    }

    #[test]
    fn ci_workflow_uses_rust_cache() {
        let root = workspace_root();
        let path = root.join(".github").join("workflows").join("ci.yml");
        let content = fs::read_to_string(&path).unwrap_or_default();

        assert!(
            content.contains("rust-cache") || content.contains("Swatinem/rust-cache"),
            "CI workflow must use rust-cache for build speed"
        );
    }

    #[test]
    fn ci_workflow_disables_fail_fast() {
        let root = workspace_root();
        let path = root.join(".github").join("workflows").join("ci.yml");
        let content = fs::read_to_string(&path).unwrap_or_default();

        assert!(
            content.contains("fail-fast: false"),
            "CI workflow matrix must set 'fail-fast: false' for independent OS reporting"
        );
    }

    // =========================================================================
    // Unit 6: CONTRIBUTING.md
    // =========================================================================

    #[test]
    fn contributing_md_exists() {
        let root = workspace_root();
        let path = root.join("CONTRIBUTING.md");
        assert!(path.exists(), "expected {} to exist", path.display());
    }

    #[test]
    fn contributing_md_references_agent_adapter_trait() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("CONTRIBUTING.md")).unwrap_or_default();
        assert!(
            content.contains("AgentAdapter"),
            "CONTRIBUTING.md must reference the AgentAdapter trait"
        );
    }

    #[test]
    fn contributing_md_references_quality_gate() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("CONTRIBUTING.md")).unwrap_or_default();

        assert!(
            content.contains("cargo fmt") || content.contains("cargo clippy"),
            "CONTRIBUTING.md must reference quality gate commands"
        );
    }

    #[test]
    fn contributing_md_has_required_sections() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join("CONTRIBUTING.md")).unwrap_or_default();

        let required_sections =
            ["Getting Started", "Development Workflow", "Agent Adapter", "Code Review", "License"];
        for section in &required_sections {
            assert!(
                content.contains(section),
                "CONTRIBUTING.md must contain a '{}' section",
                section
            );
        }
    }

    // =========================================================================
    // Unit 7: .gitignore Fix
    // =========================================================================

    #[test]
    fn gitignore_does_not_ignore_cargo_lock() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join(".gitignore")).unwrap_or_default();

        // Check that Cargo.lock is NOT listed as a pattern to ignore.
        // It could appear as "Cargo.lock" on its own line.
        let ignores_cargo_lock = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == "Cargo.lock" || trimmed == "/Cargo.lock"
        });

        assert!(
            !ignores_cargo_lock,
            ".gitignore must NOT ignore Cargo.lock — binary projects should commit the lockfile"
        );
    }

    #[test]
    fn gitignore_still_ignores_target() {
        let root = workspace_root();
        let content = fs::read_to_string(root.join(".gitignore")).unwrap_or_default();

        let ignores_target = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed == "target/" || trimmed == "/target/" || trimmed == "target"
        });

        assert!(ignores_target, ".gitignore must still ignore the target/ directory");
    }
}
