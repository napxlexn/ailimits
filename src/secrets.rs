// secrets.rs — the app's own secrets in Credential Manager (service
// `ailimits`), kept on this PC only.
//
// keyring 3 writes every credential with CRED_PERSIST_ENTERPRISE, which a
// roaming domain profile copies to every PC the user signs in to. The app's
// keys and tokens belong to this PC: they are written here with
// CRED_PERSIST_LOCAL_MACHINE, under the very target keyring reads
// (`<label>.<service>`, a generic credential, the label as user name), so
// keyring's reads and deletes (ailimits-auth, the uninstaller) find them as
// before. A secret read here that is still kept the old way is written again
// the new way (`keep_local`), once.
//
// Imports: CredWriteW, CredReadW and CredFree come with keyring already; no
// new import.

/// The Credential Manager service of every secret the app keeps.
pub const SERVICE: &str = "ailimits";

/// The longest secret Credential Manager keeps (CRED_MAX_CREDENTIAL_BLOB_SIZE).
const MAX_BLOB: usize = 5 * 512;

/// A password as keyring keeps it: UTF-16, little-endian.
fn password_blob(password: &str) -> Vec<u8> {
    password
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

/// A password from its blob (keyring's reading of it); None for a blob that
/// is not UTF-16.
fn password_of(blob: &[u8]) -> Option<String> {
    if !blob.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = blob
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes(*p))
        .collect();
    String::from_utf16(&units).ok()
}

/// Overwrites `bytes` so the secret does not stay in freed memory.
fn wipe(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        // SAFETY: a plain write to a byte the slice owns.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
}

/// Stores `password` under `label`, on this PC only.
pub fn write_password(label: &str, password: &str) -> keyring::Result<()> {
    let mut blob = password_blob(password);
    let result = write_secret(label, &blob);
    wipe(&mut blob);
    result
}

/// The password under `label`; `NoEntry` when none is kept.
pub fn read_password(label: &str) -> keyring::Result<String> {
    let mut blob = read_secret(label)?;
    let password = password_of(&blob).ok_or_else(|| keyring::Error::BadEncoding(Vec::new()));
    wipe(&mut blob);
    password
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn os_error() -> keyring::Error {
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        // ERROR_NOT_FOUND
        Some(1168) => keyring::Error::NoEntry,
        // ERROR_NO_SUCH_LOGON_SESSION: no store for this session.
        Some(1312) => keyring::Error::NoStorageAccess(Box::new(err)),
        _ => keyring::Error::PlatformFailure(Box::new(err)),
    }
}

/// Stores `secret` under `label`, on this PC only (CRED_PERSIST_LOCAL_MACHINE),
/// replacing what was kept there.
#[cfg(windows)]
pub fn write_secret(label: &str, secret: &[u8]) -> keyring::Result<()> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };
    if secret.len() > MAX_BLOB {
        return Err(keyring::Error::TooLong("secret".into(), MAX_BLOB as u32));
    }
    let mut target = wide(&format!("{label}.{SERVICE}"));
    let mut user = wide(label);
    let mut comment = wide("AI Limits");
    let mut alias = wide("");
    let mut blob = secret.to_vec();
    // SAFETY: every pointer is to a buffer that lives past the call;
    // CredWriteW copies what it keeps.
    let ok = unsafe {
        let mut c: CREDENTIALW = std::mem::zeroed();
        c.Type = CRED_TYPE_GENERIC;
        c.TargetName = target.as_mut_ptr();
        c.Comment = comment.as_mut_ptr();
        c.CredentialBlobSize = blob.len() as u32;
        c.CredentialBlob = blob.as_mut_ptr();
        c.Persist = CRED_PERSIST_LOCAL_MACHINE;
        c.TargetAlias = alias.as_mut_ptr();
        c.UserName = user.as_mut_ptr();
        CredWriteW(&c, 0)
    };
    let result = if ok == 0 { Err(os_error()) } else { Ok(()) };
    wipe(&mut blob);
    result
}

/// The secret under `label`; `NoEntry` when none is kept. One still kept
/// the way keyring writes (roaming) is written again for this PC only.
#[cfg(windows)]
pub fn read_secret(label: &str) -> keyring::Result<Vec<u8>> {
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_PERSIST_ENTERPRISE, CRED_TYPE_GENERIC,
    };
    let target = wide(&format!("{label}.{SERVICE}"));
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `target` is a NUL-ended wide string; on success `cred` points
    // to memory CredReadW allocated, freed below with CredFree.
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) } == 0 {
        return Err(os_error());
    }
    // SAFETY: `cred` is the valid credential CredReadW returned; its blob is
    // `CredentialBlobSize` bytes (or null when empty).
    let (secret, roaming) = unsafe {
        let c = &*cred;
        let secret = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
        };
        let roaming = c.Persist == CRED_PERSIST_ENTERPRISE;
        CredFree(cred.cast());
        (secret, roaming)
    };
    if roaming {
        keep_local(label, &secret);
    }
    Ok(secret)
}

/// Writes a secret kept the roaming way again for this PC only. A failure
/// changes nothing: the secret stays as it was, and is read as before.
#[cfg(windows)]
fn keep_local(label: &str, secret: &[u8]) {
    if write_secret(label, secret).is_ok() {
        tracing::info!("credential '{label}' now kept on this PC only");
    }
}

#[cfg(not(windows))]
pub fn write_secret(label: &str, secret: &[u8]) -> keyring::Result<()> {
    keyring::Entry::new(SERVICE, label)?.set_secret(secret)
}

#[cfg(not(windows))]
pub fn read_secret(label: &str) -> keyring::Result<Vec<u8>> {
    keyring::Entry::new(SERVICE, label)?.get_secret()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A password is kept as keyring keeps it (UTF-16 little-endian), so
    /// either reads what the other wrote; a blob of odd length is not one.
    #[test]
    fn a_password_is_kept_as_keyring_keeps_it() {
        let blob = password_blob("sk-ä€😀");
        assert_eq!(&blob[..4], &[b's', 0, b'k', 0]);
        assert_eq!(password_of(&blob).as_deref(), Some("sk-ä€😀"));
        assert_eq!(password_of(&[1, 2, 3]), None);
        assert_eq!(password_of(&[]).as_deref(), Some(""));
        let mut b = vec![7u8; 4];
        wipe(&mut b);
        assert_eq!(b, [0; 4]);
    }

    /// Written here and read by keyring, and the other way round, under a
    /// throwaway label: touches the real Credential Manager, so ignored.
    #[test]
    #[ignore]
    fn keyring_and_this_store_read_each_other() {
        let label = "ailimits_selftest";
        write_password(label, "one").unwrap();
        let entry = keyring::Entry::new(SERVICE, label).unwrap();
        assert_eq!(entry.get_password().unwrap(), "one");
        entry.set_password("two").unwrap();
        // Kept the roaming way by keyring: read, and written again locally.
        assert_eq!(read_password(label).unwrap(), "two");
        assert_eq!(read_password(label).unwrap(), "two");
        entry.delete_credential().unwrap();
        assert!(matches!(read_password(label), Err(keyring::Error::NoEntry)));
    }
}
