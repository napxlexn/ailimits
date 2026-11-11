// models/files.rs — bounded reads of other tools' files: a size cap for
// settings, the last bytes of a session file, a stamp (modification time and
// length) that says whether a file changed since it was last read, and the
// check every value passes before it leaves a reader.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

/// Settings files larger than this are not read.
pub(super) const SETTINGS_CAP: u64 = 256 * 1024;
/// How much of the end of a session file is read.
pub(super) const TAIL_LEN: u64 = 64 * 1024;
/// Longer model ids and efforts are not names: dropped.
const VALUE_CAP: usize = 128;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// What a file looked like when it was read: a changed stamp means read again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stamp {
    modified: Option<SystemTime>,
    pub(super) len: u64,
}

/// The stamp of a regular file; None when it is missing or not a file.
pub(super) fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then(|| Stamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// The whole file when it is at most `cap` bytes, without a UTF-8 BOM.
pub(super) fn read_capped(path: &Path, cap: u64) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    if file.metadata().ok()?.len() > cap {
        return None;
    }
    let mut buf = Vec::new();
    // The file may grow between the check and the read: read one byte past
    // the cap to notice.
    file.take(cap + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > cap {
        return None;
    }
    if buf.starts_with(BOM) {
        buf.drain(..BOM.len());
    }
    Some(buf)
}

/// The last `n` bytes of a file as text, whole lines only: a line cut by the
/// start of the window is dropped. Invalid UTF-8 becomes U+FFFD, so a broken
/// line fails to parse on its own without hiding the others.
pub(super) fn read_tail(path: &Path, n: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(n);
    // One byte before the window says whether its first line is whole.
    let from = start.saturating_sub(1);
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    file.take(n + (start - from)).read_to_end(&mut buf).ok()?;
    let text: &[u8] = if from == 0 && start == 0 {
        buf.strip_prefix(BOM).unwrap_or(&buf)
    } else {
        match buf.iter().position(|&b| b == b'\n') {
            Some(i) => &buf[i + 1..],
            None => &[],
        }
    };
    Some(String::from_utf8_lossy(text).into_owned())
}

/// A model id or an effort as text a label can show: a string, trimmed, not
/// empty, at most 128 bytes, no control characters. Anything else is None.
pub(super) fn clean(value: Option<&serde_json::Value>) -> Option<String> {
    clean_str(value?.as_str()?)
}

/// `clean` for a TOML value.
pub(super) fn clean_toml(value: Option<&toml::Value>) -> Option<String> {
    clean_str(value?.as_str()?)
}

fn clean_str(s: &str) -> Option<String> {
    let s = s.trim();
    let ok = !s.is_empty() && s.len() <= VALUE_CAP && !s.chars().any(char::is_control);
    ok.then(|| s.to_string())
}

#[cfg(test)]
pub(super) mod test_dir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh directory under %TEMP%, removed when dropped.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "ailimits-models-{tag}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        /// Writes `bytes` at `rel`, creating the folders on the way.
        pub fn write(&self, rel: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_dir::TempDir;
    use super::*;

    #[test]
    fn a_capped_read_refuses_big_files_and_drops_the_bom() {
        let dir = TempDir::new("cap");
        let small = dir.write("a.json", b"\xEF\xBB\xBF{}");
        assert_eq!(read_capped(&small, 16).as_deref(), Some(&b"{}"[..]));
        let big = dir.write("b.json", vec![b' '; 17]);
        assert!(read_capped(&big, 16).is_none());
        assert!(read_capped(&dir.path().join("missing"), 16).is_none());
        // A folder is not a file.
        assert!(read_capped(dir.path(), 16).is_none());
        assert!(stamp(dir.path()).is_none());
    }

    #[test]
    fn a_tail_holds_whole_lines_only() {
        let dir = TempDir::new("tail");
        let path = dir.write("s.jsonl", b"first line\nsecond\nthird\n");
        // The whole file fits.
        assert_eq!(read_tail(&path, 64).unwrap(), "first line\nsecond\nthird\n");
        // The window starts inside "first line": that line is dropped.
        assert_eq!(read_tail(&path, 16).unwrap(), "second\nthird\n");
        // The window starts exactly at "second": nothing is lost.
        assert_eq!(read_tail(&path, 13).unwrap(), "second\nthird\n");
        // No line break inside the window: nothing whole to read.
        let one = dir.write("one.jsonl", b"abcdefghijklmnop");
        assert_eq!(read_tail(&one, 4).unwrap(), "");
        // A BOM at the start of a short file goes; a cut UTF-8 sequence in a
        // dropped line does no harm.
        let bom = dir.write("bom.jsonl", b"\xEF\xBB\xBF{\"a\":1}\r\n");
        assert_eq!(read_tail(&bom, 64).unwrap(), "{\"a\":1}\r\n");
        let utf = dir.write("utf.jsonl", "ééé\nok\n".as_bytes());
        assert_eq!(read_tail(&utf, 4).unwrap(), "ok\n");
        assert!(read_tail(&dir.path().join("missing"), 64).is_none());
        let empty = dir.write("empty.jsonl", b"");
        assert_eq!(read_tail(&empty, 64).unwrap(), "");
    }

    #[test]
    fn only_short_plain_strings_pass() {
        use serde_json::json;
        assert_eq!(clean(Some(&json!(" gpt-6.1 "))).as_deref(), Some("gpt-6.1"));
        assert!(clean(Some(&json!(""))).is_none());
        assert!(clean(Some(&json!("   "))).is_none());
        assert!(clean(Some(&json!(5))).is_none());
        assert!(clean(Some(&json!(null))).is_none());
        assert!(clean(Some(&json!({"id": "x"}))).is_none());
        assert!(clean(Some(&json!("a\nb"))).is_none());
        assert!(clean(Some(&json!("x".repeat(129)))).is_none());
        assert!(clean(Some(&json!("x".repeat(128)))).is_some());
        assert!(clean(None).is_none());
    }
}
