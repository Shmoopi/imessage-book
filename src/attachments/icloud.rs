//! Detecting and materializing iCloud-offloaded (dataless) attachments.
//!
//! When "Messages in iCloud" is enabled with optimized storage, macOS evicts the
//! bytes of attachments under `~/Library/Messages/Attachments/`, leaving a *dataless*
//! placeholder: the path still resolves and reports a logical size, but the file's
//! `st_flags` carry the `SF_DATALESS` flag. We detect that and, when the user opts in,
//! ask the file provider to download the data with `brctl download` (falling back to
//! `fileproviderctl materialize`), then poll until the bytes arrive.

#[cfg(target_os = "macos")]
use std::os::macos::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// macOS BSD flag marking a file whose data has been evicted (e.g. to iCloud).
#[cfg(target_os = "macos")]
const SF_DATALESS: u32 = 0x4000_0000;

/// Whether the file's data has been evicted (macOS `SF_DATALESS`). On non-macOS
/// platforms — where the flag doesn't exist — nothing is ever considered evicted, so
/// iOS-backup exports still work; the sibling `.icloud` stub check in [`is_dataless`]
/// remains the portable fallback.
#[cfg(target_os = "macos")]
fn is_dataless_flag(m: &std::fs::Metadata) -> bool {
    (m.st_flags() & SF_DATALESS) != 0
}

#[cfg(not(target_os = "macos"))]
fn is_dataless_flag(_m: &std::fs::Metadata) -> bool {
    false
}

/// Whether `path` currently holds real bytes on disk.
///
/// Uses the `SF_DATALESS` flag rather than block count, because APFS stores small
/// files inline (zero allocated blocks) even when their data is present.
pub fn has_data(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(m) => m.len() > 0 && !is_dataless_flag(&m),
        Err(_) => false,
    }
}

/// The sibling `.name.icloud` stub macOS leaves for a fully-evicted file, if present.
fn icloud_stub(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let stub = path.with_file_name(format!(".{name}.icloud"));
    stub.exists().then_some(stub)
}

/// Is this attachment an offloaded iCloud placeholder rather than a real file?
pub fn is_dataless(path: &Path) -> bool {
    if let Ok(m) = std::fs::metadata(path) {
        if m.len() > 0 && is_dataless_flag(&m) {
            return true;
        }
    }
    // Or the bytes live only in a `.name.icloud` stub next to the (absent) file.
    !path.exists() && icloud_stub(path).is_some()
}

/// Attempt to download an offloaded file's data, blocking until it arrives or
/// `timeout` elapses. Returns `true` if the file ends up with real bytes.
pub fn materialize(path: &Path, timeout: Duration) -> bool {
    if has_data(path) {
        return true;
    }

    // `brctl download` is the classic tool; `fileproviderctl materialize` is the
    // modern File Provider equivalent. Try both — either may be a no-op depending on
    // the macOS version.
    let _ = Command::new("brctl").arg("download").arg(path).output();
    let _ = Command::new("fileproviderctl")
        .arg("materialize")
        .arg(path)
        .output();

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if has_data(path) {
            return true;
        }
        sleep(Duration::from_millis(250));
    }
    has_data(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_file_has_data_and_is_not_dataless() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("photo.jpg");
        std::fs::write(&f, b"not empty").unwrap();
        assert!(has_data(&f));
        assert!(!is_dataless(&f));
    }

    #[test]
    fn missing_file_has_no_data() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_data(&dir.path().join("nope.jpg")));
    }

    #[test]
    fn detects_icloud_stub_for_evicted_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("clip.mov");
        // The real file is absent; only the `.clip.mov.icloud` stub exists.
        std::fs::write(dir.path().join(".clip.mov.icloud"), b"stub").unwrap();
        assert!(is_dataless(&f));
        assert!(!has_data(&f));
    }
}
