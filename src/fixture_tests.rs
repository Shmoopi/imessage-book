//! End-to-end test of the DB → assemble → `BookView` path against a synthetic SQLite
//! database that mimics the iMessage schema. Lives inside the crate (not `tests/`)
//! because it needs `rusqlite` and the internal `db`/`assemble` APIs.

use imessage_database::util::dates::get_offset;
use imessage_database::util::platform::Platform;
use rusqlite::Connection;

use crate::assemble::{build_book, Subset};
use crate::attachments::{AttachMode, AttachOptions, Processor};
use crate::config::Config;
use crate::db;
use crate::db::contacts::ContactResolver;
use std::collections::HashMap;

// 2020-11-02 12:00:00 UTC expressed in Apple's nanoseconds-since-2001 epoch.
const D1: i64 = 626_011_200_000_000_000;
const HOUR_NS: i64 = 3_600_000_000_000;

fn write_fixture(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "
        CREATE TABLE handle (rowid INTEGER PRIMARY KEY, id TEXT, person_centric_id TEXT);
        CREATE TABLE chat (rowid INTEGER PRIMARY KEY, chat_identifier TEXT, service_name TEXT, display_name TEXT);
        CREATE TABLE message (
            rowid INTEGER PRIMARY KEY, guid TEXT, text TEXT, service TEXT, handle_id INTEGER,
            subject TEXT, date INTEGER, date_read INTEGER, date_delivered INTEGER,
            is_from_me INTEGER, is_read INTEGER, item_type INTEGER, group_title TEXT,
            group_action_type INTEGER, associated_message_guid TEXT, associated_message_type INTEGER,
            balloon_bundle_id TEXT, expressive_send_style_id TEXT, thread_originator_guid TEXT,
            thread_originator_part TEXT, date_edited INTEGER
        );
        CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);
        CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
        CREATE TABLE message_attachment_join (message_id INTEGER, attachment_id INTEGER);
        CREATE TABLE chat_recoverable_message_join (chat_id INTEGER, message_id INTEGER);

        INSERT INTO handle (rowid, id, person_centric_id) VALUES (1, '+15551234567', NULL);
        INSERT INTO chat (rowid, chat_identifier, service_name, display_name)
            VALUES (1, '+15551234567', 'iMessage', NULL);
        INSERT INTO chat_handle_join (chat_id, handle_id) VALUES (1, 1);
        ",
    )
    .unwrap();

    // m1: incoming normal message.
    conn.execute(
        "INSERT INTO message (rowid, guid, text, service, handle_id, date, is_from_me, is_read, item_type, group_action_type, date_edited)
         VALUES (1, 'G1', 'hey there', 'iMessage', 1, ?1, 0, 1, 0, 0, 0)",
        [D1],
    )
    .unwrap();
    // m2: outgoing reply to m1.
    conn.execute(
        "INSERT INTO message (rowid, guid, text, service, handle_id, date, is_from_me, is_read, item_type, group_action_type, date_edited, thread_originator_guid, thread_originator_part)
         VALUES (2, 'G2', 'reply body', 'iMessage', NULL, ?1, 1, 1, 0, 0, 0, 'G1', '0:0:0')",
        [D1 + HOUR_NS],
    )
    .unwrap();
    // m3: a "Loved" tapback on m1 (should attach as a tapback, not render as a message).
    conn.execute(
        "INSERT INTO message (rowid, guid, service, handle_id, date, is_from_me, is_read, item_type, group_action_type, date_edited, associated_message_guid, associated_message_type)
         VALUES (3, 'G3', 'iMessage', 1, ?1, 0, 1, 0, 0, 0, 'p:0/G1', 2000)",
        [D1 + HOUR_NS + 60_000_000_000],
    )
    .unwrap();

    conn.execute_batch(
        "INSERT INTO chat_message_join (chat_id, message_id) VALUES (1,1),(1,2),(1,3);",
    )
    .unwrap();
}

#[test]
fn end_to_end_from_fixture_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("chat.db");
    write_fixture(&db_path);

    let conn = db::open(&db_path).unwrap();

    let chats = db::query::resolve_chats(&conn, "+15551234567").unwrap();
    assert_eq!(chats.len(), 1, "should resolve exactly the one chat");
    let chat_ids: Vec<i32> = chats.iter().map(|c| c.rowid).collect();

    assert_eq!(db::query::participant_count(&conn, &chat_ids).unwrap(), 1);
    let is_group = db::query::participant_count(&conn, &chat_ids).unwrap() > 1;
    assert!(!is_group);

    let messages = db::query::messages_for_chats(&conn, &chat_ids).unwrap();
    assert_eq!(
        messages.len(),
        3,
        "all three rows load (reaction filtered later)"
    );

    let handle_names = db::contacts::handle_map(&conn).unwrap();
    let opts = AttachOptions {
        mode: AttachMode::None,
        ..AttachOptions::default()
    };
    let processor = Processor {
        platform: Platform::macOS,
        attachment_db_root: db_path.clone(),
        out_root: dir.path(),
        opts: &opts,
    };

    let resolver = ContactResolver::new(HashMap::new(), HashMap::new());
    let book = build_book(
        &conn,
        messages,
        get_offset(),
        is_group,
        &handle_names,
        &resolver,
        &Config::default(),
        "+15551234567",
        &processor,
        &Subset::default(),
    )
    .unwrap();

    // The reaction is filtered out, leaving the two real messages.
    assert_eq!(book.message_count, 2);
    assert_eq!(book.chapters.len(), 1);
    assert!(book.chapters[0].title.contains("November"));
    assert!(book.chapters[0].title.contains("2020"));

    let msgs = &book.chapters[0].days[0].messages;
    assert_eq!(msgs.len(), 2);

    // m1 carries the Loved tapback; senders are absent in a 1:1 chat.
    assert!(msgs[0].tapbacks.iter().any(|t| t.contains("Loved")));
    assert!(msgs[0].sender.is_none());

    // m2 is a reply whose parent preview resolved to m1's text.
    assert!(msgs[1].reply);
    assert_eq!(msgs[1].reply_to.as_deref(), Some("hey there"));
    assert!(msgs[1].from_me);
}
