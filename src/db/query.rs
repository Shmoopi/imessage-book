//! Chat lookup and the message iterator.

use std::collections::HashSet;
use std::rc::Rc;

use anyhow::{Context, Result};
use imessage_database::tables::{
    chat::Chat,
    messages::Message,
    table::{
        Table, CHAT, CHAT_HANDLE_JOIN, CHAT_MESSAGE_JOIN, MESSAGE, MESSAGE_ATTACHMENT_JOIN,
        RECENTLY_DELETED,
    },
};
use rusqlite::{types::Value, Connection};

/// All chats in the database (used by `list-chats`).
pub fn all_chats(db: &Connection) -> Result<Vec<Chat>> {
    let mut stmt = Chat::get(db).map_err(|e| anyhow::anyhow!("{e}"))?;
    let chats = stmt
        .query_map([], Chat::from_row)
        .context("querying chats")?
        .filter_map(|c| c.ok())
        .collect();
    Ok(chats)
}

/// A summary row for `list-chats`: how many messages and the date span.
pub struct ChatSummary {
    pub identifier: String,
    pub display_name: Option<String>,
    pub count: i64,
    /// First/last message dates as raw Apple nanosecond timestamps.
    pub first: Option<i64>,
    pub last: Option<i64>,
    /// True when the chat has more than one other participant (a group chat). Lets callers
    /// label 1:1 conversations by contact name without mistaking them for groups.
    pub is_group: bool,
}

/// Per-chat message counts and date ranges, most active first.
pub fn chat_summaries(db: &Connection) -> Result<Vec<ChatSummary>> {
    let mut stmt = db
        .prepare(&format!(
            "SELECT c.chat_identifier, c.display_name, COUNT(m.ROWID), MIN(m.date), MAX(m.date), \
                    (SELECT COUNT(*) FROM {CHAT_HANDLE_JOIN} chj WHERE chj.chat_id = c.ROWID) > 1
             FROM {CHAT} c
             LEFT JOIN {CHAT_MESSAGE_JOIN} j ON j.chat_id = c.ROWID
             LEFT JOIN {MESSAGE} m ON m.ROWID = j.message_id
             GROUP BY c.ROWID
             ORDER BY COUNT(m.ROWID) DESC, c.chat_identifier",
        ))
        .context("preparing chat summary query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ChatSummary {
                identifier: row.get(0)?,
                display_name: row.get(1).unwrap_or(None),
                count: row.get(2)?,
                first: row.get(3).unwrap_or(None),
                last: row.get(4).unwrap_or(None),
                is_group: row.get(5).unwrap_or(false),
            })
        })
        .context("querying chat summaries")?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

/// Resolve a user-supplied recipient to the chats to export.
///
/// Prefers an **exact** (case-insensitive) match on the chat identifier — this
/// intentionally returns every chat row sharing that identifier so a contact's
/// iMessage and SMS threads are merged. Only if there is no exact match does it fall
/// back to a substring / display-name search, and if that is ambiguous (matches more
/// than one distinct identifier) it errors with the candidates rather than silently
/// merging unrelated conversations.
pub fn resolve_chats(db: &Connection, needle: &str) -> Result<Vec<Chat>> {
    let needle_lc = needle.to_lowercase();
    let mut exact: Vec<Chat> = Vec::new();
    let mut fuzzy: Vec<Chat> = Vec::new();

    for c in all_chats(db)? {
        if c.chat_identifier.eq_ignore_ascii_case(needle) {
            exact.push(c);
            continue;
        }
        let id_match = c.chat_identifier.to_lowercase().contains(&needle_lc);
        let name_match = c
            .display_name()
            .map(|d| d.to_lowercase().contains(&needle_lc))
            .unwrap_or(false);
        if id_match || name_match {
            fuzzy.push(c);
        }
    }

    if !exact.is_empty() {
        return Ok(exact);
    }

    let mut identifiers: Vec<String> = fuzzy.iter().map(|c| c.chat_identifier.clone()).collect();
    identifiers.sort();
    identifiers.dedup();
    if identifiers.len() > 1 {
        anyhow::bail!(
            "'{needle}' matches multiple conversations: {}. \
             Re-run with a more specific recipient (an exact identifier from `list-chats`).",
            identifiers.join(", ")
        );
    }
    Ok(fuzzy)
}

/// Number of distinct participants (handles) across the given chats. Used to decide
/// whether a conversation is a group (more than one other participant).
pub fn participant_count(db: &Connection, chat_ids: &[i32]) -> Result<usize> {
    let mut stmt = db
        .prepare(&format!(
            "SELECT COUNT(DISTINCT handle_id) FROM {CHAT_HANDLE_JOIN} WHERE chat_id IN rarray(?1)"
        ))
        .context("preparing participant query")?;
    let id_values = Rc::new(
        chat_ids
            .iter()
            .copied()
            .map(Value::from)
            .collect::<Vec<Value>>(),
    );
    let count: i64 = stmt
        .query_row([id_values], |r| r.get(0))
        .context("counting participants")?;
    Ok(count.max(0) as usize)
}

/// All messages belonging to the given chat rowids, in chronological order, with
/// duplicates (a message linked to more than one matched chat) removed.
///
/// The SQL mirrors `Message::get` from `imessage-database` but adds a
/// `WHERE c.chat_id IN (...)` filter, since the library's built-in query has no
/// way to scope to a chat. Uses rusqlite's `rarray` to bind the id list.
pub fn messages_for_chats(db: &Connection, chat_ids: &[i32]) -> Result<Vec<Message>> {
    let mut stmt = db
        .prepare(&format!(
            "SELECT
                 *,
                 c.chat_id,
                 (SELECT COUNT(*) FROM {MESSAGE_ATTACHMENT_JOIN} a WHERE m.ROWID = a.message_id) as num_attachments,
                 (SELECT b.chat_id FROM {RECENTLY_DELETED} b WHERE m.ROWID = b.message_id) as deleted_from,
                 (SELECT COUNT(*) FROM {MESSAGE} m2 WHERE m2.thread_originator_guid = m.guid) as num_replies
             FROM
                 message as m
                 LEFT JOIN {CHAT_MESSAGE_JOIN} as c ON m.ROWID = c.message_id
             WHERE
                 c.chat_id IN rarray(?1)
             ORDER BY
                 m.date;",
        ))
        .context("preparing message query")?;

    let id_values = Rc::new(
        chat_ids
            .iter()
            .copied()
            .map(Value::from)
            .collect::<Vec<Value>>(),
    );
    let mut seen = HashSet::new();
    let messages = stmt
        .query_map([id_values], Message::from_row)
        .context("querying messages")?
        .filter_map(|m| m.ok())
        // A message joined to two matched chats appears twice; keep the first.
        .filter(|m| seen.insert(m.rowid))
        .collect();
    Ok(messages)
}
