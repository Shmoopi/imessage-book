//! LaTeX backend: the printable book source.

use anyhow::{Context, Result};
use minijinja::syntax::SyntaxConfig;
use minijinja::Environment;

use super::escape::latex_escape;
use crate::model::BookView;

/// Embedded template. Uses `<< >>` / `<% %>` / `<# #>` delimiters so Jinja does not
/// collide with LaTeX's pervasive `{` / `}`.
const BOOK_TEX: &str = include_str!("../../templates/latex/book.tex");

pub fn render(book: &BookView) -> Result<String> {
    let mut env = Environment::new();

    let syntax = SyntaxConfig::builder()
        .block_delimiters("<%", "%>")
        .variable_delimiters("<<", ">>")
        .comment_delimiters("<#", "#>")
        .build()
        .context("configuring LaTeX template syntax")?;
    env.set_syntax(syntax);

    // `tex` escapes text and wraps emoji for the LaTeX font setup.
    env.add_filter("tex", |s: String| latex_escape(&s));

    env.add_template("book.tex", BOOK_TEX)
        .context("loading LaTeX template")?;
    let tmpl = env
        .get_template("book.tex")
        .context("getting LaTeX template")?;
    tmpl.render(book).context("rendering LaTeX")
}
