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
    assert!(parses("private _text='a\u{00a0}b\u{202f}€\u{0084}';"));
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

fn diagnostics(source: &str) -> hemtt_workspace::reporting::Codes {
    diagnostics_with_project(source, None)
}

#[test]
fn params_accepts_a_default_outside_the_input_type_filter() {
    for source in [
        r#"[] params [["_value", 0, [""]]]; if (_value isEqualType 0) then {};"#,
        r#"[] params [["_value", "", [[]]]]; if (_value isEqualType "") then {};"#,
        r#"[] params [["_value", objNull, [0]]]; isNull _value;"#,
    ] {
        let codes = diagnostics(source);
        assert!(!codes.iter().any(|c| c.ident() == "L-S12"), "{source}");
    }
    // The fallback type is still propagated into subsequent uses.
    assert!(
        diagnostics(r#"params [["_value", "sentinel", [0]]]; _value + true;"#)
            .iter()
            .any(|c| c.ident() == "L-S12")
    );
    // Malformed parameter schemas are still diagnosed.
    assert!(
        diagnostics(r#"params [["_value", 0, "wrong schema"]];"#)
            .iter()
            .any(|c| c.ident() == "L-S12")
    );
}

#[test]
fn registering_callbacks_does_not_execute_their_global_assignments() {
    let source = r#"
        G_stop = {
            if (!isNil "G_first") then {
                removeMissionEventHandler ["Ended", G_first]; G_first = nil;
            };
            if (!isNil "G_second") then {
                removeMissionEventHandler ["MPEnded", G_second]; G_second = nil;
            };
        };
        G_first = addMissionEventHandler ["Ended", {call G_stop}];
        G_second = addMissionEventHandler ["MPEnded", {call G_stop}];
        removeMissionEventHandler ["Ended", G_first];
    "#;
    assert!(!diagnostics(source).iter().any(|c| c.ident() == "L-S12"));
    // Invalid code inside a callback must still be checked.
    assert!(
        diagnostics(r#"addMissionEventHandler ["Ended", {player setFuel true}];"#)
            .iter()
            .any(|c| c.ident() == "L-S12")
    );
}

fn diagnostics_with_project(
    source: &str,
    project: Option<&hemtt_common::config::ProjectConfig>,
) -> hemtt_workspace::reporting::Codes {
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
    let database = std::sync::Arc::new(Database::a3(false));
    let sqf = parser::run(&database, &processed).unwrap();
    hemtt_sqf::analyze::analyze(
        &sqf,
        project,
        &processed,
        std::sync::Arc::new(hemtt_workspace::addons::Addon::test_addon()),
        database,
        None,
    )
    .0
}

#[test]
fn format_runtime_compatibility_preserves_real_findings() {
    use hemtt_workspace::reporting::Severity;
    assert!(
        !diagnostics("format [\"%2\", \"unused\", \"two\"]; ")
            .iter()
            .any(|c| c.ident() == "L-S08")
    );
    assert!(
        diagnostics("format [\"%1\", 1, 2];")
            .iter()
            .any(|c| c.ident() == "L-S08" && c.severity() == Severity::Warning)
    );
    assert!(
        diagnostics("format [\"%2\", 1];")
            .iter()
            .any(|c| c.ident() == "L-S08" && c.severity() == Severity::Error)
    );
    assert!(
        diagnostics("format [\"LIKE '%abc%' id=%1\", 7];")
            .iter()
            .any(|c| c.ident() == "L-S08" && c.severity() == Severity::Error)
    );
}

#[test]
fn public_variable_handler_warning_is_overload_specific() {
    use hemtt_workspace::reporting::Severity;
    assert!(
        diagnostics("\"probe\" addPublicVariableEventHandler {};")
            .iter()
            .any(|c| c.ident() == "L-S09"
                && c.severity() == Severity::Warning
                && c.message().contains("ordinary String/Code"))
    );
    assert!(
        diagnostics("\"probe\" addPublicVariableEventHandler [objNull, {}];")
            .iter()
            .any(|c| c.ident() == "L-S09" && c.severity() == Severity::Error)
    );
    let project = hemtt_common::config::ProjectConfig::from_file(&std::path::PathBuf::from(
        "tests/lints/native_explicit_ban.toml",
    ))
    .unwrap();
    assert!(
        diagnostics_with_project(
            "\"probe\" addPublicVariableEventHandler {};",
            Some(&project)
        )
        .iter()
        .any(|c| c.ident() == "L-S09"
            && c.severity() == Severity::Error
            && c.message().contains("banned by the project"))
    );
}
