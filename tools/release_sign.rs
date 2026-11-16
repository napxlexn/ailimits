// tools/release_sign.rs — the release signing key: make it, sign with it,
// check a signature. A Cargo example, so it is never built into the app:
//
//     cargo run --example release-sign -- <command>
//
//   keygen <dir>            make a new key pair in <dir>; refuses to replace
//                           a key that is already there
//   sign <file>...          sign each file into <file>.sig with the private
//                           key, then check each signature with the keys built
//                           into the app (src/updater.rs, RELEASE_KEYS): a
//                           release can only carry signatures the updater
//                           accepts
//   verify <file>...        check <file>.sig with the built-in keys
//   public-key              print the public half of the private key
//
// The private key comes from the AILIMITS_RELEASE_SIGNING_KEY environment
// variable (the release workflow's secret) or from `--key-file <path>`, as
// hex of an Ed25519 PKCS#8 document. It is never printed, logged, or written
// anywhere but the file `keygen` creates.
//
// Setup, once (the owner): the key pair lives outside the repository, in
// %USERPROFILE%\.ailimits-release\ (release-signing-key.hex is the private
// key: back it up offline; release-signing-key.pub.txt the public one, which
// is RELEASE_KEYS in src/updater.rs). The release workflow reads the private
// key from the secret AILIMITS_RELEASE_SIGNING_KEY of the `release`
// environment (a repository secret of that name works too), passed on stdin
// so it never appears on a command line:
//
//     gh api -X PUT repos/napxlexn/ailimits/environments/release
//     Get-Content -Raw "$env:USERPROFILE\.ailimits-release\release-signing-key.hex" |
//         gh secret set AILIMITS_RELEASE_SIGNING_KEY --env release --repo napxlexn/ailimits
//
// A new key: `keygen` into a new folder, add its public key to RELEASE_KEYS
// beside the old one, ship a release signed with the OLD key, then switch the
// secret; drop the old key from RELEASE_KEYS a release later.

use ailimits::updater::{
    encode_signature, hex_decode, hex_lower, key_id, signed_message, verify_release, RELEASE_KEYS,
    SIGNATURE_SUFFIX,
};
use anyhow::{bail, Context, Result};
use ring::digest::{digest, SHA256};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::path::{Path, PathBuf};

/// The environment variable the release workflow hands the secret in.
const KEY_ENV: &str = "AILIMITS_RELEASE_SIGNING_KEY";
/// The private key's file name, as `keygen` writes it.
const KEY_FILE: &str = "release-signing-key.hex";
/// The public key's file name, beside it.
const PUBLIC_FILE: &str = "release-signing-key.pub.txt";

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let key_file = take_option(&mut args, "--key-file")?;
    let Some(command) = args.first().cloned() else {
        usage();
        bail!("no command");
    };
    let rest = &args[1..];
    match command.as_str() {
        "keygen" => {
            let [dir] = rest else {
                bail!("usage: keygen <dir>");
            };
            keygen(Path::new(dir))
        }
        "sign" => {
            if rest.is_empty() {
                bail!("usage: sign <file>...");
            }
            let key = load_key(key_file.as_deref())?;
            for file in rest {
                sign(&key, Path::new(file))?;
            }
            Ok(())
        }
        "verify" => {
            if rest.is_empty() {
                bail!("usage: verify <file>...");
            }
            for file in rest {
                verify(Path::new(file))?;
            }
            Ok(())
        }
        "public-key" => {
            let key = load_key(key_file.as_deref())?;
            print_public(&public(&key));
            Ok(())
        }
        _ => {
            usage();
            bail!("unknown command {command:?}");
        }
    }
}

fn usage() {
    eprintln!("release-sign — the AI Limits release signing key");
    eprintln!();
    eprintln!("  keygen <dir>            make a new key pair in <dir>");
    eprintln!("  sign <file>...          write <file>.sig for each file");
    eprintln!("  verify <file>...        check <file>.sig with the app's built-in keys");
    eprintln!("  public-key              print the public key");
    eprintln!();
    eprintln!("The private key: {KEY_ENV} (hex), or --key-file <path>.");
}

/// Remove `--name <value>` from `args`, returning the value.
fn take_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
    let Some(at) = args.iter().position(|a| a == name) else {
        return Ok(None);
    };
    if at + 1 >= args.len() {
        bail!("{name} needs a value");
    }
    let value = args.remove(at + 1);
    args.remove(at);
    Ok(Some(value))
}

/// The private key, from `--key-file` or the environment. No message here
/// ever carries the key's text.
fn load_key(key_file: Option<&str>) -> Result<Ed25519KeyPair> {
    let text = match key_file {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("read the key file {path}"))?
        }
        None => std::env::var(KEY_ENV).map_err(|_| {
            anyhow::anyhow!(
                "no signing key: {KEY_ENV} is not set (the release secret) and no --key-file was \
                 given — refusing to publish anything unsigned"
            )
        })?,
    };
    let text = text.trim();
    if text.is_empty() {
        bail!("the signing key is empty");
    }
    let pkcs8 = hex_decode(text).context("the signing key is not hex")?;
    Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| anyhow::anyhow!("the signing key is not an Ed25519 PKCS#8 key"))
}

fn public(key: &Ed25519KeyPair) -> [u8; 32] {
    key.public_key()
        .as_ref()
        .try_into()
        .expect("an Ed25519 public key is 32 bytes")
}

/// The public key in every form it is needed in.
fn public_text(public: &[u8; 32]) -> String {
    let mut literal = String::from("[\n");
    for row in public.chunks(16) {
        literal.push_str("    ");
        let bytes: Vec<String> = row.iter().map(|b| format!("0x{b:02x}")).collect();
        literal.push_str(&bytes.join(", "));
        literal.push_str(",\n");
    }
    literal.push(']');
    format!(
        "public key {}\nkey id     {}\nas src/updater.rs RELEASE_KEYS entry:\n{literal}\n",
        hex_lower(public),
        hex_lower(&key_id(public))
    )
}

fn print_public(public: &[u8; 32]) {
    print!("{}", public_text(public));
    if !RELEASE_KEYS.contains(public) {
        println!("(not yet one of the keys this build of the app trusts)");
    }
}

fn keygen(dir: &Path) -> Result<()> {
    let private = dir.join(KEY_FILE);
    if private.exists() {
        bail!(
            "{} already exists — a release key is never replaced by accident",
            private.display()
        );
    }
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| anyhow::anyhow!("could not generate a key"))?;
    let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| anyhow::anyhow!("the new key does not load"))?;
    let public = public(&key);
    // create_new: never overwrite, even if a key appeared meanwhile.
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&private)
        .with_context(|| format!("create {}", private.display()))?;
    file.write_all(format!("{}\n", hex_lower(pkcs8.as_ref())).as_bytes())
        .with_context(|| format!("write {}", private.display()))?;
    std::fs::write(dir.join(PUBLIC_FILE), public_text(&public))
        .with_context(|| format!("write {}", dir.join(PUBLIC_FILE).display()))?;
    println!("private key written to {} (not shown)", private.display());
    print_public(&public);
    Ok(())
}

fn file_name(file: &Path) -> Result<&str> {
    file.file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("{} has no usable file name", file.display()))
}

fn signature_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(SIGNATURE_SUFFIX);
    PathBuf::from(name)
}

fn sign(key: &Ed25519KeyPair, file: &Path) -> Result<()> {
    let name = file_name(file)?;
    let bytes = std::fs::read(file).with_context(|| format!("read {}", file.display()))?;
    let message = signed_message(name, digest(&SHA256, &bytes).as_ref());
    let signature: [u8; 64] = key
        .sign(&message)
        .as_ref()
        .try_into()
        .expect("an Ed25519 signature is 64 bytes");
    let text = encode_signature(&key_id(&public(key)), &signature);
    // The updater's own check, before anything is written: a key the app
    // does not trust (a wrong secret) fails the release here.
    verify_release(name, &bytes, &text).with_context(|| {
        format!(
            "the signature of {name} does not verify with the keys built into the app — is \
             {KEY_ENV} the release key?"
        )
    })?;
    let out = signature_path(file);
    std::fs::write(&out, &text).with_context(|| format!("write {}", out.display()))?;
    println!(
        "signed {name} -> {} (key {})",
        out.display(),
        hex_lower(&key_id(&public(key)))
    );
    Ok(())
}

fn verify(file: &Path) -> Result<()> {
    let name = file_name(file)?;
    let bytes = std::fs::read(file).with_context(|| format!("read {}", file.display()))?;
    let sig = signature_path(file);
    let text = std::fs::read_to_string(&sig).with_context(|| format!("read {}", sig.display()))?;
    verify_release(name, &bytes, &text).with_context(|| format!("{name}: NOT verified"))?;
    println!("{name}: verified");
    Ok(())
}
