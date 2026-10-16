#![allow(clippy::unwrap_used)]

use hemtt_common::config::{PDriveOption, PreprocessorOptions};
use hemtt_preprocessor::Processor;
use hemtt_workspace::Workspace;

fn preprocess(source: &str, files: &[(&str, &str)]) -> Result<String, String> {
    let workspace = Workspace::builder()
        .memory()
        .finish(None, false, &PDriveOption::Disallow)
        .unwrap();
    for (name, content) in files {
        let file = workspace.join(name).unwrap();
        if !file.parent().exists().unwrap() {
            file.parent().create_dir().unwrap();
        }
        file.create_file()
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();
    }
    let file = workspace.join("source.sqf").unwrap();
    file.create_file()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    Processor::run(&file, &PreprocessorOptions::default())
        .map(|processed| processed.as_str().to_owned())
        .map_err(|error| {
            error
                .1
                .get_code()
                .unwrap()
                .diagnostic()
                .unwrap()
                .to_string(&hemtt_workspace::reporting::WorkspaceFiles::new())
        })
}

#[test]
fn native_unicode_strings_preserve_bytes() {
    for quote in ['\'', '"'] {
        let source =
            format!("private _text = {quote}a\u{00a0}b\u{202f}€\u{0084}\u{00ad}\u{e000}{quote};\n");
        assert_eq!(preprocess(&source, &[]).unwrap(), source);
    }
}

#[test]
fn utf8_bom_is_rejected_by_encoding_policy() {
    let error = preprocess("\u{feff}private _x=42;\n", &[]).unwrap_err();
    assert!(error.contains("BOM"), "{error}");
    assert!(error.contains("encoding policy"), "{error}");
    assert_eq!(
        preprocess("private _x=42;\n", &[]).unwrap(),
        "private _x=42;\n"
    );
}

#[test]
fn included_config_bom_is_rejected_without_rejecting_unicode_strings() {
    let error = preprocess(
        "#include \"header.hpp\"\n",
        &[("header.hpp", "\u{feff}#define VALUE 1\n")],
    )
    .unwrap_err();
    assert!(error.contains("BOM"), "{error}");
    assert!(error.contains("header.hpp"), "{error}");
    assert_eq!(
        preprocess("private _s='a\u{feff}b';\n", &[]).unwrap(),
        "private _s='a\u{feff}b';\n"
    );
}

#[test]
fn failed_include_remains_a_dependency_for_editor_repair() {
    let workspace = Workspace::builder()
        .memory()
        .finish(None, false, &PDriveOption::Disallow)
        .unwrap();
    let root = workspace.join("source.sqf").unwrap();
    root.create_file()
        .unwrap()
        .write_all(b"#include \"header.hpp\"\nVALUE\n")
        .unwrap();
    let header = workspace.join("header.hpp").unwrap();
    header
        .create_file()
        .unwrap()
        .write_all(b"#define VALUE 1\n")
        .unwrap();
    let sources = hemtt_workspace::SourceDatabase::new();
    let id = sources.file_id(&header);
    sources.set_overlay(id, "\u{feff}#define VALUE 1\n", 1);
    let (attempted, error) =
        Processor::run_with_sources(&root, &PreprocessorOptions::default(), &sources)
            .err()
            .unwrap();
    assert_eq!(error.get_code().unwrap().ident(), "BOM");
    assert!(attempted.contains(&header));
    sources.set_overlay(id, "#define VALUE 2\n", 2);
    let repaired =
        Processor::run_with_sources(&root, &PreprocessorOptions::default(), &sources).unwrap();
    assert!(repaired.as_str().contains('2'));
}

#[test]
fn open_trailing_comment_is_discarded() {
    assert_eq!(
        preprocess("private _x=42; /* unfinished\nignored", &[]).unwrap(),
        "private _x=42; "
    );
    // An opening comment delimiter inside either quote style remains data.
    for source in [
        "private _s='/*';\n",
        "private _s=\"/*\";\n",
        "private _s='it''s /*';\n",
    ] {
        assert_eq!(preprocess(source, &[]).unwrap(), source);
    }
}

#[test]
fn has_include_uses_actual_workspace_for_both_branches() {
    for (path, files, expected) in [
        ("present.hpp", vec![("present.hpp", "")], "true"),
        ("missing.hpp", vec![], "false"),
        (
            "\\life_shared\\lyl_dev",
            vec![("life_shared/lyl_dev", "")],
            "true",
        ),
    ] {
        let source = format!("#if __has_include ( \"{path}\" )\ntrue\n#else\nfalse\n#endif\n");
        assert_eq!(preprocess(&source, &files).unwrap().trim(), expected);
    }
}

#[test]
fn has_include_rejects_malformed_and_unsupported_expressions() {
    for expression in [
        "__has_include(\"missing\"",
        "__has_include(missing)",
        "__has_include(\"missing\") || 1",
    ] {
        assert!(preprocess(&format!("#if {expression}\ntrue\n#endif\n"), &[]).is_err());
    }
}
