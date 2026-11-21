// updater/signature.rs — release signatures: the keys this build trusts,
// what a signature signs and how its file reads.
//
// One file for both sides: the app checks an installer with it
// (updater.rs), and the release signer (tools/release-signer, which includes
// this very file) signs with it and checks each signature against these
// keys before writing it, so the two can never disagree. It needs only ring
// and std, so the signer is built from ring alone.

use ring::digest::{digest, SHA256};
use ring::signature::{UnparsedPublicKey, ED25519};

/// The release signing keys this build trusts: Ed25519 public keys. A
/// signature names its key by `key_id`, so a later key can be added here
/// (and the old one dropped a release after) without breaking an update.
/// The private key is never in the repository: the owner keeps it, and the
/// release workflow signs with it from the `AILIMITS_RELEASE_SIGNING_KEY`
/// secret (tools/release-signer).
pub const RELEASE_KEYS: &[[u8; 32]] = &[
    // Key id 1507bd92b1a4bb7b, made 2026-11-16.
    [
        0x86, 0x91, 0x88, 0x60, 0x23, 0x42, 0x6a, 0x4f, 0x88, 0x27, 0x79, 0xd7, 0x54, 0xfe, 0xaf,
        0x85, 0x0e, 0x97, 0x7d, 0x5e, 0x70, 0x71, 0x00, 0x13, 0xc0, 0x92, 0xe4, 0xac, 0xa0, 0x2c,
        0x0d, 0xd9,
    ],
];

/// What a signature asset's name adds to the signed file's:
/// `AiLimits-Setup-0.7.1.exe.sig`.
pub const SIGNATURE_SUFFIX: &str = ".sig";

/// The first word of a signature file: its format, so a later one can be told
/// apart.
const SIGNATURE_FORMAT: &str = "ailimits-signature-v1";

/// What a release signature signs: the published file's name and the SHA-256
/// of its bytes. The name carries the version, so a signed installer cannot
/// be passed off as another version.
pub fn signed_message(file_name: &str, sha256: &[u8]) -> Vec<u8> {
    format!(
        "AI Limits release signature v1\nfile {file_name}\nsha256 {}\n",
        hex_lower(sha256)
    )
    .into_bytes()
}

/// A key's id: the first 8 bytes of the SHA-256 of the public key.
pub fn key_id(public_key: &[u8; 32]) -> [u8; 8] {
    let mut id = [0u8; 8];
    id.copy_from_slice(&digest(&SHA256, public_key).as_ref()[..8]);
    id
}

/// The text of a signature file: format, key id and signature, one line.
pub fn encode_signature(key: &[u8; 8], signature: &[u8; 64]) -> String {
    format!(
        "{SIGNATURE_FORMAT} {} {}\n",
        hex_lower(key),
        hex_lower(signature)
    )
}

/// A signature file, read.
pub struct ReleaseSignature {
    key: [u8; 8],
    signature: [u8; 64],
}

impl ReleaseSignature {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parts = text.split_whitespace();
        let (Some(format), Some(key), Some(signature), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err("not a release signature".into());
        };
        if format != SIGNATURE_FORMAT {
            return Err(format!("unknown release signature format {format:?}"));
        }
        let key = hex_decode(key)
            .and_then(|k| <[u8; 8]>::try_from(k).ok())
            .ok_or("release signature has no readable key id")?;
        let signature = hex_decode(signature)
            .and_then(|s| <[u8; 64]>::try_from(s).ok())
            .ok_or("release signature is not readable")?;
        Ok(Self { key, signature })
    }
}

/// Check `bytes`, published as `file_name`, against the text of its
/// signature file, with `keys`.
pub fn verify_with_keys(
    keys: &[[u8; 32]],
    file_name: &str,
    bytes: &[u8],
    signature: &str,
) -> Result<(), String> {
    let signature = ReleaseSignature::parse(signature)?;
    let key = keys
        .iter()
        .find(|k| key_id(k) == signature.key)
        .ok_or("signed with a key this build does not trust")?;
    let message = signed_message(file_name, digest(&SHA256, bytes).as_ref());
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&message, &signature.signature)
        .map_err(|_| format!("the signature does not match {file_name}"))
}

/// Lower-case hex of `bytes`.
pub fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Bytes from hex (either case); None on an odd length or a non-hex digit.
pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let (pairs, odd) = text.as_bytes().as_chunks::<2>();
    if !odd.is_empty() {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    pairs
        .iter()
        .map(|&[high, low]| Some((nibble(high)? << 4) | nibble(low)?))
        .collect()
}
