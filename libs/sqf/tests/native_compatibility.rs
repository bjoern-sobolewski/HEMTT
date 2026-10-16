#![allow(clippy::unwrap_used)]

use hemtt_common::config::{PDriveOption, PreprocessorOptions};
use hemtt_preprocessor::Processor;
use hemtt_sqf::parser::{self, database::Database};
use hemtt_workspace::Workspace;

fn parses(source: &str) -> bool {
    let workspace = Workspace::builder()
        .memory()
        .finish(None, false, &PDriveOption::Disallow)
        .unwrap();
    let file = workspace.join("source.sqf").unwrap();
    file.create_file()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let processed = Processor::run(&file, &PreprocessorOptions::default()).unwrap();
    parser::run(&Database::a3(false), &processed).is_ok()
}

#[test]
fn native_compact_tokens_parse() {
    for source in [
        "isNil\"_insertId\";",
        "for\"_i\"from 1 to 2 do {};",
        "\"'\"in _this;",
    ] {
        assert!(parses(source), "{source}");
    }
}

#[test]
fn comma_statements_do_not_change_array_separators() {
    assert!(parses("private _a=1, private _b=2; [_a,_b];"));
    assert!(parses("private _a=[{private _x=1, _x},2];"));
    assert!(!parses("private _a=[1 2];"));
    assert!(!parses("private _a=;"));
}

#[test]
fn native_encoded_function_sources_parse() {
    assert!(parses(
        "\u{feff}private _text='a\u{00a0}b\u{202f}€\u{0084}';"
    ));
    assert!(parses("private _x=42; /* unfinished"));
}

#[test]
fn native_browser_command_is_available_without_project_metadata() {
    assert!(parses(
        "private _value=controlNull ctrlWebBrowserAction [\"Deflate\",\"hello\"];"
    ));
}

#[test]
fn documented_damage_alias_uses_binary_signature() {
    assert!(parses("player setDammage 0.9;"));
    assert!(!Database::a3(false).has_nular_command("setDammage"));
}
