#![allow(clippy::unwrap_used)]

use std::{fs, io::Read as _};

use clap::Parser;
use sealed_test::prelude::*;

use hemtt::Cli;

#[sealed_test]
fn build_simple() {
    let _directory = hemtt_test::directory::TemporaryDirectory::copy(&std::path::PathBuf::from(
        format!("{}/tests/workspace_simple/", env!("CARGO_MANIFEST_DIR")),
    ));
    hemtt::execute(&Cli::parse_from(vec!["hemtt", "dev"])).unwrap();
    hemtt::execute(&Cli::parse_from(vec!["hemtt", "build"])).unwrap();
}

#[sealed_test]
fn build_preserves_exec_config_and_checks_properties_after_it() {
    let _directory = hemtt_test::directory::TemporaryDirectory::copy(&std::path::PathBuf::from(
        format!("{}/tests/workspace_simple/", env!("CARGO_MANIFEST_DIR")),
    ));
    let config_path = "addons/main/description.ext";
    let header_path = "addons/main/runtime.hpp";
    let config = "#include \"runtime.hpp\"\nclass Settings { value = __EVAL(ArmaValue); };\n";
    let header = "__EXEC(ArmaValue = [1, (2 + 3)];);\n";
    fs::write(config_path, config).unwrap();
    fs::write(header_path, header).unwrap();

    hemtt::execute(&Cli::parse_from(["hemtt", "build"])).unwrap();
    let mut pbo = hemtt_pbo::ReadablePbo::from(
        fs::File::open(".hemttout/build/addons/test_main.pbo").unwrap(),
    )
    .unwrap();
    for (name, original) in [("description.ext", config), ("runtime.hpp", header)] {
        let mut bytes = Vec::new();
        pbo.file(name)
            .unwrap()
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, original.as_bytes());
    }

    fs::write(
        config_path,
        [config, "class Broken { value = ; };\n"].concat(),
    )
    .unwrap();
    hemtt::execute(&Cli::parse_from(["hemtt", "check"])).unwrap();
    let annotations = fs::read_to_string(".hemttout/ci_annotations.txt").unwrap();
    assert!(
        annotations.contains("||notice||__EXEC is deferred to Arma"),
        "{annotations}"
    );
    assert!(
        annotations
            .lines()
            .any(|line| line.contains("||error||") && line.ends_with("/description.ext")),
        "{annotations}"
    );
}

#[sealed_test]
fn build_post_release() {
    let _directory =
        hemtt_test::directory::TemporaryDirectory::copy(&std::path::PathBuf::from(format!(
            "{}/tests/workspace_post_release/",
            env!("CARGO_MANIFEST_DIR")
        )));
    hemtt::execute(&Cli::parse_from(vec!["hemtt", "script", "test"])).unwrap();
    hemtt::execute(&Cli::parse_from(vec!["hemtt", "release"])).unwrap();
}
