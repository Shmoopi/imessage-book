//! Contact name resolution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use imessage_database::tables::{handle::Handle, table::Cacheable};
use rusqlite::{Connection, OpenFlags};

/// Map from `handle_id` to a raw contact identifier (phone number or email).
///
/// `Handle::cache` collapses duplicate handles for the same person and maps
/// handle id `0` to the special "Me" marker used in group chats.
pub fn handle_map(db: &Connection) -> Result<HashMap<i32, String>> {
    Handle::cache(db).map_err(|e| anyhow::anyhow!("building contact map: {e}"))
}

/// Normalize a raw handle identifier into a key for AddressBook lookups: emails are
/// lowercased; phone numbers are reduced to their last 10 digits so `+1 (555) 123-4567`
/// and `5551234567` match.
pub fn normalize(raw: &str) -> String {
    if raw.contains('@') {
        return raw.trim().to_lowercase();
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() > 10 {
        digits[digits.len() - 10..].to_string()
    } else {
        digits
    }
}

/// Best-effort map of normalized phone/email → contact display name, read from the
/// macOS AddressBook. Returns an empty map when Contacts data is unavailable (e.g. no
/// Full Disk Access, or on iOS backups).
pub fn address_book_names() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(home) = std::env::var_os("HOME") else {
        return map;
    };
    let root = Path::new(&home).join("Library/Application Support/AddressBook");
    for db_path in find_abcddb(&root) {
        if let Ok(conn) = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            read_contacts(&conn, &mut map);
        }
    }
    map
}

/// Recursively collect `*.abcddb` databases under `root`.
fn find_abcddb(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("abcddb") {
                out.push(path);
            }
        }
    }
    out
}

fn read_contacts(conn: &Connection, map: &mut HashMap<String, String>) {
    // Records store first/last/organization; phone and email rows point back via ZOWNER.
    let name = "TRIM(COALESCE(r.ZFIRSTNAME,'') || ' ' || COALESCE(r.ZLASTNAME,''))";
    let queries = [
        format!(
            "SELECT p.ZFULLNUMBER, {name}, r.ZORGANIZATION \
             FROM ZABCDPHONENUMBER p JOIN ZABCDRECORD r ON p.ZOWNER = r.Z_PK"
        ),
        format!(
            "SELECT e.ZADDRESS, {name}, r.ZORGANIZATION \
             FROM ZABCDEMAILADDRESS e JOIN ZABCDRECORD r ON e.ZOWNER = r.Z_PK"
        ),
    ];
    for query in queries {
        let Ok(mut stmt) = conn.prepare(&query) else {
            continue;
        };
        let rows = stmt.query_map([], |row| {
            let handle: Option<String> = row.get(0)?;
            let full_name: Option<String> = row.get(1)?;
            let org: Option<String> = row.get(2)?;
            Ok((handle, full_name, org))
        });
        let Ok(rows) = rows else { continue };
        for (handle, full_name, org) in rows.flatten() {
            let Some(handle) = handle else { continue };
            let display = match full_name {
                Some(n) if !n.trim().is_empty() => n.trim().to_string(),
                _ => match org {
                    Some(o) if !o.trim().is_empty() => o.trim().to_string(),
                    _ => continue,
                },
            };
            map.entry(normalize(&handle)).or_insert(display);
        }
    }
}

/// Resolves a raw handle identifier to a friendly display name, preferring an explicit
/// `book.toml` mapping, then the macOS AddressBook, then the raw identifier itself.
pub struct ContactResolver {
    toml: HashMap<String, String>,
    book: HashMap<String, String>,
}

impl ContactResolver {
    pub fn new(toml: HashMap<String, String>, book: HashMap<String, String>) -> Self {
        ContactResolver { toml, book }
    }

    pub fn display_name(&self, raw: &str) -> String {
        if let Some(name) = self.toml.get(raw) {
            return name.clone();
        }
        if let Some(name) = self.book.get(&normalize(raw)) {
            return name.clone();
        }
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_phones_and_emails() {
        assert_eq!(normalize("+1 (555) 123-4567"), "5551234567");
        assert_eq!(normalize("5551234567"), "5551234567");
        assert_eq!(normalize("Foo@Example.com "), "foo@example.com");
    }

    #[test]
    fn resolver_priority() {
        let mut toml = HashMap::new();
        toml.insert("+15551234567".to_string(), "Best Friend".to_string());
        let mut book = HashMap::new();
        book.insert("5559999999".to_string(), "Work Contact".to_string());
        let r = ContactResolver::new(toml, book);
        assert_eq!(r.display_name("+15551234567"), "Best Friend"); // toml wins
        assert_eq!(r.display_name("+1-555-999-9999"), "Work Contact"); // address book
        assert_eq!(r.display_name("+15550000000"), "+15550000000"); // fallback
    }
}
