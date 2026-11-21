//! Milestone 0 exit-invariant tests across documentation, `justfile`, `rig/expected`, and `rig/`.

use std::collections::BTreeMap;
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
}

fn extract_markdown_h2_section<'a>(markdown: &'a str, heading: &str) -> &'a str {
    let heading_line = format!("## {heading}\n");
    let start_offset = markdown
        .find(&heading_line)
        .unwrap_or_else(|| panic!("missing heading '## {heading}'"))
        + heading_line.len();
    let remainder = &markdown[start_offset..];
    remainder
        .find("\n## ")
        .map_or(remainder, |end_offset| &remainder[..end_offset])
}

fn has_standalone_ascii_word(haystack: &str, target_word: &str) -> bool {
    let lowered_haystack = haystack.to_ascii_lowercase();
    let lowered_target = target_word.to_ascii_lowercase();
    lowered_haystack
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == lowered_target)
}

fn collect_rig_source_files(target_path: &Path, collected_paths: &mut Vec<PathBuf>) {
    if target_path.is_file() {
        let is_skipped_file = target_path
            .file_name()
            .and_then(|os_name| os_name.to_str())
            .is_some_and(|file_name| {
                file_name == ".DS_Store"
                    || Path::new(file_name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("pyc"))
            });
        if !is_skipped_file {
            collected_paths.push(target_path.to_path_buf());
        }
        return;
    }
    let directory_entries = fs::read_dir(target_path).unwrap_or_else(|read_error| {
        panic!("failed to list {}: {read_error}", target_path.display())
    });
    for entry_result in directory_entries {
        let entry_path = entry_result.expect("directory entry").path();
        let is_skipped_dir = entry_path
            .file_name()
            .and_then(|os_name| os_name.to_str())
            .is_some_and(|dir_name| matches!(dir_name, "node_modules" | "__pycache__"));
        if !is_skipped_dir {
            collect_rig_source_files(&entry_path, collected_paths);
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
            if let Some(ref recipe_name) = active_recipe {
                recipes
                    .entry(recipe_name.clone())
                    .or_default()
                    .push(line.trim().to_owned());
            }
            continue;
        }
        if let Some((header_left, _header_right)) = line.split_once(':') {
            let recipe_name = header_left
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_owned();
            active_recipe = Some(recipe_name.clone());
            recipes.entry(recipe_name).or_default();
        } else {
            active_recipe = None;
        }
    }
    recipes
}

#[test]
fn terminals_doc_has_zero_verify_markers_and_records_conpty_and_tmux_3_3() {
    let terminals_doc = read_repo_file("docs/terminals.md");
    assert!(
        !has_standalone_ascii_word(&terminals_doc, "verify"),
        "docs/terminals.md still contains standalone 'Verify'/'verify'"
    );
    for required_fragment in [
        "ConPTY",
        "tmux 3.3",
        "tmux 3.8",
        "tmux -CC",
        "cmd.exe /D /S /C",
        "rig/expected",
    ] {
        assert!(
            terminals_doc.contains(required_fragment),
            "docs/terminals.md missing '{required_fragment}'"
        );
    }
}

#[test]
fn testing_doc_covers_all_layers_vm_settings_reliability_counts_and_built_status() {
    let testing_doc = read_repo_file("docs/testing.md");
    for required_fragment in [
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
    ] {
        assert!(
            testing_doc.contains(required_fragment),
            "docs/testing.md missing '{required_fragment}'"
        );
    }
    let status_section = extract_markdown_h2_section(&testing_doc, "Status").to_ascii_lowercase();
    for stale_phrase in ["to be built", "not set up", "throwaway"] {
        assert!(
            !status_section.contains(stale_phrase),
            "docs/testing.md ## Status still contains '{stale_phrase}'"
        );
    }
    for built_piece in [
        "rig/probe",
        "rig/xterm",
        "rig/tmux",
        "rig/iterm2",
        "rig/linux",
        "rig/expected",
    ] {
        assert!(
            status_section.contains(built_piece),
            "docs/testing.md ## Status missing '{built_piece}'"
        );
    }
}

#[test]
fn architecture_and_decisions_satisfy_m0_exit_criteria() {
    let architecture_doc = read_repo_file("docs/architecture.md");
    let layout_section = extract_markdown_h2_section(&architecture_doc, "The layout");
    for required_entry in [".github/", "justfile", "rig/", "scripts/"] {
        assert!(
            layout_section.contains(required_entry),
            "docs/architecture.md layout missing '{required_entry}'"
        );
    }

    let decisions_doc = read_repo_file("docs/decisions.md");
    for decision_heading in [
        "The capability probe lives in the test rig until avandar-tui-term exists",
        "tmux 3.3 is tested in the Linux floor container",
    ] {
        let section_body = extract_markdown_h2_section(&decisions_doc, decision_heading);
        for required_label in [
            "**Decision.**",
            "**Why.**",
            "**Consequences.**",
            "**Revisit when.**",
        ] {
            assert!(
                section_body.contains(required_label),
                "decision '{decision_heading}' missing '{required_label}'"
            );
        }
    }
}

#[test]
fn readme_documents_rig_commands_and_hides_private_apps() {
    let readme_doc = read_repo_file("README.md");
    let developing_section = extract_markdown_h2_section(&readme_doc, "Developing");
    for required_command in [
        "just check",
        "just lint-targets",
        "just linux",
        "just xterm",
        "just xterm-tmux",
        "just orca",
        "just iterm2",
        "just windows",
        "docs/testing.md",
    ] {
        assert!(
            developing_section.contains(required_command),
            "README.md ## Developing missing '{required_command}'"
        );
    }
    for forbidden_private_name in ["brain", "huckleberry-cli", "jpsyx"] {
        assert!(
            !readme_doc
                .to_ascii_lowercase()
                .contains(forbidden_private_name),
            "README.md leaked private app reference '{forbidden_private_name}'"
        );
    }
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
        assert!(
            workspace_root
                .join("rig/expected")
                .join(baseline_file)
                .is_file(),
            "missing baseline rig/expected/{baseline_file}"
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
        let file_contents = fs::read_to_string(&file_path).expect("UTF-8 rig source file");
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
