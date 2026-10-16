//! Ordered config structure for editor consumers. Registration policy stays in
//! the consumer; preprocessing and parsing use the same dirty source database.
use hemtt_config::{Class, Property, Value};
use hemtt_preprocessor::Processor;
use serde_json::{Value as Json, json};
use url::Url;

use crate::{sources::SourceSync, workspace::EditorWorkspaces};

#[derive(serde::Deserialize)]
pub struct StructureParams {
    pub url: Url,
    #[serde(default)]
    pub classes: Vec<String>,
}

fn properties(items: &[Property]) -> Vec<Json> {
    items
        .iter()
        .map(|item| match item {
            Property::Class(class) => json!({
                "kind": "class", "name": class.name().map(|n| n.as_str()),
                "parent": class.parent().map(|n| n.as_str()),
                "external": matches!(class, Class::External { .. }),
                "properties": properties(class.properties()),
            }),
            Property::Entry { name, value, .. } => json!({
                "kind": "entry", "name": name.as_str(),
                "literal": if let Value::Str(value) = value { Some(value.value()) } else { None },
            }),
            Property::Delete(name) => json!({"kind": "delete", "name": name.as_str()}),
            other => json!({"kind": "unresolved", "name": other.name().as_str()}),
        })
        .collect()
}

pub async fn structure(params: StructureParams) -> Json {
    let url = params.url;
    let Some(workspace) = EditorWorkspaces::get().guess_workspace_retry(&url).await else {
        return json!({"properties": [], "limitations": ["Config workspace unavailable"]});
    };
    let Ok(source) = workspace.join_url(&url) else {
        return json!({"properties": [], "limitations": ["Config source unavailable"]});
    };
    let project = workspace.config();
    let defaults = hemtt_common::config::PreprocessorOptions::default();
    let options = project.as_ref().map_or(&defaults, |p| p.preprocessor());
    // The requested root is intentional (e.g. description.ext), not its
    // synthetic addon's config.cpp. Includes still resolve through the VFS.
    let (processed, skipped) = match Processor::run_structure_with_sources(
        &source,
        options,
        &SourceSync::get().database(),
    ) {
        Ok(result) => result,
        Err((_, error)) => {
            let message = error.get_code().map_or_else(
                || error.to_string(),
                |code| {
                    format!(
                        "{}: {} at {:?}",
                        code.ident(),
                        code.message(),
                        code.token().map(|t| t.position())
                    )
                },
            );
            return json!({"properties": [], "limitations": [format!("Config preprocessing failed; registrations unresolved: {message}")]});
        }
    };
    match hemtt_config::parse(project.as_ref(), &processed) {
        Ok(report) => json!({
            "properties": properties(&report.config().0), "limitations": [],
            "provisional": skipped.iter().map(|code| format!("{}: {} at {:?}; construct not executed or resolved; side effects and expression values unknown", code.ident(), code.message(), code.token().map(|t| t.position()))).chain(report.errors().iter().map(|code| format!("{}: {}", code.ident(), code.message()))).collect::<Vec<_>>(),
            "includes": processed.included_files().iter().map(|p| workspace.to_url(p)).collect::<Vec<_>>(),
        }),
        Err(codes) => {
            if !params.classes.is_empty() {
                return match hemtt_config::projection::selected(processed.as_str(), &params.classes)
                {
                    Ok(config) => json!({
                        "properties": properties(&config.0), "limitations": [],
                        "provisional": skipped.iter().map(|code| format!("{}: {}; construct not executed or resolved; expression values unknown", code.ident(), code.message())).chain(codes.iter().map(|code| format!("Full config parsing failed: {}: {}; only requested top-level classes projected; root validation and external inheritance unresolved", code.ident(), code.message()))).collect::<Vec<_>>(),
                        "includes": processed.included_files().iter().map(|p| workspace.to_url(p)).collect::<Vec<_>>(),
                    }),
                    Err(error) => json!({"properties": [], "limitations": [error]}),
                };
            }
            json!({"properties": [], "limitations": codes.iter().map(|code| format!("Config parsing failed; registrations unresolved: {}: {}", code.ident(), code.message())).collect::<Vec<_>>()})
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn retains_external_class_projection() {
        let config = hemtt_config::Class::External {
            name: hemtt_config::Ident::new("external".into(), 0..8),
        };
        let value = super::properties(&[hemtt_config::Property::Class(config)]);
        assert_eq!(value[0]["external"], true);
        assert_eq!(value[0]["name"], "external");
    }
}
