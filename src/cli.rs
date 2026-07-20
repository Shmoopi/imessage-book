//! Command-line interface.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};
use imessage_database::tables::table::DEFAULT_PATH_IOS;
use imessage_database::util::dirs::default_db_path;

use crate::assemble::Subset;
use crate::attachments::{AttachMode, AttachOptions};
use crate::build::Engine;

#[derive(Parser)]
#[command(author, version, about = "Turn an iMessage conversation into a book.")]
pub struct Cli {
    /// Path to a `book.toml` config. Defaults to `book.toml` in the current directory.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Write a commented starter `book.toml` in the current directory.
    Init {
        /// Overwrite an existing `book.toml`.
        #[arg(long)]
        force: bool,
    },
    /// List conversations in the database (to discover recipients / group names).
    ListChats {
        #[command(flatten)]
        db: DbLocation,
        /// Emit the conversation list as JSON (for scripting and the GUI).
        #[arg(long)]
        json: bool,
    },
    /// Render an HTML preview and open it in the browser (no LaTeX required).
    Preview {
        /// Phone number, email, or group name to export.
        recipient: String,
        #[command(flatten)]
        db: DbLocation,
        #[command(flatten)]
        subset: SubsetArgs,
        #[command(flatten)]
        attach: AttachArgs,
        /// Directory for generated files.
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,
        /// Port for the local preview server.
        #[arg(long, default_value_t = 8000)]
        port: u16,
    },
    /// Render an EPUB book for e-readers.
    Epub {
        /// Phone number, email, or group name to export.
        recipient: String,
        #[command(flatten)]
        db: DbLocation,
        #[command(flatten)]
        subset: SubsetArgs,
        #[command(flatten)]
        attach: AttachArgs,
        /// Directory for generated files.
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,
        /// Open the EPUB when it's written.
        #[arg(long)]
        open: bool,
    },
    /// Export the assembled conversation as a single JSON document.
    Json {
        /// Phone number, email, or group name to export.
        recipient: String,
        #[command(flatten)]
        db: DbLocation,
        #[command(flatten)]
        subset: SubsetArgs,
        #[command(flatten)]
        attach: AttachArgs,
        /// Directory for generated files.
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,
        /// Open the JSON file when it's written.
        #[arg(long)]
        open: bool,
    },
    /// Export the conversation as a Markdown document.
    #[command(alias = "md")]
    Markdown {
        /// Phone number, email, or group name to export.
        recipient: String,
        #[command(flatten)]
        db: DbLocation,
        #[command(flatten)]
        subset: SubsetArgs,
        #[command(flatten)]
        attach: AttachArgs,
        /// Directory for generated files.
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,
        /// Open the Markdown file when it's written.
        #[arg(long)]
        open: bool,
    },
    /// Render LaTeX and build a PDF book.
    Build {
        /// Phone number, email, or group name to export.
        recipient: String,
        #[command(flatten)]
        db: DbLocation,
        #[command(flatten)]
        subset: SubsetArgs,
        #[command(flatten)]
        attach: AttachArgs,
        /// Directory for generated files.
        #[arg(short, long, default_value = "output")]
        output_dir: PathBuf,
        /// LaTeX engine to use.
        #[arg(long, value_enum, default_value_t = EngineArg::Auto)]
        engine: EngineArg,
        /// Open the PDF when the build finishes.
        #[arg(long)]
        open: bool,
    },
}

#[derive(Args)]
#[group(required = false, multiple = false)]
pub struct DbLocation {
    /// Path to the root of an iOS backup folder.
    #[arg(short, long)]
    pub ios_backup_dir: Option<PathBuf>,
    /// Path to a chat database directly. Defaults to the standard macOS location.
    #[arg(short, long)]
    pub chat_database: Option<PathBuf>,
}

/// Resolved database location.
pub struct ResolvedDb {
    /// Path to the SQLite database file.
    pub db_path: PathBuf,
    /// Whether this came from an iOS backup (affects attachment path resolution).
    pub is_ios: bool,
    /// Root passed to `resolved_attachment_path`: backup root for iOS, db path otherwise.
    pub attachment_root: PathBuf,
}

impl DbLocation {
    pub fn resolve(&self) -> ResolvedDb {
        match (&self.ios_backup_dir, &self.chat_database) {
            (Some(ios_dir), None) => ResolvedDb {
                db_path: ios_dir.join(DEFAULT_PATH_IOS),
                is_ios: true,
                attachment_root: ios_dir.clone(),
            },
            (None, Some(db)) => ResolvedDb {
                db_path: db.clone(),
                is_ios: false,
                attachment_root: db.clone(),
            },
            _ => {
                let db = default_db_path();
                ResolvedDb {
                    attachment_root: db.clone(),
                    db_path: db,
                    is_ios: false,
                }
            }
        }
    }
}

#[derive(Args)]
pub struct SubsetArgs {
    /// Render at most this many messages.
    #[arg(long)]
    pub limit: Option<usize>,
    /// Only include messages on or after this date (YYYY-MM-DD).
    #[arg(long)]
    pub from: Option<String>,
    /// Only include messages on or before this date (YYYY-MM-DD).
    #[arg(long)]
    pub to: Option<String>,
    /// Evenly sample this many messages across the range (for quick previews).
    #[arg(long)]
    pub sample: Option<usize>,
}

impl SubsetArgs {
    pub fn to_subset(&self) -> Result<Subset> {
        let parse = |s: &Option<String>| -> Result<Option<NaiveDate>> {
            match s {
                Some(v) => Ok(Some(
                    NaiveDate::parse_from_str(v, "%Y-%m-%d")
                        .with_context(|| format!("parsing date '{v}' (expected YYYY-MM-DD)"))?,
                )),
                None => Ok(None),
            }
        };
        Ok(Subset {
            limit: self.limit,
            from: parse(&self.from)?,
            to: parse(&self.to)?,
            sample: self.sample,
        })
    }
}

#[derive(Args)]
pub struct AttachArgs {
    /// How to handle attachments.
    #[arg(long, value_enum, default_value_t = AttachModeArg::Media)]
    pub attachments: AttachModeArg,
    /// Download offloaded attachments from iCloud (macOS only; hits the network).
    #[arg(long)]
    pub download_from_icloud: bool,
    /// Skip embedding attachments larger than this many megabytes.
    #[arg(long)]
    pub max_attachment_mb: Option<u64>,
}

impl AttachArgs {
    pub fn to_options(&self) -> AttachOptions {
        AttachOptions {
            mode: match self.attachments {
                AttachModeArg::None => AttachMode::None,
                AttachModeArg::Media => AttachMode::Media,
            },
            download_icloud: self.download_from_icloud,
            max_bytes: self.max_attachment_mb.map(|mb| mb * 1024 * 1024),
            subdir: "attachments".to_string(),
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum AttachModeArg {
    /// Never embed; render labeled placeholders.
    None,
    /// Embed images and video poster frames.
    Media,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum EngineArg {
    Auto,
    Tectonic,
    System,
}

impl From<EngineArg> for Engine {
    fn from(e: EngineArg) -> Self {
        match e {
            EngineArg::Auto => Engine::Auto,
            EngineArg::Tectonic => Engine::Tectonic,
            EngineArg::System => Engine::System,
        }
    }
}
