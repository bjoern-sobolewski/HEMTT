//! Ordered config structure for editor consumers. Registration policy stays in
//! the consumer; preprocessing and parsing use the same dirty source database.
use hemtt_config::{Class, Property, Value};
use hemtt_preprocessor::Processor;
use serde_json::{Value as Json, json};
use url::Url;

use crate::{sources::SourceSync, workspace::EditorWorkspaces};

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

pub async fn structure(url: Url) -> Json {
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
    let Ok(processed) =
        Processor::run_with_sources(&source, options, &SourceSync::get().database())
    else {
        return json!({"properties": [], "limitations": ["Config preprocessing failed; registrations unresolved. Inspect diagnostics."]});
    };
    match hemtt_config::parse(project.as_ref(), &processed) {
        Ok(report) if report.errors().is_empty() => json!({
            "properties": properties(&report.config().0), "limitations": [],
            "includes": processed.included_files().iter().map(|p| workspace.to_url(p)).collect::<Vec<_>>(),
        }),
        _ => {
            json!({"properties": [], "limitations": ["Config parsing failed; registrations unresolved. Inspect diagnostics."]})
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
