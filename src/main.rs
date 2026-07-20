//! imessage-book CLI entry point. The logic lives in the `imessage_book` library crate.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use imessage_database::tables::chat::Chat;
use imessage_database::util::dates::get_offset;

use imessage_book::attachments::{self, Processor};
use imessage_book::cli::{Cli, Command, DbLocation};
use imessage_book::config::Config;
use imessage_book::model::BookView;
use imessage_book::{assemble, build, db, preview, render};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config;
    match cli.command {
        Command::Init { force } => init_config(force),
        Command::ListChats { db, json } => list_chats(&db, json, config_path.as_deref()),
        Command::Preview {
            recipient,
            db,
            subset,
            attach,
            output_dir,
            port,
        } => {
            let out_abs = prepare_output_dir(&output_dir)?;
            let opts = attach.to_options();
            let book = load_book(
                &db,
                &recipient,
                &subset.to_subset()?,
                &opts,
                &out_abs,
                config_path.as_deref(),
            )?;
            let html = render::html::render(&book)?;
            preview::write_index(&out_abs, &html)?;
            println!(
                "Rendered {} messages. Starting preview…",
                book.message_count
            );
            preview::serve(&out_abs, port)
        }
        Command::Epub {
            recipient,
            db,
            subset,
            attach,
            output_dir,
            open,
        } => {
            let out_abs = prepare_output_dir(&output_dir)?;
            let opts = attach.to_options();
            let book = load_book(
                &db,
                &recipient,
                &subset.to_subset()?,
                &opts,
                &out_abs,
                config_path.as_deref(),
            )?;
            println!("Rendered {} messages. Writing EPUB…", book.message_count);
            render::epub::build(&book, &out_abs, open)?;
            Ok(())
        }
        Command::Build {
            recipient,
            db,
            subset,
            attach,
            output_dir,
            engine,
            open,
        } => {
            let out_abs = prepare_output_dir(&output_dir)?;
            let opts = attach.to_options();
            let book = load_book(
                &db,
                &recipient,
                &subset.to_subset()?,
                &opts,
                &out_abs,
                config_path.as_deref(),
            )?;
            let latex = render::latex::render(&book)?;
            build::write_source(&out_abs, &latex)?;
            println!("Rendered {} messages. Building PDF…", book.message_count);
            build::build_pdf(&out_abs, engine.into(), open)?;
            Ok(())
        }
        Command::Json {
            recipient,
            db,
            subset,
            attach,
            output_dir,
            open,
        } => {
            let out_abs = prepare_output_dir(&output_dir)?;
            let opts = attach.to_options();
            let book = load_book(
                &db,
                &recipient,
                &subset.to_subset()?,
                &opts,
                &out_abs,
                config_path.as_deref(),
            )?;
            println!("Rendered {} messages. Writing JSON…", book.message_count);
            let json = render::json::render(&book)?;
            write_text_output(&out_abs, "book.json", &json, open)
        }
        Command::Markdown {
            recipient,
            db,
            subset,
            attach,
            output_dir,
            open,
        } => {
            let out_abs = prepare_output_dir(&output_dir)?;
            let opts = attach.to_options();
            let book = load_book(
                &db,
                &recipient,
                &subset.to_subset()?,
                &opts,
                &out_abs,
                config_path.as_deref(),
            )?;
            println!(
                "Rendered {} messages. Writing Markdown…",
                book.message_count
            );
            let md = render::markdown::render(&book);
            write_text_output(&out_abs, "book.md", &md, open)
        }
    }
}

/// Write a starter `book.toml` in the current directory.
fn init_config(force: bool) -> Result<()> {
    let path = Path::new("book.toml");
    if path.exists() && !force {
        anyhow::bail!(
            "book.toml already exists here. Re-run `imessage-book init --force` to overwrite it."
        );
    }
    std::fs::write(path, imessage_book::config::STARTER_TOML).context("writing book.toml")?;
    println!("Wrote book.toml. Edit it, then run `imessage-book preview <recipient>`.");
    Ok(())
}

/// Write a text artifact to the output directory and optionally open it.
fn write_text_output(
    out_dir: &Path,
    filename: &str,
    contents: &str,
    open_after: bool,
) -> Result<()> {
    let path = out_dir.join(filename);
    std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    println!("Wrote {}", path.display());
    if open_after {
        if let Err(e) = open::that(&path) {
            eprintln!("(could not open {} automatically: {e})", path.display());
        }
    }
    Ok(())
}

/// Create the output directory and return its absolute path.
fn prepare_output_dir(output_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("creating output directory {}", output_dir.display()))?;
    std::fs::canonicalize(output_dir)
        .with_context(|| format!("resolving output directory {}", output_dir.display()))
}

fn list_chats(db_loc: &DbLocation, as_json: bool, config_path: Option<&Path>) -> Result<()> {
    let resolved = db_loc.resolve();
    let conn = db::open(&resolved.db_path)?;
    let summaries = db::query::chat_summaries(&conn)?;
    let offset = get_offset();

    // Resolve friendly names the same way the book does: `book.toml` names first, then the
    // macOS Contacts (AddressBook). A named group keeps its own DB display name.
    let config = Config::load(config_path)?;
    let resolver = db::contacts::ContactResolver::new(
        config.names.clone(),
        db::contacts::address_book_names(),
    );
    let friendly = |s: &db::query::ChatSummary| -> Option<String> {
        if let Some(name) = s.display_name.as_deref().filter(|n| !n.is_empty()) {
            return Some(name.to_string());
        }
        let name = resolver.display_name(&s.identifier);
        (name != s.identifier).then_some(name)
    };

    if as_json {
        // Machine-readable output for scripting and the GUI. `first`/`last` are ISO
        // dates (YYYY-MM-DD) or null; `identifier` is what you pass to the export
        // subcommands as the recipient; `display_name` is the resolved contact/group name.
        let rows: Vec<serde_json::Value> = summaries
            .iter()
            .map(|s| {
                serde_json::json!({
                    "identifier": s.identifier,
                    "display_name": friendly(s),
                    "count": s.count,
                    "first": fmt_date(s.first, offset),
                    "last": fmt_date(s.last, offset),
                    "is_group": s.is_group,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if summaries.is_empty() {
        println!("No conversations found in {}.", resolved.db_path.display());
        return Ok(());
    }
    println!(
        "Conversations in {} (most active first):\n",
        resolved.db_path.display()
    );
    for s in &summaries {
        let name = match friendly(s) {
            Some(n) => format!(" (\"{n}\")"),
            None => String::new(),
        };
        let span = match (fmt_date(s.first, offset), fmt_date(s.last, offset)) {
            (Some(a), Some(b)) if a == b => format!(", {a}"),
            (Some(a), Some(b)) => format!(", {a} to {b}"),
            _ => String::new(),
        };
        println!(
            "  {}{}  —  {} messages{}",
            s.identifier, name, s.count, span
        );
    }
    Ok(())
}

/// Format a raw Apple-epoch nanosecond timestamp as `YYYY-MM-DD`.
fn fmt_date(ns: Option<i64>, offset: i64) -> Option<String> {
    let ns = ns?;
    imessage_database::util::dates::get_local_time(&ns, &offset)
        .ok()
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// The common path: open the DB, find the conversation, and assemble the book.
fn load_book(
    db_loc: &DbLocation,
    recipient: &str,
    subset: &assemble::Subset,
    opts: &attachments::AttachOptions,
    out_abs: &Path,
    config_path: Option<&Path>,
) -> Result<BookView> {
    let resolved = db_loc.resolve();
    let conn = db::open(&resolved.db_path)?;

    let chats = db::query::resolve_chats(&conn, recipient)?;
    if chats.is_empty() {
        anyhow::bail!(
            "No conversation matching '{recipient}'. Run `imessage-book list-chats` to see \
             available conversations."
        );
    }
    for c in &chats {
        println!("Found conversation: {}", chat_label(c));
    }
    let chat_ids: Vec<i32> = chats.iter().map(|c| c.rowid).collect();
    // A conversation is a group when it has more than one other participant.
    let is_group = db::query::participant_count(&conn, &chat_ids)? > 1;

    let messages = db::query::messages_for_chats(&conn, &chat_ids)?;
    let handle_names = db::contacts::handle_map(&conn)?;
    let config = Config::load(config_path)?;
    // Names come from book.toml first, then the macOS AddressBook (best-effort).
    let resolver = db::contacts::ContactResolver::new(
        config.names.clone(),
        db::contacts::address_book_names(),
    );

    let default_title = chats
        .iter()
        .find_map(|c| c.display_name().map(str::to_string))
        .unwrap_or_else(|| recipient.to_string());

    let processor = Processor {
        platform: db::platform_for(resolved.is_ios),
        attachment_db_root: resolved.attachment_root.clone(),
        out_root: out_abs,
        opts,
    };

    let mut book = assemble::build_book(
        &conn,
        messages,
        get_offset(),
        is_group,
        &handle_names,
        &resolver,
        &config,
        &default_title,
        &processor,
        subset,
    )?;

    if let Some(cover) = &config.cover_image {
        book.cover = attachments::process_cover(Path::new(cover), out_abs);
    }
    Ok(book)
}

fn chat_label(c: &Chat) -> String {
    match c.display_name() {
        Some(name) if !name.is_empty() => format!("{} (\"{}\")", c.chat_identifier, name),
        _ => c.chat_identifier.clone(),
    }
}
