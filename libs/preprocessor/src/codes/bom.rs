use hemtt_workspace::{
    WorkspacePath,
    reporting::{Code, Diagnostic, Label},
};

/// Encoding policy shared by CLI preprocessing and unsaved editor analysis.
pub struct Bom {
    pub file: WorkspacePath,
}

impl Code for Bom {
    fn ident(&self) -> &'static str {
        "BOM"
    }

    fn message(&self) -> String {
        "UTF-8 BOM is disallowed by encoding policy".to_string()
    }

    fn help(&self) -> Option<String> {
        Some("Save as UTF-8 without BOM; native acceptance varies by loading path".to_string())
    }

    fn diagnostic(&self) -> Option<Diagnostic> {
        Some(Diagnostic::new(self.ident(), self.message()).with_label(
            Label::primary(self.file.clone(), 0..3).with_message("save as utf-8 without bom"),
        ))
    }
}
