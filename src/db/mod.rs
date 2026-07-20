//! Database access: opening the iMessage SQLite DB and resolving platform.

pub mod contacts;
pub mod query;

use std::path::Path;

use anyhow::{Context, Result};
use imessage_database::tables::table::get_connection;
use imessage_database::util::platform::Platform;
use rusqlite::Connection;

/// Open the chat database read-only.
///
/// `get_connection` already emits a Full Disk Access hint in its error, which we
/// surface to the user via anyhow context.
pub fn open(db_path: &Path) -> Result<Connection> {
    if !db_path.exists() {
        anyhow::bail!(
            "No iMessage database found at {}.\n\
             On a Mac, grant your terminal Full Disk Access (System Settings > Privacy & \
             Security > Full Disk Access), or pass --ios-backup-dir / --chat-database.",
            db_path.display()
        );
    }
    let db = get_connection(db_path)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("opening iMessage database at {}", db_path.display()))?;
    // Register the `rarray` virtual table once per connection so the chat-id filter
    // queries can bind a list of ids.
    rusqlite::vtab::array::load_module(&db).context("loading sqlite rarray module")?;
    Ok(db)
}

/// Determine whether the DB comes from a live macOS install or an iOS backup.
///
/// The caller knows which flavor of path it built, so we take an explicit hint
/// rather than sniffing.
pub fn platform_for(is_ios_backup: bool) -> Platform {
    if is_ios_backup {
        Platform::iOS
    } else {
        Platform::macOS
    }
}
