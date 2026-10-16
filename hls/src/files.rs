use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use ropey::Rope;
use url::Url;

use crate::{TextDocumentItem, TextInformation};

#[derive(Clone)]
pub struct FileCache {
    ropes: Arc<DashMap<Url, Rope>>,
}

impl FileCache {
    pub fn get() -> Self {
        static SINGLETON: LazyLock<FileCache> = LazyLock::new(|| FileCache {
            ropes: Arc::new(DashMap::new()),
        });
        (*SINGLETON).clone()
    }

    pub fn text(&self, url: &Url) -> Option<String> {
        self.ropes.get(url).map(|r| r.value().to_string())
    }

    pub fn insert(&self, url: Url, text: Rope) {
        self.ropes.insert(url, text);
    }

    pub fn entry(&'_ self, url: Url) -> dashmap::Entry<'_, Url, Rope> {
        self.ropes.entry(url)
    }

    pub fn is_open(&self, url: &Url) -> bool {
        self.ropes.contains_key(url)
    }

    #[allow(clippy::unused_self)]
    pub fn on_change(&self, document: &TextDocumentItem<'_>) {
        match &document.text {
            TextInformation::Full(text) => {
                Self::get().insert(document.uri.clone(), Rope::from_str(text));
            }
            TextInformation::Changes(changes) => {
                let file_cache = Self::get();
                let mut rope = file_cache
                    .entry(document.uri.clone())
                    .or_insert_with(|| Rope::from_str(""));
                for change in changes {
                    if let Some(range) = change.range {
                        let start = rope.line_to_char(range.start.line as usize)
                            + utf16_to_char(
                                &rope.line(range.start.line as usize).to_string(),
                                range.start.character as usize,
                            );
                        let end = rope.line_to_char(range.end.line as usize)
                            + utf16_to_char(
                                &rope.line(range.end.line as usize).to_string(),
                                range.end.character as usize,
                            );
                        rope.remove(start..end);
                        rope.insert(start, change.text.as_str());
                    } else {
                        *rope = Rope::from_str(&change.text);
                    }
                }
            }
        }
    }

    pub fn on_close(&self, url: &Url) {
        self.ropes.remove(url);
    }
}

fn utf16_to_char(line: &str, offset: usize) -> usize {
    let mut units = 0;
    line.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= offset
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

    #[test]
    fn edits_use_utf16_and_accept_full_replacements() {
        let cache = FileCache::get();
        let uri = Url::parse("file:///unicode.sqf").expect("url");
        cache.on_change(&TextDocumentItem {
            uri: uri.clone(),
            text: TextInformation::Full("\"😀\"; x = 1;"),
            version: Some(1),
        });
        cache.on_change(&TextDocumentItem {
            uri: uri.clone(),
            text: TextInformation::Changes(vec![TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 6), Position::new(0, 7))),
                range_length: None,
                text: "y".into(),
            }]),
            version: Some(2),
        });
        assert_eq!(cache.text(&uri).as_deref(), Some("\"😀\"; y = 1;"));
        cache.on_change(&TextDocumentItem {
            uri: uri.clone(),
            text: TextInformation::Changes(vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "z = 2;".into(),
            }]),
            version: Some(3),
        });
        assert_eq!(cache.text(&uri).as_deref(), Some("z = 2;"));
        cache.on_close(&uri);
    }
}
