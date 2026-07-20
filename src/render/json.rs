//! JSON backend: the whole [`BookView`](crate::model::BookView) as one document, for
//! archival and interop with other tools. Every field of the model is already
//! `Serialize`, so this is a faithful dump — chapters, days, messages, attachments,
//! tapbacks, and the computed stats.

use anyhow::{Context, Result};

use crate::model::BookView;

/// Serialize the book to pretty-printed JSON.
pub fn render(book: &BookView) -> Result<String> {
    serde_json::to_string_pretty(book).context("serializing book to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_title_and_chapters() {
        let mut book = BookView {
            title: "Hi".to_string(),
            ..Default::default()
        };
        book.message_count = 0;
        let json = render(&book).unwrap();
        assert!(json.contains("\"title\": \"Hi\""));
        assert!(json.contains("\"chapters\": []"));
    }
}
