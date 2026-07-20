//! HTML backend: a Messages-style page for instant preview.

use anyhow::{Context, Result};
use minijinja::{context, Environment, Value};

use crate::model::BookView;

/// The template is embedded so the binary is self-contained.
const PAGE: &str = include_str!("../../templates/html/page.html");

pub fn render(book: &BookView) -> Result<String> {
    let mut env = Environment::new();
    // Registered under a `.html` name so minijinja auto-escapes message text.
    env.add_template("page.html", PAGE)
        .context("loading HTML template")?;
    let tmpl = env
        .get_template("page.html")
        .context("getting HTML template")?;

    // Serialize the stats to JSON for the client-side charts, so the template stays a
    // presentation layer and all the numbers live in one place.
    let stats_json = match &book.stats {
        Some(stats) => {
            escape_for_script(&serde_json::to_string(stats).context("serializing stats")?)
        }
        None => "null".to_string(),
    };

    let ctx = context! {
        stats_json,
        ..Value::from_serialize(book)
    };
    tmpl.render(ctx).context("rendering HTML")
}

/// Escape a JSON blob so it can be inlined inside a `<script>` element without a string
/// value such as `</script>` prematurely closing it. These characters only appear inside
/// JSON string literals, so escaping them everywhere keeps the document valid JSON.
fn escape_for_script(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}
