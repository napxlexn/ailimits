// models/files.rs — bounded reads of other tools' files: a size cap for
// settings, a session file read back from its end in steps, a stamp (modification time and
// length) that says whether a file changed since it was last read, and the
// check every value passes before it leaves a reader.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

/// Settings files larger than this are not read.
pub(super) const SETTINGS_CAP: u64 = 256 * 1024;
/// A session file is read back from its end this much at a time...
pub(super) const TAIL_STEP: u64 = 64 * 1024;
/// ...until a model record turns up, at most this far from the end. Claude
/// Code writes a 110-160 KB record after a session's first answer, which
/// puts that answer past one step; measured on the owner's PC 2026-11-13,
/// the newest model record sat within 1 MB of the end in 97.9% of the
/// states a session file passed through.
pub(super) const TAIL_CAP: u64 = 1024 * 1024;
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

/// A file that could not be read this time: locked by a scanner or a
/// backup, an I/O error, or cut short while it was read. This says nothing
/// about what the file holds, so the caller keeps what it showed and reads
/// again on the next cycle, instead of caching "no model".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Unread;

/// Opens a file to read: Ok(None) when it is not there (a definite answer),
/// Err(Unread) when it is there but cannot be opened now.
fn open(path: &Path) -> Result<Option<File>, Unread> {
    match File::open(path) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Unread),
    }
}

/// The whole file when it is at most `cap` bytes, without a UTF-8 BOM.
/// Ok(None) when the file is missing or larger than `cap` (definite: no
/// model); Err(Unread) when it could not be read this time.
pub(super) fn read_capped(path: &Path, cap: u64) -> Result<Option<Vec<u8>>, Unread> {
    let Some(file) = open(path)? else {
        return Ok(None);
    };
    if file.metadata().map_err(|_| Unread)?.len() > cap {
        return Ok(None);
    }
    let mut buf = Vec::new();
    // The file may grow between the check and the read: read one byte past
    // the cap to notice.
    file.take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|_| Unread)?;
    if buf.len() as u64 > cap {
        return Ok(None);
    }
    if buf.starts_with(BOM) {
        buf.drain(..BOM.len());
    }
    Ok(Some(buf))
}

/// The last `n` bytes of a file as text, whole lines only: a line cut by the
/// start of the window is dropped. Invalid UTF-8 becomes U+FFFD, so a broken
/// line fails to parse on its own without hiding the others.
#[cfg(test)]
pub(super) fn read_tail(path: &Path, n: u64) -> Option<String> {
    scan_back(path, n, n, |tail| Some(tail.to_string()))
        .ok()
        .flatten()
}

/// Reads a file back from its end, `step` bytes at a time and at most `cap`
/// bytes in all, and gives the first answer `pick` finds: `pick` sees each
/// step's whole lines as text, the newest step first. A line that crosses
/// a step boundary is whole in the earlier step; a line cut by the cap is
/// dropped. Invalid UTF-8 becomes U+FFFD, so a broken line fails to parse
/// on its own without hiding the others. Memory: at most `cap` plus a step.
/// Ok(None): read, no answer (or no file). Err(Unread): not read this time,
/// among others because the file got shorter while it was read.
pub(super) fn scan_back<T>(
    path: &Path,
    step: u64,
    cap: u64,
    pick: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, Unread> {
    let Some(mut file) = open(path)? else {
        return Ok(None);
    };
    let len = file.metadata().map_err(|_| Unread)?.len();
    scan_back_in(&mut file, len, step, cap, pick)
}

/// `scan_back` over any source whose length was taken as `len` beforehand.
/// The source may hold less by the time it is read (a tool rewrote or cut
/// the file in place): a short read is Err(Unread), never a panic.
fn scan_back_in<R: Read + Seek, T>(
    src: &mut R,
    len: u64,
    step: u64,
    cap: u64,
    pick: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, Unread> {
    let floor = len.saturating_sub(cap);
    let step = step.max(1);
    let mut end = len;
    // The start of a line the newer step began in the middle of.
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let start = end.saturating_sub(step).max(floor);
        // One byte before the step says whether its first line is whole.
        let from = start.saturating_sub(1);
        let want = end - from;
        src.seek(SeekFrom::Start(from)).map_err(|_| Unread)?;
        let mut buf = Vec::with_capacity(want as usize + carry.len());
        src.by_ref()
            .take(want)
            .read_to_end(&mut buf)
            .map_err(|_| Unread)?;
        if (buf.len() as u64) < want {
            // The file is shorter than when it was measured.
            return Err(Unread);
        }
        buf.extend_from_slice(&carry);
        // Byte 0 is the one before the step (none at the file's start). A
        // newline there: every line is whole. Otherwise the first line goes
        // on in the next (earlier) step, or is cut by the cap.
        let cut = match (start, buf.iter().position(|&b| b == b'\n')) {
            (0, _) => 0,
            (_, Some(i)) => i + 1,
            (_, None) => buf.len(),
        };
        let whole = if start == 0 {
            buf.strip_prefix(BOM).unwrap_or(&buf)
        } else {
            buf.get(cut..).unwrap_or_default()
        };
        if let Some(found) = pick(&String::from_utf8_lossy(whole)) {
            return Ok(Some(found));
        }
        if start <= floor {
            return Ok(None);
        }
        carry = buf.get(1..cut).map(<[u8]>::to_vec).unwrap_or_default();
        end = start;
    }
}

/// The stamp of a SQLite database with its write-ahead log: in WAL mode a
/// database takes its writes in `<file>-wal` until a checkpoint, so the
/// main file alone can look unchanged after a change.
pub(super) fn db_stamp(path: &Path) -> Option<Stamp> {
    let db = stamp(path)?;
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let wal = stamp(Path::new(&wal));
    Some(Stamp {
        modified: db.modified.max(wal.and_then(|w| w.modified)),
        len: db.len.wrapping_add(wal.map_or(0, |w| w.len)),
    })
}

/// The entry of Cursor's state database that holds the editor's AI
/// settings, the model chosen for each place among them.
const CURSOR_ITEM: &str =
    "src.vs.platform.reactivestorage.browser.reactiveStorageServiceImpl.persistentStorage.applicationUser";
/// Only the Agent's model settings leave the database: the rest of the
/// entry (half a megabyte of the editor's settings on the owner's PC) is
/// never copied out, and no other entry (the sign-in among them) is read.
const CURSOR_AGENT_QUERY: &str = "SELECT json_extract(CAST(value AS TEXT), \
     '$.aiSettings.modelConfig.composer') FROM ItemTable WHERE key = ?1";
/// For a Windows SQLite built without JSON functions: the same entry whole,
/// and the object taken out here. The size is checked inside SQLite, so an
/// entry over `CURSOR_ENTRY_CAP` never leaves the database (`length` of a
/// blob is its bytes, of text its characters; the copy is checked again in
/// bytes).
const CURSOR_ENTRY_CAP: usize = 8 * 1024 * 1024;

/// The fallback query with the cap inside it: NULL for a larger entry.
fn cursor_entry_query(cap: usize) -> String {
    format!(
        "SELECT CASE WHEN length(value) <= {cap} THEN CAST(value AS TEXT) END \
         FROM ItemTable WHERE key = ?1"
    )
}

/// The Agent's model settings in Cursor's state database, as JSON text:
/// Ok(None) when there are none; Err(Unread) when the database could not be
/// read this time (Cursor holds a lock, an I/O error, or it is not a
/// database Cursor wrote): the caller keeps what it showed and asks again on
/// the next cycle. Opened read-only through Windows' own SQLite
/// (`platform::sqlite`), which never changes the file and leaves nothing
/// beside it.
pub(super) fn cursor_agent_model(path: &Path) -> Result<Option<String>, Unread> {
    use crate::platform::sqlite::{lookup_texts, SqliteError};
    let first = |sql: &str| lookup_texts(path, sql, &[CURSOR_ITEM]).map(|mut v| v.pop().flatten());
    match first(CURSOR_AGENT_QUERY) {
        Ok(found) => Ok(found),
        Err(SqliteError::Failed(e)) if e.contains("no such function") => {
            match first(&cursor_entry_query(CURSOR_ENTRY_CAP)) {
                Ok(Some(entry)) if entry.len() <= CURSOR_ENTRY_CAP => Ok(composer_of(&entry)),
                // Over the cap, or no entry: no model, as for any odd shape.
                Ok(_) => Ok(None),
                Err(_) => Err(Unread),
            }
        }
        Err(_) => Err(Unread),
    }
}

/// The Agent's object out of the whole entry, as JSON text.
fn composer_of(entry: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(entry)
        .ok()
        .and_then(|v| v.pointer("/aiSettings/modelConfig/composer").cloned())
        .filter(|v| !v.is_null())
        .map(|v| v.to_string())
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

/// `clean` for text from elsewhere (an environment variable).
pub(super) fn clean_str(s: &str) -> Option<String> {
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
        assert_eq!(read_capped(&small, 16), Ok(Some(b"{}".to_vec())));
        let big = dir.write("b.json", vec![b' '; 17]);
        assert_eq!(read_capped(&big, 16), Ok(None));
        assert_eq!(read_capped(&dir.path().join("missing"), 16), Ok(None));
        // A folder is not a file (and is never stamped, so never read).
        assert!(stamp(dir.path()).is_none());
    }

    /// A file another program holds open without sharing (a scanner, a
    /// backup) is "not read this time", never "no model": the reader keeps
    /// the label and asks again. Once released it reads.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_locked_file_is_unread_not_empty() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = TempDir::new("locked");
        let path = dir.write("config.toml", "model = \"gpt-6.1\"\n");
        let lock = std::fs::File::options()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert_eq!(read_capped(&path, 1024), Err(Unread));
        assert_eq!(scan_back(&path, 4, 64, |t| Some(t.len())), Err(Unread));
        drop(lock);
        assert_eq!(
            read_capped(&path, 1024),
            Ok(Some(b"model = \"gpt-6.1\"\n".to_vec()))
        );
        assert_eq!(scan_back(&path, 64, 64, |t| Some(t.len())), Ok(Some(18)));
    }

    /// A session file a tool cuts or rewrites in place while it is read: the
    /// length taken first is more than the file then holds. Every such read
    /// is Err(Unread) — in the release build a panic here would end the
    /// whole app (panic = "abort").
    #[test]
    fn a_file_shorter_than_measured_is_unread_never_a_panic() {
        use std::io::Cursor;
        let text = b"a\nbbbbbbbbbbbb\ncc\n\nddddd\neeeeeeeeeeee\nf\n";
        let len = text.len() as u64;
        // The exact case found: nothing left at all, a window that does not
        // start at the file's beginning.
        let mut empty = Cursor::new(Vec::new());
        let got: Result<Option<()>, Unread> = scan_back_in(&mut empty, len, 8, 1 << 20, |_| None);
        assert_eq!(got, Err(Unread));
        // Every shorter length, every step, with and without a cap.
        for kept in 0..len {
            for step in 1..=len + 1 {
                for cap in [3, len / 2, 1 << 20] {
                    let mut src = Cursor::new(text[..kept as usize].to_vec());
                    let got: Result<Option<()>, Unread> =
                        scan_back_in(&mut src, len, step, cap, |_| None);
                    assert_eq!(got, Err(Unread), "kept {kept}, step {step}, cap {cap}");
                }
            }
        }
        // The full length still reads every line.
        let mut whole = Cursor::new(text.to_vec());
        let lines = std::cell::RefCell::new(0);
        let none: Result<Option<()>, Unread> = scan_back_in(&mut whole, len, 5, 1 << 20, |part| {
            *lines.borrow_mut() += part.lines().count();
            None
        });
        assert_eq!(none, Ok(None));
        assert_eq!(lines.into_inner(), 7);
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

    /// Every line of a file, newest first, as `scan_back` hands them out:
    /// each whole exactly once, whatever the step.
    fn lines_back(path: &Path, step: u64, cap: u64) -> Vec<String> {
        let seen = std::cell::RefCell::new(Vec::new());
        let none: Result<Option<()>, Unread> = scan_back(path, step, cap, |part| {
            let mut lines: Vec<String> = part.lines().map(String::from).collect();
            lines.reverse();
            seen.borrow_mut().extend(lines);
            None
        });
        assert_eq!(none, Ok(None));
        seen.into_inner()
    }

    #[test]
    fn a_scan_back_sees_every_line_once_whole() {
        let dir = TempDir::new("scan");
        let text = "a\nbbbbbbbbbbbbbbbbbbbb\nc\n\nddddd\neeeeeeeeeeee\nf\n";
        let path = dir.write("s.jsonl", text);
        let mut want: Vec<&str> = text.lines().collect();
        want.reverse();
        for step in 1..=text.len() as u64 + 2 {
            assert_eq!(lines_back(&path, step, 1 << 20), want, "step {step}");
        }
        // The first answer found ends the read.
        let pick = |part: &str| {
            part.lines()
                .rev()
                .find(|l| l.starts_with('d'))
                .map(String::from)
        };
        assert_eq!(
            scan_back(&path, 3, 1 << 20, pick),
            Ok(Some("ddddd".to_string()))
        );
        // A line cut by the cap is dropped; every line inside it is whole.
        let cap = "ddddd\neeeeeeeeeeee\nf\n".len() as u64;
        for step in 1..=cap {
            assert_eq!(lines_back(&path, step, cap), ["f", "eeeeeeeeeeee", "ddddd"]);
        }
        assert_eq!(lines_back(&path, 4, cap - 1), ["f", "eeeeeeeeeeee"]);
        // A BOM goes, CRLF stays for the parser, a missing file is None.
        let bom = dir.write("bom.jsonl", b"\xEF\xBB\xBF{}\r\nx\r\n");
        assert_eq!(lines_back(&bom, 2, 64), ["x", "{}"]);
        let missing = scan_back(&dir.path().join("missing"), 4, 64, |_| Some(()));
        assert_eq!(missing, Ok(None));
        let empty = dir.write("empty.jsonl", b"");
        assert!(lines_back(&empty, 4, 64).is_empty());
    }

    /// The fallback for a SQLite without JSON functions copies the entry out
    /// only when it is within the cap: a larger one is NULL inside SQLite.
    #[cfg(target_os = "windows")]
    #[test]
    fn the_whole_entry_fallback_is_capped_inside_sqlite() {
        use crate::platform::sqlite::lookup_texts;
        use crate::platform::sqlite::test_support::{TempDb, TestDb};
        assert!(cursor_entry_query(CURSOR_ENTRY_CAP).contains("8388608"));
        let tmp = TempDb::new("entry-cap");
        {
            let db = TestDb::create(&tmp.0);
            db.exec("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);");
            db.exec(&format!(
                "INSERT INTO ItemTable VALUES ('{CURSOR_ITEM}', CAST('{{\"aiSettings\": {{\"modelConfig\": {{\"composer\": {{\"modelName\": \"x\"}}}}}}}}' AS BLOB));"
            ));
            db.exec("INSERT INTO ItemTable VALUES ('text', '0123456789');");
        }
        let entry = |cap: usize, key: &str| {
            lookup_texts(&tmp.0, &cursor_entry_query(cap), &[key]).map(|mut v| v.pop().flatten())
        };
        let whole = entry(1024, CURSOR_ITEM).unwrap().unwrap();
        assert_eq!(composer_of(&whole).as_deref(), Some(r#"{"modelName":"x"}"#));
        // One byte over: nothing leaves the database.
        let len = whole.len();
        assert_eq!(entry(len - 1, CURSOR_ITEM).unwrap(), None);
        assert_eq!(
            entry(len, CURSOR_ITEM).unwrap().as_deref(),
            Some(&whole[..])
        );
        // A text value is measured too.
        assert_eq!(entry(9, "text").unwrap(), None);
        assert_eq!(entry(10, "text").unwrap().as_deref(), Some("0123456789"));
        assert_eq!(composer_of("not json"), None);
        assert_eq!(composer_of(r#"{"aiSettings": {}}"#), None);
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
