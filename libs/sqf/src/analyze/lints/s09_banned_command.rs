use std::{ops::Range, sync::Arc};

use hemtt_common::config::{LintConfig, ProjectConfig};
use hemtt_workspace::{lint::{AnyLintRunner, Lint, LintRunner}, reporting::{Code, Codes, Diagnostic, Processed, Severity}};

use crate::{analyze::LintData, Expression};

crate::analyze::lint!(LintS09BannedCommand);

impl Lint<LintData> for LintS09BannedCommand {
    fn ident(&self) -> &'static str {
        "banned_commands"
    }

    fn sort(&self) -> u32 {
        90
    }

    fn description(&self) -> &'static str {
        "Checks for broken or banned commands."
    }

    fn documentation(&self) -> &'static str {
r#"### Configuration

- **banned**: Additional commands to check for
- **ignore**: An array of commands to ignore

```toml
[lints.sqf.banned_commands]
options.banned = [
    "execVM",
]
options.ignore = [
    "echo",
]
```

### Example

**Incorrect**
```sqf
echo "Hello World"; // Doesn't exist in the retail game
```

### Explanation

Checks for usage of broken or banned commands."#
    }

    fn default_config(&self) -> LintConfig {
        LintConfig::error()
    }

    fn minimum_severity(&self) -> Severity {
        Severity::Warning
    }

    fn runners(&self) -> Vec<Box<dyn AnyLintRunner<LintData>>> {
        vec![Box::new(Runner)]
    }
}

struct Runner;
impl LintRunner<LintData> for Runner {
    type Target = Expression;
    
    fn run(
        &self,
        _project: Option<&ProjectConfig>,
        config: &LintConfig,
        processed: Option<&Processed>,
        _runtime: &hemtt_common::config::RuntimeArguments,
        target: &Self::Target,
        data: &LintData,
    ) -> Codes {
        let Some(processed) = processed else {
            return Vec::new();
        };
        let Some(command) = target.command_name() else {
            return Vec::new();
        };
        if let Some(toml::Value::Array(ignore)) = config.option("ignore")
            && ignore.iter().any(|i| i.as_str().map(str::to_lowercase) == Some(command.to_lowercase())) {
                return Vec::new();
            }
        let Some(wiki) = data.database.wiki().commands().get(command) else {
            return Vec::new();
        };
        let explicitly_banned = matches!(config.option("banned"), Some(toml::Value::Array(banned))
            if banned.iter().any(|i| i.as_str().is_some_and(|s| s.eq_ignore_ascii_case(command))));
        // The wiki's Broken Commands group applies only to this command's
        // alternative [target, code] overload. The ordinary String/Code form
        // works in multiplayer. A project ban still applies to either overload.
        let ordinary_public_variable_handler = command.eq_ignore_ascii_case("addPublicVariableEventHandler")
            && matches!(target, Expression::BinaryCommand(_, _, rhs, _) if matches!(rhs.as_ref(), Expression::Code(..)));
        if !explicitly_banned && ordinary_public_variable_handler {
            return vec![Arc::new(CodeS09BannedCommand::new(
                target.span(), command.to_string(), processed, Severity::Warning, true, true,
            ))];
        }
        if !explicitly_banned && wiki.groups().contains(&String::from("Broken Commands")) {
            return vec![Arc::new(CodeS09BannedCommand::new(
                target.span(),
                command.to_string(),
                processed,
                config.severity(),
                true,
                false,
            ))];
        }
        if let Some(toml::Value::Array(banned)) = config.option("banned")
            && banned.iter().any(|i| i.as_str().map(str::to_lowercase) == Some(command.to_lowercase())) {
                return vec![Arc::new(CodeS09BannedCommand::new(
                    target.span(),
                    command.to_string(),
                    processed,
                    config.severity(),
                    false,
                    false,
                ))];
            }
        Vec::new()
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct CodeS09BannedCommand {
    span: Range<usize>,
    command: String,
    from_wiki: bool,
    ordinary_public_variable_handler: bool,

    severity: Severity,
    diagnostic: Option<Diagnostic>,
}

impl Code for CodeS09BannedCommand {
    fn ident(&self) -> &'static str {
        "L-S09"
    }

    fn link(&self) -> Option<&str> {
        Some("/lints/sqf.html#banned_commands")
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn message(&self) -> String {
        if self.ordinary_public_variable_handler {
            "`addPublicVariableEventHandler` is deprecated; its ordinary String/Code form works in multiplayer. Only the alternative [target, code] syntax is marked broken.".to_string()
        } else if self.from_wiki {
            format!("`{}` is marked as a broken command on the wiki", self.command)
        } else {
            format!("`{}` is banned by the project", self.command)
        }
    }

    fn label_message(&self) -> String {
        if self.ordinary_public_variable_handler {
            "deprecated command".to_string()
        } else if self.from_wiki {
            "broken command".to_string()
        } else {
            "banned command".to_string()
        }
    }

    fn diagnostic(&self) -> Option<Diagnostic> {
        self.diagnostic.clone()
    }
}

impl CodeS09BannedCommand {
    #[must_use]
    pub fn new(span: Range<usize>, command: String, processed: &Processed, severity: Severity, from_wiki: bool, ordinary_public_variable_handler: bool) -> Self {
        Self {
            span,
            command,

            severity,
            diagnostic: None,
            from_wiki,
            ordinary_public_variable_handler,
        }
        .generate_processed(processed)
    }

    fn generate_processed(mut self, processed: &Processed) -> Self {
        self.diagnostic = Diagnostic::from_code_processed(&self, self.span.clone(), processed);
        self
    }
}
