//! EPUB backend: an e-reader-friendly book from the shared model.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ZipLibrary};
use minijinja::{context, Environment};

use crate::model::BookView;

const STYLE: &str = include_str!("../../templates/epub/style.css");
const FRONT: &str = include_str!("../../templates/epub/front.xhtml");
const CHAPTER: &str = include_str!("../../templates/epub/chapter.xhtml");
const GALLERY: &str = include_str!("../../templates/epub/gallery.xhtml");

fn mime_for(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

/// Registered under `.html` names so minijinja auto-escapes interpolated text; the
/// template bodies themselves are XHTML.
fn env() -> Result<Environment<'static>> {
    let mut env = Environment::new();
    env.add_template("front.html", FRONT)
        .context("front template")?;
    env.add_template("chapter.html", CHAPTER)
        .context("chapter template")?;
    env.add_template("gallery.html", GALLERY)
        .context("gallery template")?;
    Ok(env)
}

/// Build `out_dir/book.epub` from the model, embedding image resources from `out_dir`.
pub fn build(book: &BookView, out_dir: &Path, open_after: bool) -> Result<PathBuf> {
    let env = env()?;
    let mut builder = EpubBuilder::new(ZipLibrary::new().map_err(|e| anyhow::anyhow!("{e}"))?)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    builder.epub_version(EpubVersion::V30);
    builder
        .metadata("title", &book.title)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(author) = &book.author {
        builder
            .metadata("author", author)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    builder
        .add_resource("style.css", STYLE.as_bytes(), "text/css")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Cover image.
    if let Some(cover) = &book.cover {
        if let Ok(bytes) = std::fs::read(out_dir.join(cover)) {
            builder
                .add_cover_image(cover.as_str(), &bytes[..], mime_for(cover))
                .map_err(|e| anyhow::anyhow!("adding cover: {e}"))?;
        }
    }

    // Embed every referenced image (and gallery images) as a resource once.
    let mut seen = HashSet::new();
    for src in book
        .chapters
        .iter()
        .flat_map(|c| &c.days)
        .flat_map(|d| &d.messages)
        .flat_map(|m| &m.attachments)
        .filter_map(|a| a.src.as_ref())
        .chain(book.gallery.iter())
    {
        if !seen.insert(src.clone()) {
            continue;
        }
        match std::fs::read(out_dir.join(src)) {
            Ok(bytes) => builder
                .add_resource(src.as_str(), &bytes[..], mime_for(src))
                .map_err(|e| anyhow::anyhow!("adding resource {src}: {e}"))?,
            Err(e) => {
                eprintln!("  (skipping EPUB image {src}: {e})");
                &mut builder
            }
        };
    }

    // Front matter (title + stats).
    let front = env
        .get_template("front.html")?
        .render(context! { book => book })
        .context("rendering EPUB front matter")?;
    builder
        .add_content(EpubContent::new("front.xhtml", front.as_bytes()).title(book.title.clone()))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // One content document per month chapter.
    let chapter_tmpl = env.get_template("chapter.html")?;
    for (i, chapter) in book.chapters.iter().enumerate() {
        let xhtml = chapter_tmpl
            .render(context! { chapter => chapter, is_group => book.is_group })
            .with_context(|| format!("rendering EPUB chapter {}", chapter.title))?;
        builder
            .add_content(
                EpubContent::new(format!("chap-{i}.xhtml"), xhtml.as_bytes())
                    .title(chapter.title.clone())
                    .level(1),
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    // Optional photo gallery appendix.
    if !book.gallery.is_empty() {
        let xhtml = env
            .get_template("gallery.html")?
            .render(context! { images => &book.gallery })
            .context("rendering EPUB gallery")?;
        builder
            .add_content(
                EpubContent::new("gallery.xhtml", xhtml.as_bytes())
                    .title("Photos")
                    .level(1),
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    let out = out_dir.join("book.epub");
    let file =
        std::fs::File::create(&out).with_context(|| format!("creating {}", out.display()))?;
    builder
        .generate(file)
        .map_err(|e| anyhow::anyhow!("generating EPUB: {e}"))?;

    println!("Wrote {}", out.display());
    if open_after {
        if let Err(e) = open::that(&out) {
            eprintln!("(could not open EPUB automatically: {e})");
        }
    }
    Ok(out)
}
