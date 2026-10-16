//! Editor-only recovery of selected top-level statements after a full parse
//! fails. Boundaries are balanced lexically; the existing config parser still
//! parses every selected declaration. This is never validation or rapification.
use chumsky::Parser;

pub fn selected(source: &str, names: &[String]) -> Result<crate::Config, String> {
    let mut projected = String::new();
    let mut start = 0;
    let mut stack = Vec::new();
    let mut quote = None;
    for (offset, ch) in source.char_indices() {
        if let Some(current) = quote {
            if ch == current {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '{' => stack.push(ch),
            '[' | '(' if !stack.contains(&'{') => stack.push(ch),
            '}' | ']' | ')' => {
                // Config math inside a class can be an opaque expression,
                // with parentheses unsupported by the full parser. Only its
                // braces affect top-level declaration boundaries. Selected
                // bodies still go through the existing config parser below.
                if ch != '}' && stack.contains(&'{') {
                    continue;
                }
                let expected = match ch {
                    '}' => '{',
                    ']' => '[',
                    _ => '(',
                };
                if stack.pop() != Some(expected) {
                    let context: String = source[..offset]
                        .chars()
                        .rev()
                        .take(100)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    return Err(format!(
                        "Unbalanced config boundaries at byte {offset} near {context:?}; selected structure unavailable"
                    ));
                }
            }
            ';' if stack.is_empty() => {
                let statement = &source[start..=offset];
                if selects(statement, names) {
                    projected.push_str(statement);
                    projected.push('\n');
                }
                start = offset + 1;
            }
            _ => {}
        }
    }
    if quote.is_some() || !stack.is_empty() || !source[start..].trim().is_empty() {
        return Err("Incomplete config boundaries; selected structure unavailable".into());
    }
    let (config, errors) = crate::parse::config().parse_recovery(projected.as_str());
    if !errors.is_empty() {
        return Err(format!(
            "Selected config structure has parser errors: {errors:?}"
        ));
    }
    config.ok_or_else(|| "Selected config structure unavailable".into())
}

fn selects(statement: &str, names: &[String]) -> bool {
    let text = statement.trim_start();
    let Some((keyword, rest)) = text.split_once(char::is_whitespace) else {
        return false;
    };
    if !keyword.eq_ignore_ascii_case("class") && !keyword.eq_ignore_ascii_case("delete") {
        return false;
    }
    let name: String = rest
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    names
        .iter()
        .any(|selected| selected.eq_ignore_ascii_case(&name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn projects_only_registered_top_level_statements_with_existing_parser() {
        let source = "bad = {unsupported}; class Other { text=\"class Wanted {;};\"; class Wanted {}; }; class Wanted; class Wanted { tag=\"one\"; }; delete Wanted; class Wanted { tag=\"two\"; };";
        let config = super::selected(source, &["Wanted".into()]).expect("balanced projection");
        assert_eq!(config.0.len(), 4);
        assert!(matches!(config.0[2], crate::Property::Delete(_)));
        assert!(
            super::selected(
                "class Other { size=(1 + 2)); }; class Wanted {};",
                &["Wanted".into()]
            )
            .is_ok()
        );
        assert!(
            super::selected("class Other {};", &["Wanted".into()])
                .expect("empty selection")
                .0
                .is_empty()
        );
        for broken in [
            "class Wanted {",
            "class Wanted { tag=; };",
            "bad={]; class Wanted {};",
            "bad=\"unterminated; class Wanted {};",
        ] {
            assert!(
                super::selected(broken, &["Wanted".into()]).is_err(),
                "{broken}"
            );
        }
    }
}
