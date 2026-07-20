//! LaTeX text escaping and emoji handling.

use std::sync::LazyLock;

use regex::Regex;

/// Runs of emoji/pictographic characters, wrapped in the template's `\emojifont`.
static EMOJI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\p{Extended_Pictographic}+)").expect("valid emoji regex"));

/// Escape arbitrary message text for insertion into LaTeX, then wrap emoji runs in
/// `{\emojifont …}` so they render with the emoji font declared in the preamble.
///
/// The literal backslash must be handled before any replacement that introduces
/// backslashes. This mirrors the original tool's behavior but compiles the emoji
/// regex once (via `LazyLock`) instead of on every call.
pub fn latex_escape(text: &str) -> String {
    let escaped = text
        // Normalize a few "smart" characters to plain ASCII first.
        .replace('\u{2019}', "'")
        .replace(['\u{201C}', '\u{201D}'], "\"")
        .replace('\u{2026}', "...")
        // Literal backslash first, before we start emitting our own backslashes.
        .replace('\\', r"\textbackslash ")
        .replace('$', r"\$")
        .replace('%', r"\%")
        .replace('&', r"\&")
        .replace('_', r"\_")
        .replace('^', r"\textasciicircum ")
        .replace('~', r"\textasciitilde ")
        .replace('#', r"\#")
        .replace('{', r"\{")
        .replace('}', r"\}")
        // A single newline is not a line break in LaTeX; make it explicit.
        .replace('\n', "\\newline\n")
        // The emoji "variation selector" isn't supported by the emoji font.
        .replace('\u{FE0F}', "");

    EMOJI_RE
        .replace_all(&escaped, "{\\emojifont $1}")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_special_chars() {
        assert_eq!(latex_escape("100% & $5"), r"100\% \& \$5");
    }

    #[test]
    fn backslash_before_introduced_escapes() {
        // A literal backslash must not double-escape the escapes we add.
        assert_eq!(latex_escape(r"a\b"), r"a\textbackslash b");
    }

    #[test]
    fn wraps_emoji() {
        let out = latex_escape("hi 😀");
        assert!(out.contains(r"{\emojifont 😀}"), "got: {out}");
    }

    #[test]
    fn newline_becomes_explicit_break() {
        assert!(latex_escape("a\nb").contains(r"\newline"));
    }
}
