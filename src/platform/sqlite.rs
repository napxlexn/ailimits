// platform/sqlite.rs — read-only lookups in another app's SQLite database,
// through the SQLite that ships with Windows (System32\winsqlite3.dll).
//
// Why not a crate: a bundled SQLite (rusqlite "bundled") compiles the whole
// engine into the exe for one SELECT a minute. winsqlite3.dll is part of
// Windows 10 and 11, is loaded from System32 only, and only once a lookup
// actually runs — a machine without it degrades to an honest error instead of
// failing to start. See docs/en/ARCHITECTURE.md, "Key decisions".
//
// Every connection is read-only: the database belongs to another app (which
// may have it open and be writing to it), and nothing here may ever change
// it or leave a file beside it. How it is opened depends on the database
// (`open_mode`):
// - A WAL database whose owner is not running has no -wal file. A plain
//   read-only open would create -wal and -shm beside it to read it, and a
//   read-only connection cannot remove them again. So it is opened as
//   `file:...?immutable=1`: no lock is taken and no file is created, and
//   nothing can change the database meanwhile, as its owner is not running.
// - A WAL database whose owner has it open (its -wal exists, and its -shm):
//   SQLITE_OPEN_READONLY, reading the owner's -wal/-shm as they are, never
//   checkpointing or truncating them. Nothing new is created.
// - A database in rollback-journal mode: SQLITE_OPEN_READONLY; a reader
//   creates no file there, and the shared lock keeps the read consistent.

use std::path::Path;

/// How long one lookup may wait on the owner's lock before giving up. Short
/// on purpose: a busy database is a transient error, the next cycle retries.
const BUSY_TIMEOUT_MS: i32 = 200;

/// Why a lookup failed.
#[derive(Debug)]
pub enum SqliteError {
    /// winsqlite3.dll could not be loaded or lacks an export.
    Unavailable(String),
    /// The owner holds a lock right now (SQLITE_BUSY / SQLITE_LOCKED).
    Busy,
    /// Anything else: not a database, corrupt, no such table, I/O.
    Failed(String),
}

impl std::fmt::Display for SqliteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SqliteError::Unavailable(e) => write!(f, "winsqlite3 unavailable: {e}"),
            SqliteError::Busy => write!(f, "database is locked by its owner"),
            SqliteError::Failed(e) => write!(f, "sqlite error: {e}"),
        }
    }
}

impl std::error::Error for SqliteError {}

/// Run `sql` — one statement with a single `?1` text parameter and one result
/// column — once per key against the database at `path`, opened strictly
/// read-only. Returns, per key, the first row's first column as text; `None`
/// when there is no row, the value is NULL, or it is not UTF-8.
pub fn lookup_texts(
    path: &Path,
    sql: &str,
    keys: &[&str],
) -> Result<Vec<Option<String>>, SqliteError> {
    #[cfg(target_os = "windows")]
    {
        win::lookup_texts(path, sql, keys)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (path, sql, keys);
        Err(SqliteError::Unavailable(
            "winsqlite3 exists only on Windows".to_string(),
        ))
    }
}

/// How a lookup opens the database, so that reading it leaves nothing behind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
enum OpenMode {
    /// SQLITE_OPEN_READONLY on the plain path, with the usual read locks.
    Shared,
    /// SQLITE_OPEN_READONLY | SQLITE_OPEN_URI on this `?immutable=1` URI.
    Immutable(String),
}

/// `Immutable` for a WAL database without its -wal file (its owner is not
/// running), when the path has a URI form; `Shared` otherwise.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn open_mode(path: &Path) -> OpenMode {
    if !is_wal_database(path) || sidecar(path, "-wal").exists() {
        return OpenMode::Shared;
    }
    immutable_uri(path).map_or(OpenMode::Shared, OpenMode::Immutable)
}

/// Whether the file is a SQLite database in WAL mode, from its 100-byte
/// header: "SQLite format 3\0", then the write and read versions (bytes 18
/// and 19), both 2 in WAL mode. Anything unreadable is not, and the open
/// that follows reports it honestly.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_wal_database(path: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 20];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok()
        && header.starts_with(b"SQLite format 3\0")
        && header[18] == 2
        && header[19] == 2
}

/// `<path><suffix>`: the -wal, -shm or -journal file beside a database.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn sidecar(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(suffix);
    p.into()
}

/// The `file:` URI that opens `path` read-only with no lock and no file
/// created: `file:///C:/Users/.../state.vscdb?immutable=1`, every byte but
/// the plain ones percent-encoded (spaces, `%`, `#`, `?`, non-ASCII as its
/// UTF-8). Only a path on a drive (`C:\...`) has one; a UNC or `\\?\` path,
/// or one that is not UTF-8, gets None and is opened the plain way.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn immutable_uri(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let b = text.as_bytes();
    let on_drive =
        b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/');
    if !on_drive {
        return None;
    }
    let mut uri = String::from("file:///");
    for (i, &byte) in b.iter().enumerate() {
        match byte {
            b'\\' | b'/' => uri.push('/'),
            b':' if i == 1 => uri.push(':'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri.push_str("?immutable=1");
    Some(uri)
}

#[cfg(target_os = "windows")]
mod win {
    use super::{OpenMode, SqliteError, BUSY_TIMEOUT_MS};
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::path::Path;
    use std::ptr::{null, null_mut};
    use std::sync::OnceLock;
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
    };

    const SQLITE_OK: c_int = 0;
    const SQLITE_BUSY: c_int = 5;
    const SQLITE_LOCKED: c_int = 6;
    const SQLITE_ROW: c_int = 100;
    const SQLITE_DONE: c_int = 101;
    const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;
    const SQLITE_OPEN_URI: c_int = 0x0000_0040;
    #[cfg(test)]
    const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
    #[cfg(test)]
    const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;

    type Db = c_void;
    type Stmt = c_void;
    /// A bind destructor; `None` is SQLITE_STATIC (the caller keeps the bytes
    /// alive until the statement is reset).
    type Destructor = Option<unsafe extern "system" fn(*mut c_void)>;

    // winsqlite3 is built with the system calling convention (__stdcall on
    // x86); "system" is the same as "C" on x64 and arm64.
    type OpenV2 =
        unsafe extern "system" fn(*const c_char, *mut *mut Db, c_int, *const c_char) -> c_int;
    type CloseV2 = unsafe extern "system" fn(*mut Db) -> c_int;
    type BusyTimeout = unsafe extern "system" fn(*mut Db, c_int) -> c_int;
    type ErrMsg = unsafe extern "system" fn(*mut Db) -> *const c_char;
    type PrepareV2 = unsafe extern "system" fn(
        *mut Db,
        *const c_char,
        c_int,
        *mut *mut Stmt,
        *mut *const c_char,
    ) -> c_int;
    type BindText =
        unsafe extern "system" fn(*mut Stmt, c_int, *const c_char, c_int, Destructor) -> c_int;
    type Step = unsafe extern "system" fn(*mut Stmt) -> c_int;
    type Reset = unsafe extern "system" fn(*mut Stmt) -> c_int;
    type ColumnText = unsafe extern "system" fn(*mut Stmt, c_int) -> *const u8;
    type ColumnBytes = unsafe extern "system" fn(*mut Stmt, c_int) -> c_int;
    type Finalize = unsafe extern "system" fn(*mut Stmt) -> c_int;
    #[cfg(test)]
    type Exec = unsafe extern "system" fn(
        *mut Db,
        *const c_char,
        *const c_void,
        *mut c_void,
        *mut *mut c_char,
    ) -> c_int;

    /// The handful of exports a lookup needs, resolved once.
    struct Api {
        open_v2: OpenV2,
        close_v2: CloseV2,
        busy_timeout: BusyTimeout,
        errmsg: ErrMsg,
        prepare_v2: PrepareV2,
        bind_text: BindText,
        step: Step,
        reset: Reset,
        column_text: ColumnText,
        column_bytes: ColumnBytes,
        finalize: Finalize,
        /// Test builds only: the tests write throwaway databases to read back.
        #[cfg(test)]
        exec: Exec,
    }

    /// The API, loaded on first use and kept for the process lifetime. The
    /// failure is cached too, so a machine without the DLL does not retry the
    /// load every cycle.
    fn api() -> Result<&'static Api, SqliteError> {
        static API: OnceLock<Result<Api, String>> = OnceLock::new();
        API.get_or_init(load)
            .as_ref()
            .map_err(|e| SqliteError::Unavailable(e.clone()))
    }

    fn load() -> Result<Api, String> {
        let name: Vec<u16> = "winsqlite3.dll\0".encode_utf16().collect();
        // System32 only — never the application directory or the PATH, so a
        // planted winsqlite3.dll next to the exe is never picked up.
        let module =
            unsafe { LoadLibraryExW(name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if module.is_null() {
            return Err(format!(
                "LoadLibraryExW failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        macro_rules! export {
            ($name:literal, $ty:ty) => {{
                let proc = unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) }
                    .ok_or_else(|| format!("export {} missing", $name))?;
                // SAFETY: the export has exactly this C signature (sqlite3.h);
                // only the pointer's type changes.
                unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(proc) }
            }};
        }
        Ok(Api {
            open_v2: export!("sqlite3_open_v2", OpenV2),
            close_v2: export!("sqlite3_close_v2", CloseV2),
            busy_timeout: export!("sqlite3_busy_timeout", BusyTimeout),
            errmsg: export!("sqlite3_errmsg", ErrMsg),
            prepare_v2: export!("sqlite3_prepare_v2", PrepareV2),
            bind_text: export!("sqlite3_bind_text", BindText),
            step: export!("sqlite3_step", Step),
            reset: export!("sqlite3_reset", Reset),
            column_text: export!("sqlite3_column_text", ColumnText),
            column_bytes: export!("sqlite3_column_bytes", ColumnBytes),
            finalize: export!("sqlite3_finalize", Finalize),
            #[cfg(test)]
            exec: export!("sqlite3_exec", Exec),
        })
    }

    /// An open connection; closed on drop. sqlite3_open_v2 hands back a
    /// handle even when it fails, and that one must be closed as well.
    struct Conn {
        api: &'static Api,
        db: *mut Db,
    }

    impl Conn {
        fn open(api: &'static Api, path: &Path, flags: c_int) -> Result<Conn, SqliteError> {
            // SQLite takes UTF-8 file names on Windows.
            let path = path
                .to_str()
                .ok_or_else(|| SqliteError::Failed("database path is not UTF-8".to_string()))?;
            Conn::open_name(api, path, flags)
        }

        /// Open by a name SQLite takes as it is: a path, or a `file:` URI
        /// with SQLITE_OPEN_URI.
        fn open_name(api: &'static Api, name: &str, flags: c_int) -> Result<Conn, SqliteError> {
            let path = CString::new(name)
                .map_err(|_| SqliteError::Failed("database path holds a NUL".to_string()))?;
            let mut db = null_mut();
            let rc = unsafe { (api.open_v2)(path.as_ptr(), &mut db, flags, null()) };
            let conn = Conn { api, db };
            if rc != SQLITE_OK {
                return Err(conn.error(rc));
            }
            unsafe { (api.busy_timeout)(db, BUSY_TIMEOUT_MS) };
            Ok(conn)
        }

        /// Classify a result code; the message comes from the connection
        /// (SQL and file names only — no bound values are ever echoed).
        fn error(&self, rc: c_int) -> SqliteError {
            // Extended codes keep the primary code in the low byte.
            if matches!(rc & 0xff, SQLITE_BUSY | SQLITE_LOCKED) {
                return SqliteError::Busy;
            }
            let msg = if self.db.is_null() {
                format!("code {rc}")
            } else {
                let ptr = unsafe { (self.api.errmsg)(self.db) };
                if ptr.is_null() {
                    format!("code {rc}")
                } else {
                    let text = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
                    format!("{text} (code {rc})")
                }
            };
            SqliteError::Failed(msg)
        }

        /// Execute SQL with no results — test builds only.
        #[cfg(test)]
        fn exec(&self, sql: &str) -> Result<(), SqliteError> {
            let sql = CString::new(sql).map_err(|_| SqliteError::Failed("NUL in SQL".into()))?;
            let rc =
                unsafe { (self.api.exec)(self.db, sql.as_ptr(), null(), null_mut(), null_mut()) };
            if rc == SQLITE_OK {
                Ok(())
            } else {
                Err(self.error(rc))
            }
        }
    }

    impl Drop for Conn {
        fn drop(&mut self) {
            if !self.db.is_null() {
                unsafe { (self.api.close_v2)(self.db) };
            }
        }
    }

    /// A prepared statement; finalized on drop (before its connection).
    struct Statement<'c> {
        conn: &'c Conn,
        stmt: *mut Stmt,
    }

    impl Drop for Statement<'_> {
        fn drop(&mut self) {
            if !self.stmt.is_null() {
                unsafe { (self.conn.api.finalize)(self.stmt) };
            }
        }
    }

    impl Statement<'_> {
        /// Bind `key` to `?1`, step once, copy the first column out, reset.
        fn first_text(&self, key: &str) -> Result<Option<String>, SqliteError> {
            let api = self.conn.api;
            let len = c_int::try_from(key.len())
                .map_err(|_| SqliteError::Failed("key too long".to_string()))?;
            // SQLITE_STATIC: `key` outlives the step and the reset below.
            let rc = unsafe { (api.bind_text)(self.stmt, 1, key.as_ptr().cast(), len, None) };
            if rc != SQLITE_OK {
                return Err(self.conn.error(rc));
            }
            let result = match unsafe { (api.step)(self.stmt) } {
                SQLITE_ROW => {
                    // Text first, then the byte count — the order sqlite3.h
                    // asks for, so the count matches the converted text.
                    let ptr = unsafe { (api.column_text)(self.stmt, 0) };
                    let len = unsafe { (api.column_bytes)(self.stmt, 0) };
                    if ptr.is_null() || len <= 0 {
                        Ok(None)
                    } else {
                        // Copied out before the reset invalidates the pointer.
                        let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
                        Ok(String::from_utf8(bytes.to_vec()).ok())
                    }
                }
                SQLITE_DONE => Ok(None),
                rc => Err(self.conn.error(rc)),
            };
            unsafe { (api.reset)(self.stmt) };
            result
        }
    }

    pub(super) fn lookup_texts(
        path: &Path,
        sql: &str,
        keys: &[&str],
    ) -> Result<Vec<Option<String>>, SqliteError> {
        let api = api()?;
        let conn = match super::open_mode(path) {
            OpenMode::Shared => Conn::open(api, path, SQLITE_OPEN_READONLY)?,
            OpenMode::Immutable(uri) => {
                Conn::open_name(api, &uri, SQLITE_OPEN_READONLY | SQLITE_OPEN_URI)?
            }
        };
        let sql = CString::new(sql).map_err(|_| SqliteError::Failed("NUL in SQL".into()))?;
        let mut stmt = null_mut();
        let rc = unsafe { (api.prepare_v2)(conn.db, sql.as_ptr(), -1, &mut stmt, null_mut()) };
        let stmt = Statement { conn: &conn, stmt };
        if rc != SQLITE_OK || stmt.stmt.is_null() {
            return Err(conn.error(rc));
        }
        keys.iter().map(|key| stmt.first_text(key)).collect()
    }

    /// A writable connection for building throwaway databases in tests.
    #[cfg(test)]
    pub(crate) struct TestDb(Conn);

    #[cfg(test)]
    impl TestDb {
        pub(crate) fn create(path: &Path) -> TestDb {
            let api = api().expect("winsqlite3.dll loads");
            let conn = Conn::open(api, path, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE)
                .expect("throwaway database opens");
            TestDb(conn)
        }

        pub(crate) fn exec(&self, sql: &str) {
            self.0.exec(sql).expect("test SQL runs");
        }
    }
}

/// Throwaway databases for tests here and in the providers that read one.
#[cfg(all(test, target_os = "windows"))]
pub(crate) mod test_support {
    pub(crate) use super::win::TestDb;

    /// A fresh path under %TEMP% per test, removed (with any -wal/-shm/-journal
    /// sidecar) when the guard drops.
    pub(crate) struct TempDb(pub std::path::PathBuf);

    impl TempDb {
        pub(crate) fn new(tag: &str) -> TempDb {
            let path = std::env::temp_dir().join(format!(
                "ailimits-sqlite-{tag}-{}-{}.db",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            ));
            TempDb(path)
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let mut p = self.0.clone().into_os_string();
                p.push(suffix);
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::test_support::{TempDb, TestDb};
    use super::*;

    const QUERY: &str = "SELECT value FROM ItemTable WHERE key = ?1";
    const SCHEMA: &str =
        "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);";

    #[test]
    fn reads_text_and_blob_values_back_and_misses_cleanly() {
        let tmp = TempDb::new("values");
        {
            let db = TestDb::create(&tmp.0);
            db.exec(SCHEMA);
            db.exec("INSERT INTO ItemTable VALUES ('a', 'text value');");
            db.exec("INSERT INTO ItemTable VALUES ('b', CAST('blob value' AS BLOB));");
            db.exec("INSERT INTO ItemTable VALUES ('n', NULL);");
        }
        let got = lookup_texts(&tmp.0, QUERY, &["a", "b", "missing", "n"]).expect("lookup");
        assert_eq!(
            got,
            vec![
                Some("text value".to_string()),
                Some("blob value".to_string()),
                None,
                None
            ]
        );
    }

    #[test]
    fn a_held_write_lock_is_busy_not_a_failure() {
        let tmp = TempDb::new("locked");
        let db = TestDb::create(&tmp.0);
        db.exec(SCHEMA);
        db.exec("INSERT INTO ItemTable VALUES ('a', 'x');");
        // Rollback-journal mode: an exclusive transaction shuts readers out.
        db.exec("BEGIN EXCLUSIVE;");
        assert!(matches!(
            lookup_texts(&tmp.0, QUERY, &["a"]),
            Err(SqliteError::Busy)
        ));
        db.exec("COMMIT;");
        assert_eq!(
            lookup_texts(&tmp.0, QUERY, &["a"]).expect("lock released"),
            vec![Some("x".to_string())]
        );
    }

    #[test]
    fn a_wal_database_is_read_with_its_uncheckpointed_writes() {
        // The Cursor state database runs in WAL mode; a value the owner wrote
        // but has not checkpointed yet lives only in the -wal file.
        let tmp = TempDb::new("wal");
        let db = TestDb::create(&tmp.0);
        db.exec("PRAGMA journal_mode=WAL;");
        db.exec("PRAGMA wal_autocheckpoint=0;");
        db.exec(SCHEMA);
        db.exec("INSERT INTO ItemTable VALUES ('a', 'fresh');");
        // The writer stays open, so nothing is checkpointed into the main file.
        // Its -wal is there: the owner's files are read as they are.
        assert_eq!(open_mode(&tmp.0), OpenMode::Shared);
        assert_eq!(
            lookup_texts(&tmp.0, QUERY, &["a"]).expect("lookup"),
            vec![Some("fresh".to_string())]
        );
        // A writer can keep going after the read-only reader is done.
        db.exec("INSERT INTO ItemTable VALUES ('a', 'newer');");
        assert_eq!(
            lookup_texts(&tmp.0, QUERY, &["a"]).expect("lookup"),
            vec![Some("newer".to_string())]
        );
    }

    /// The files beside a database, as they are now.
    fn sidecars(path: &std::path::Path) -> Vec<&'static str> {
        ["-wal", "-shm", "-journal"]
            .into_iter()
            .filter(|s| sidecar(path, s).exists())
            .collect()
    }

    /// A WAL database whose owner is not running (Cursor closed: it removes
    /// its -wal and -shm on exit) is read without a single file appearing
    /// beside it — a plain read-only open would leave -wal and -shm behind.
    fn reads_a_closed_wal_database_without_leaving_files(path: &std::path::Path) {
        {
            let db = TestDb::create(path);
            db.exec("PRAGMA journal_mode=WAL;");
            db.exec(SCHEMA);
            db.exec("INSERT INTO ItemTable VALUES ('a', 'kept');");
        }
        assert!(is_wal_database(path));
        assert!(
            sidecars(path).is_empty(),
            "the owner left {:?}",
            sidecars(path)
        );
        assert!(matches!(open_mode(path), OpenMode::Immutable(_)));
        for _ in 0..3 {
            assert_eq!(
                lookup_texts(path, QUERY, &["a", "missing"]).expect("lookup"),
                vec![Some("kept".to_string()), None]
            );
            assert!(sidecars(path).is_empty(), "left {:?}", sidecars(path));
        }
        // The owner starts again and writes: its -wal is there now, and the
        // read goes through it (its newest value), still adding nothing.
        let db = TestDb::create(path);
        db.exec("PRAGMA wal_autocheckpoint=0;");
        db.exec("INSERT INTO ItemTable VALUES ('a', 'newer');");
        let before = sidecars(path);
        assert!(before.contains(&"-wal"), "{before:?}");
        assert_eq!(open_mode(path), OpenMode::Shared);
        assert_eq!(
            lookup_texts(path, QUERY, &["a"]).expect("lookup"),
            vec![Some("newer".to_string())]
        );
        assert_eq!(sidecars(path), before);
    }

    #[test]
    fn a_closed_wal_database_is_read_without_leaving_files() {
        let tmp = TempDb::new("immutable");
        reads_a_closed_wal_database_without_leaving_files(&tmp.0);
    }

    /// The same where the URI needs escaping: a Cyrillic user name, spaces,
    /// `#`, `%`, `?`-less but `&`, `=`, brackets, as profile folders have.
    #[test]
    fn a_closed_wal_database_in_an_odd_folder_is_read_the_same() {
        let dir = std::env::temp_dir().join(format!(
            "ailimits sqlite Вася #1 %20 a&b=c (x) [y] {}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = TempDb(dir.join("state.vscdb"));
        reads_a_closed_wal_database_without_leaving_files(&tmp.0);
        drop(tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A database in rollback-journal mode is opened the plain way (its
    /// shared lock keeps the read consistent), and a reader creates nothing.
    #[test]
    fn a_rollback_journal_database_is_read_shared_and_leaves_nothing() {
        let tmp = TempDb::new("journal");
        {
            let db = TestDb::create(&tmp.0);
            db.exec(SCHEMA);
            db.exec("INSERT INTO ItemTable VALUES ('a', 'x');");
        }
        assert!(!is_wal_database(&tmp.0));
        assert_eq!(open_mode(&tmp.0), OpenMode::Shared);
        assert_eq!(
            lookup_texts(&tmp.0, QUERY, &["a"]).expect("lookup"),
            vec![Some("x".to_string())]
        );
        assert!(sidecars(&tmp.0).is_empty(), "{:?}", sidecars(&tmp.0));
    }

    #[test]
    fn the_immutable_uri_escapes_everything_but_plain_bytes() {
        use std::path::Path;
        assert_eq!(
            immutable_uri(Path::new(
                r"C:\Users\Вася\AppData\Roaming\Cursor\User\globalStorage\state.vscdb"
            ))
            .as_deref(),
            Some("file:///C:/Users/%D0%92%D0%B0%D1%81%D1%8F/AppData/Roaming/Cursor/User/globalStorage/state.vscdb?immutable=1")
        );
        assert_eq!(
            immutable_uri(Path::new(r"d:\a b\#1%?&=(x):y.db")).as_deref(),
            Some("file:///d:/a%20b/%231%25%3F%26%3D%28x%29%3Ay.db?immutable=1")
        );
        // No URI form: opened the plain way.
        for odd in [
            r"\\server\share\state.vscdb",
            r"\\?\C:\Users\x\state.vscdb",
            r"state.vscdb",
            r"\Users\x\state.vscdb",
            r"C:state.vscdb",
        ] {
            assert_eq!(immutable_uri(Path::new(odd)), None, "{odd}");
        }
        // Not a database, or missing: the plain way, which fails honestly.
        assert_eq!(
            open_mode(Path::new(r"C:\definitely\missing.db")),
            OpenMode::Shared
        );
    }

    #[test]
    fn a_file_that_is_not_a_database_fails_honestly() {
        let tmp = TempDb::new("garbage");
        std::fs::write(&tmp.0, b"this is not a sqlite database, just text").unwrap();
        assert!(matches!(
            lookup_texts(&tmp.0, QUERY, &["a"]),
            Err(SqliteError::Failed(_))
        ));
    }

    #[test]
    fn a_missing_file_is_never_created() {
        let tmp = TempDb::new("missing");
        assert!(matches!(
            lookup_texts(&tmp.0, QUERY, &["a"]),
            Err(SqliteError::Failed(_))
        ));
        assert!(!tmp.0.exists(), "a read-only open must not create the file");
    }
}
