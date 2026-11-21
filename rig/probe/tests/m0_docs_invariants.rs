//! Milestone 0 exit-invariant tests across documentation, `justfile`, `rig/expected`, and `rig/`.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const EXPECTED_BASELINE_FILES: &[&str] = &[
    "iterm2-tmux-cc.json",
    "iterm2-tmux-full.json",
    "iterm2-tmux-stock.json",
    "iterm2.json",
    "orca-tmux.json",
    "orca.json",
    "xterm-linux.json",
    "xterm-macos.json",
    "xterm-tmux-full-linux.json",
    "xterm-tmux-full-macos.json",
    "xterm-tmux-stock-linux.json",
    "xterm-tmux-stock-macos.json",
    "xterm-windows.json",
];

const TOUCHED_FILES_WITHOUT_EM_DASH: &[&str] = &[
    "docs/terminals.md",
    "docs/testing.md",
    "docs/architecture.md",
    "docs/decisions.md",
    "README.md",
    "rig/probe/tests/m0_docs_invariants.rs",
];

const RIG_SLEEP_SCAN_TARGETS: &[&str] = &[
    "rig/iterm2",
    "rig/linux",
    "rig/orca",
    "rig/probe/examples",
    "rig/probe/src",
    "rig/tmux",
    "rig/windows",
    "rig/xterm/src",
    "rig/xterm/web",
    "rig/xterm/run.sh",
    "rig/xterm/run-tmux.sh",
];

const SKIPPED_FILE_EXTENSIONS: &[&str] = &["pyc", "pyo", "png", "swp"];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn read_repo_file(relative_path: &str) -> String {
    let full_path = repository_root().join(relative_path);
    fs::read_to_string(&full_path)
        .unwrap_or_else(|read_error| panic!("failed to read {}: {read_error}", full_path.display()))
        .replace("\r\n", "\n")
}

fn extract_markdown_h2_section(markdown: &str, heading: &str) -> String {
    let target_heading = format!("## {heading}");
    let mut section_lines = Vec::new();
    let mut is_in_section = false;
    for line in markdown.lines() {
        if is_in_section {
            if line.starts_with("## ") {
                break;
            }
            section_lines.push(line);
        } else if line.trim_end() == target_heading {
            is_in_section = true;
        }
    }
    assert!(is_in_section, "missing heading '## {heading}'");
    section_lines.join("\n")
}

fn has_standalone_ascii_word(haystack: &str, target_word: &str) -> bool {
    let lowered_haystack = haystack.to_ascii_lowercase();
    let lowered_target = target_word.to_ascii_lowercase();
    lowered_haystack
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == lowered_target)
}

fn is_skipped_dir(path: &Path) -> bool {
    let Some(dir_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    matches!(
        dir_name,
        "node_modules" | "__pycache__" | "target" | ".pytest_cache" | ".mypy_cache" | ".ruff_cache"
    )
}

fn is_skipped_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    file_name == ".DS_Store"
        || Path::new(file_name).extension().is_some_and(|extension| {
            SKIPPED_FILE_EXTENSIONS
                .iter()
                .any(|skipped_extension| extension.eq_ignore_ascii_case(skipped_extension))
        })
}

fn collect_rig_source_files(target_path: &Path, collected_paths: &mut Vec<PathBuf>) {
    let root_type = fs::symlink_metadata(target_path)
        .unwrap_or_else(|read_error| {
            panic!("failed to stat {}: {read_error}", target_path.display())
        })
        .file_type();
    if root_type.is_file() {
        if !is_skipped_file(target_path) {
            collected_paths.push(target_path.to_path_buf());
        }
        return;
    }
    if !root_type.is_dir() || is_skipped_dir(target_path) {
        return;
    }
    let directory_entries = fs::read_dir(target_path).unwrap_or_else(|read_error| {
        panic!("failed to list {}: {read_error}", target_path.display())
    });
    for entry_result in directory_entries {
        let entry = entry_result.expect("directory entry");
        let file_type = entry.file_type().expect("directory entry file type");
        let entry_path = entry.path();
        if file_type.is_dir() && !is_skipped_dir(&entry_path) {
            collect_rig_source_files(&entry_path, collected_paths);
        } else if file_type.is_file() && !is_skipped_file(&entry_path) {
            collected_paths.push(entry_path);
        }
    }
}

fn parse_justfile_recipe_bodies(justfile_text: &str) -> BTreeMap<String, Vec<String>> {
    let mut recipes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut active_recipe: Option<String> = None;
    for line in justfile_text.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(recipe_name) = &active_recipe {
                recipes
                    .entry(recipe_name.clone())
                    .or_default()
                    .push(line.trim().to_owned());
            }
            continue;
        }
        let Some((header_left, _)) = line.split_once(':') else {
            active_recipe = None;
            continue;
        };
        let recipe_token = header_left.split_whitespace().next().unwrap_or("");
        let recipe_name = recipe_token.to_owned();
        active_recipe = Some(recipe_name.clone());
        recipes.entry(recipe_name).or_default();
    }
    recipes
}

fn assert_contains_all(haystack: &str, context: &str, required_fragments: &[&str]) {
    for required_fragment in required_fragments {
        assert!(
            haystack.contains(required_fragment),
            "{context} missing '{required_fragment}'"
        );
    }
}

fn assert_lacks_all(haystack: &str, context: &str, forbidden_fragments: &[&str]) {
    for forbidden_fragment in forbidden_fragments {
        assert!(
            !haystack.contains(forbidden_fragment),
            "{context} unexpectedly contains '{forbidden_fragment}'"
        );
    }
}

#[test]
fn terminals_doc_has_zero_verify_markers_and_records_conpty_and_tmux_3_3() {
    let terminals_doc = read_repo_file("docs/terminals.md");
    assert!(
        !has_standalone_ascii_word(&terminals_doc, "verify"),
        "docs/terminals.md still contains standalone 'Verify'/'verify'"
    );
    let required_fragments = [
        "ConPTY",
        "tmux 3.3",
        "tmux 3.8",
        "tmux -CC",
        "cmd.exe /D /S /C",
        "`\"\\^^^\"\"`",
        "rig/expected",
    ];
    assert_contains_all(&terminals_doc, "docs/terminals.md", &required_fragments);
}

#[test]
fn testing_doc_aligns_runner_cli_flags_and_expected_compare() {
    let testing_doc = read_repo_file("docs/testing.md");
    let required_fragments = [
        "orca terminal create",
        "expected_compare.mjs",
        "--tmux <stock|full>",
        "expected_baselines.rs",
    ];
    assert_contains_all(&testing_doc, "docs/testing.md", &required_fragments);
    assert_lacks_all(
        &testing_doc,
        "docs/testing.md",
        &["--tmux-config", "orca terminal spawn"],
    );
}

#[test]
fn testing_doc_covers_all_layers_vm_settings_reliability_counts_and_built_status() {
    let testing_doc = read_repo_file("docs/testing.md");
    let required_fragments = [
        "just test",
        "just linux",
        "just lint-targets",
        "just xterm",
        "just xterm-tmux",
        "just orca",
        "just iterm2",
        "just windows",
        "tuikit-macos",
        "/Applications/iTerm.app",
        "--vnc-experimental",
        "30",
        "10",
    ];
    assert_contains_all(&testing_doc, "docs/testing.md", &required_fragments);
    let status_section = extract_markdown_h2_section(&testing_doc, "Status").to_ascii_lowercase();
    assert_lacks_all(
        &status_section,
        "docs/testing.md ## Status",
        &["to be built", "not set up", "throwaway"],
    );
    let built_pieces = [
        "rig/probe",
        "rig/xterm",
        "rig/tmux",
        "rig/iterm2",
        "rig/linux",
        "rig/expected",
    ];
    assert_contains_all(&status_section, "docs/testing.md ## Status", &built_pieces);
}

#[test]
fn architecture_and_decisions_satisfy_m0_exit_criteria() {
    let architecture_doc = read_repo_file("docs/architecture.md");
    let layout_section = extract_markdown_h2_section(&architecture_doc, "The layout");
    assert_contains_all(
        &layout_section,
        "docs/architecture.md layout",
        &[".github/", "justfile", "rig/", "scripts/"],
    );

    let decisions_doc = read_repo_file("docs/decisions.md");
    let required_labels = [
        "**Decision.**",
        "**Why.**",
        "**Consequences.**",
        "**Revisit when.**",
    ];
    for decision_heading in [
        "The capability probe lives in the test rig until avandar-tui-term exists",
        "tmux 3.3 is tested in the Linux floor container",
    ] {
        let section_body = extract_markdown_h2_section(&decisions_doc, decision_heading);
        assert_contains_all(
            &section_body,
            &format!("decision '{decision_heading}'"),
            &required_labels,
        );
    }
}

#[test]
fn readme_documents_rig_commands_and_hides_private_apps() {
    let readme_doc = read_repo_file("README.md");
    let developing_section = extract_markdown_h2_section(&readme_doc, "Developing");
    let required_commands = [
        "just check",
        "just lint-targets",
        "just linux",
        "just xterm",
        "just xterm-tmux",
        "just orca",
        "just iterm2",
        "just windows",
        "docs/testing.md",
    ];
    assert_contains_all(
        &developing_section,
        "README.md ## Developing",
        &required_commands,
    );
    assert_lacks_all(
        &readme_doc.to_ascii_lowercase(),
        "README.md",
        &["brain", "huckleberry-cli", "jpsyx"],
    );
}

#[test]
fn justfile_recipes_and_expected_baselines_are_complete() {
    let workspace_root = repository_root();
    let recipes = parse_justfile_recipe_bodies(&read_repo_file("justfile"));
    for layer_recipe in [
        "test",
        "linux",
        "lint-targets",
        "xterm",
        "xterm-tmux",
        "orca",
        "iterm2",
        "windows",
    ] {
        let body_lines = recipes
            .get(layer_recipe)
            .unwrap_or_else(|| panic!("justfile missing recipe '{layer_recipe}'"));
        assert_eq!(
            body_lines.len(),
            1,
            "justfile recipe '{layer_recipe}' must be a single command"
        );
    }

    for baseline_file in EXPECTED_BASELINE_FILES {
        let baseline_path = workspace_root.join("rig/expected").join(baseline_file);
        assert!(
            baseline_path.is_file(),
            "missing {}",
            baseline_path.display()
        );
    }
}

#[test]
fn rig_sleep_ban_and_touched_docs_em_dash_and_local_path_bans_hold() {
    let workspace_root = repository_root();
    let sleep_word = concat!("sl", "eep");
    let mut rig_files = Vec::new();
    for relative_target in RIG_SLEEP_SCAN_TARGETS {
        collect_rig_source_files(&workspace_root.join(relative_target), &mut rig_files);
    }
    rig_files.sort();
    assert!(!rig_files.is_empty());
    for file_path in rig_files {
        let file_contents = fs::read_to_string(&file_path).unwrap_or_else(|read_error| {
            panic!("failed to read {}: {read_error}", file_path.display())
        });
        assert!(
            !has_standalone_ascii_word(&file_contents, sleep_word),
            "{} contains forbidden synchronization word",
            file_path.display()
        );
    }

    let local_users_prefix = concat!("/", "Users", "/");
    for relative_path in TOUCHED_FILES_WITHOUT_EM_DASH {
        let contents = read_repo_file(relative_path);
        assert!(
            !contents.contains('\u{2014}'),
            "{relative_path} contains an em dash"
        );
        assert!(
            !contents.contains(local_users_prefix),
            "{relative_path} leaks a local path"
        );
    }
}

#[test]
fn extract_markdown_h2_section_handles_crlf_and_ignores_h3_headings() {
    let crlf_markdown = "# Title\r\n\r\n### Target\r\ndecoy\r\n## Target\r\nbody\r\n### Sub\r\nsub\r\n## Next\r\ntail\r\n";
    assert_eq!(
        extract_markdown_h2_section(crlf_markdown, "Target"),
        "body\n### Sub\nsub"
    );
}
