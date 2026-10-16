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
fn utf8_bom_is_removed_without_rewriting_body() {
    assert_eq!(
        preprocess("\u{feff}private _x=42;\n", &[]).unwrap(),
        "private _x=42;\n"
    );
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
