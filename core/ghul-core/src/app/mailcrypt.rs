// SPDX-License-Identifier: AGPL-3.0-or-later
//! OpenPGP for GhulMail, made to need no knowledge from the user.
//!
//! - Every account gets its own key (Ed25519 for signing, X25519 for encryption, OpenPGP v4,
//!   readable by Thunderbird, GnuPG, Proton and others). It lives only in the encrypted suite.
//! - Every outgoing mail is signed (PGP/MIME, RFC 3156) and carries the sender's public key in an
//!   `Autocrypt` header, so replies can be encrypted without any key exchange by hand.
//! - A mail is encrypted when a key is known for every recipient (learned from their mails, from
//!   their provider via Web Key Directory, or imported). The subject travels inside the
//!   encryption ("protected headers"); outside it reads "...".
//! - Only Curve25519 secret keys are ever created or held. Keys of contacts may use other
//!   algorithms (RSA, NIST curves) but only their public operations are used.

use std::io::Cursor;

use base64::Engine;
use pgp::composed::{
    ArmorOptions, Deserializable, DetachedSignature, EncryptionCaps, KeyType, Message, MessageBuilder,
    SecretKeyParamsBuilder, SignedPublicKey, SignedSecretKey, SubkeyParamsBuilder,
};
use pgp::crypto::ecc_curve::ECCCurve;
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::packet::PublicSubkey;
use pgp::ser::Serialize as PgpSerialize;
use pgp::types::CompressionAlgorithm;
use pgp::types::{KeyDetails, Password};
use rand_core::OsRng;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};

fn err(e: impl std::fmt::Display) -> GhulError {
    GhulError::InvalidInput { msg: format!("OpenPGP: {e}") }
}

/// A new key for an account: armored secret key (kept in the suite) and its fingerprint.
pub(crate) fn generate(name: &str, email: &str) -> Result<(Zeroizing<String>, String)> {
    let uid = if name.trim().is_empty() {
        format!("<{}>", email.trim())
    } else {
        format!("{} <{}>", name.replace(['<', '>'], "").trim(), email.trim())
    };
    let mut enc = SubkeyParamsBuilder::default();
    enc.key_type(KeyType::ECDH(ECCCurve::Curve25519Legacy))
        .can_sign(false)
        .can_encrypt(EncryptionCaps::All)
        .can_authenticate(false);
    let mut params = SecretKeyParamsBuilder::default();
    params
        .key_type(KeyType::Ed25519Legacy)
        .can_certify(true)
        .can_sign(true)
        .can_encrypt(EncryptionCaps::None)
        .primary_user_id(uid)
        // Stated preferences: without them other programs fall back to SHA-1 and 3DES.
        .preferred_hash_algorithms(vec![HashAlgorithm::Sha512, HashAlgorithm::Sha256, HashAlgorithm::Sha384].into())
        .preferred_symmetric_algorithms(vec![SymmetricKeyAlgorithm::AES256, SymmetricKeyAlgorithm::AES128].into())
        .preferred_compression_algorithms(vec![CompressionAlgorithm::ZLIB, CompressionAlgorithm::Uncompressed].into())
        .subkeys(vec![enc.build().map_err(err)?]);
    let key = params.build().map_err(err)?.generate(OsRng).map_err(err)?;
    let fp = fingerprint_secret(&key);
    let armored = key.to_armored_string(ArmorOptions::default()).map_err(err)?;
    Ok((Zeroizing::new(armored), fp))
}

pub(crate) fn secret_key(armored: &str) -> Result<SignedSecretKey> {
    let (key, _) = SignedSecretKey::from_string(armored).map_err(err)?;
    Ok(key)
}

pub(crate) fn public_of(secret: &SignedSecretKey) -> SignedPublicKey {
    SignedPublicKey::from(secret.clone())
}

fn hex_upper(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

pub(crate) fn fingerprint_secret(k: &SignedSecretKey) -> String {
    hex_upper(k.primary_key.fingerprint().as_bytes())
}

pub(crate) fn fingerprint(k: &SignedPublicKey) -> String {
    hex_upper(k.primary_key.fingerprint().as_bytes())
}

/// Fingerprint in groups of four for display: "1A2B 3C4D …".
pub(crate) fn pretty_fingerprint(fp: &str) -> String {
    fp.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" ")
}

pub(crate) fn public_bytes(k: &SignedPublicKey) -> Result<Vec<u8>> {
    k.to_bytes().map_err(err)
}

pub(crate) fn public_armored(k: &SignedPublicKey) -> Result<String> {
    k.to_armored_string(ArmorOptions::default()).map_err(err)
}

/// Reads a public key (binary or armored) and checks its self-signatures.
pub(crate) fn parse_public(data: &[u8]) -> Result<SignedPublicKey> {
    let (key, _) = SignedPublicKey::from_reader_single(Cursor::new(data)).map_err(err)?;
    key.verify_bindings().map_err(err)?;
    if encryption_subkey(&key).is_none() && !key.primary_key.algorithm().can_encrypt() {
        return Err(err("the key cannot encrypt"));
    }
    Ok(key)
}

/// The e-mail addresses in the key's user IDs (lower case).
pub(crate) fn emails(k: &SignedPublicKey) -> Vec<String> {
    k.details
        .users
        .iter()
        .filter_map(|u| {
            let id = String::from_utf8_lossy(u.id.id()).into_owned();
            let addr = match (id.rfind('<'), id.rfind('>')) {
                (Some(a), Some(b)) if b > a => id[a + 1..b].to_owned(),
                _ => id.clone(),
            };
            let addr = addr.trim().to_lowercase();
            addr.contains('@').then_some(addr)
        })
        .collect()
}

/// A subkey that may encrypt (key flags) and is not revoked; the newest one wins.
fn encryption_subkey(k: &SignedPublicKey) -> Option<&PublicSubkey> {
    k.public_subkeys
        .iter()
        .filter(|s| s.key.algorithm().can_encrypt())
        .filter(|s| {
            s.signatures.iter().any(|sig| {
                let f = sig.key_flags();
                f.encrypt_comms() || f.encrypt_storage()
            })
        })
        .max_by_key(|s| s.key.created_at())
        .map(|s| &s.key)
}

/// Signs the MIME entity (as bytes, CRLF line ends) and returns the armored detached signature.
pub(crate) fn sign_detached(entity: &[u8], key: &SignedSecretKey) -> Result<String> {
    let sig =
        DetachedSignature::sign_binary_data(OsRng, &key.primary_key, &Password::empty(), HashAlgorithm::Sha256, entity)
            .map_err(err)?;
    sig.to_armored_string(ArmorOptions::default()).map_err(err)
}

/// Encrypts and signs the MIME entity for the recipients (and the sender, to read it in "Sent").
pub(crate) fn encrypt_signed(entity: &[u8], key: &SignedSecretKey, recipients: &[SignedPublicKey]) -> Result<String> {
    let own = public_of(key);
    let mut targets: Vec<&PublicSubkey> = Vec::new();
    for r in recipients.iter().chain(std::iter::once(&own)) {
        targets.push(encryption_subkey(r).ok_or_else(|| err("a recipient key cannot encrypt"))?);
    }
    let mut builder = MessageBuilder::from_bytes("", entity.to_vec()).seipd_v1(OsRng, SymmetricKeyAlgorithm::AES256);
    for t in targets {
        builder.encrypt_to_key(OsRng, t).map_err(err)?;
    }
    builder.sign(&key.primary_key, Password::empty(), HashAlgorithm::Sha256);
    builder.to_armored_string(OsRng, ArmorOptions::default()).map_err(err)
}

/// Result of opening an encrypted message.
pub(crate) struct Opened {
    pub plain: Zeroizing<Vec<u8>>,
    /// Fingerprint of the key whose signature inside was valid, if any.
    pub signer: Option<String>,
    pub signed: bool,
}

/// Decrypts an armored or binary OpenPGP message with the account key and checks the signature
/// inside against the given keys.
pub(crate) fn decrypt(data: &[u8], key: &SignedSecretKey, signers: &[SignedPublicKey]) -> Result<Opened> {
    let text = String::from_utf8_lossy(data);
    let msg = if text.contains("-----BEGIN PGP MESSAGE-----") {
        let start = text.find("-----BEGIN PGP MESSAGE-----").unwrap_or(0);
        let end =
            text.find("-----END PGP MESSAGE-----").map(|e| e + "-----END PGP MESSAGE-----".len()).unwrap_or(text.len());
        Message::from_string(&text[start..end]).map_err(err)?.0
    } else {
        Message::from_bytes(Cursor::new(data.to_vec())).map_err(err)?
    };
    let mut m =
        msg.decrypt(&Password::empty(), key).map_err(|_| GhulError::InvalidInput { msg: "cannot decrypt".into() })?;
    if m.is_compressed() {
        m = m.decompress().map_err(err)?;
    }
    let plain = Zeroizing::new(m.as_data_vec().map_err(err)?);
    let signed = m.is_signed() || m.is_one_pass_signed();
    let mut signer = None;
    if signed {
        for k in signers {
            if verifies_message(&m, k) {
                signer = Some(fingerprint(k));
                break;
            }
        }
    }
    Ok(Opened { plain, signer, signed })
}

fn verifies_message(m: &Message<'_>, k: &SignedPublicKey) -> bool {
    if m.verify(&k.primary_key).is_ok() {
        return true;
    }
    k.public_subkeys.iter().any(|s| m.verify(&s.key).is_ok())
}

/// Checks a detached signature (armored) over `entity`; returns the signer's fingerprint.
pub(crate) fn verify_detached(entity: &[u8], signature: &[u8], keys: &[SignedPublicKey]) -> Option<String> {
    let (sig, _) = DetachedSignature::from_reader_single(Cursor::new(signature)).ok()?;
    for k in keys {
        if sig.verify(&k.primary_key, entity).is_ok()
            || k.public_subkeys.iter().any(|s| sig.verify(&s.key, entity).is_ok())
        {
            return Some(fingerprint(k));
        }
    }
    None
}

// --- Autocrypt (https://autocrypt.org/level1.html) -----------------------------------------------

/// `Autocrypt` header value for the account's key.
pub(crate) fn autocrypt_header(email: &str, key: &SignedPublicKey) -> Result<String> {
    let data = base64::engine::general_purpose::STANDARD.encode(public_bytes(key)?);
    // Spaces are folding points; readers remove all whitespace from keydata.
    let folded: Vec<String> = data.as_bytes().chunks(72).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
    Ok(format!("addr={}; prefer-encrypt=mutual; keydata={}", email.trim(), folded.join(" ")))
}

/// Address and key from an `Autocrypt` header (unknown critical attributes make it invalid).
pub(crate) fn parse_autocrypt(value: &str) -> Option<(String, SignedPublicKey)> {
    let mut addr = None;
    let mut keydata = None;
    for attr in value.split(';') {
        let (k, v) = attr.split_once('=')?;
        let k = k.trim();
        match k {
            "addr" => addr = Some(v.trim().to_lowercase()),
            "keydata" => keydata = Some(v.split_whitespace().collect::<String>()),
            "prefer-encrypt" => {}
            other if other.starts_with('_') => {}
            _ => return None,
        }
    }
    let data = base64::engine::general_purpose::STANDARD.decode(keydata?).ok()?;
    let key = parse_public(&data).ok()?;
    let addr = addr?;
    emails(&key).contains(&addr).then_some((addr, key))
}

// --- Web Key Directory (draft-koch-openpgp-webkey-service) ---------------------------------------

/// z-base-32 of SHA-1 of the lower-case local part, as WKD names the key file.
pub(crate) fn wkd_hash(local: &str) -> String {
    use sha1::Digest as _;
    let digest = sha1::Sha1::digest(local.to_lowercase().as_bytes());
    const ALPHABET: &[u8] = b"ybndrfg8ejkmcpqxot1uwisza345h769";
    let mut out = String::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for b in digest {
        buffer = (buffer << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// The two WKD addresses of an e-mail address (advanced first, then direct).
pub(crate) fn wkd_urls(email: &str) -> Option<[String; 2]> {
    let (local, domain) = email.trim().rsplit_once('@')?;
    let domain = domain.to_lowercase();
    if local.is_empty() || domain.is_empty() || domain.contains('/') {
        return None;
    }
    let hash = wkd_hash(local);
    let l = crate::net::phpfs::rawurlencode(local);
    Some([
        format!("https://openpgpkey.{domain}/.well-known/openpgpkey/{domain}/hu/{hash}?l={l}"),
        format!("https://{domain}/.well-known/openpgpkey/hu/{hash}?l={l}"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkd_hash_matches_the_specification() {
        // Example from the WKD draft: "Joe.Doe" -> iy9q119eutrkn8s1mk4r39qejnbu3n5q
        assert_eq!(wkd_hash("Joe.Doe"), "iy9q119eutrkn8s1mk4r39qejnbu3n5q");
        let [a, d] = wkd_urls("Joe.Doe@Example.ORG").unwrap();
        assert_eq!(a, "https://openpgpkey.example.org/.well-known/openpgpkey/example.org/hu/iy9q119eutrkn8s1mk4r39qejnbu3n5q?l=Joe.Doe");
        assert_eq!(d, "https://example.org/.well-known/openpgpkey/hu/iy9q119eutrkn8s1mk4r39qejnbu3n5q?l=Joe.Doe");
    }

    #[test]
    fn sign_encrypt_decrypt_verify() {
        let (alice_sk, alice_fp) = generate("Alice Ä.", "alice@example.org").unwrap();
        let (bob_sk, _) = generate("Bob", "bob@example.org").unwrap();
        let alice = secret_key(&alice_sk).unwrap();
        let bob = secret_key(&bob_sk).unwrap();
        assert_eq!(fingerprint_secret(&alice), alice_fp);
        assert_eq!(alice_fp.len(), 40);
        let alice_pub = parse_public(&public_bytes(&public_of(&alice)).unwrap()).unwrap();
        let bob_pub = public_of(&bob);
        assert_eq!(emails(&alice_pub), vec!["alice@example.org".to_string()]);

        let entity = b"Content-Type: text/plain; charset=utf-8\r\n\r\nGeheim \xc3\xa4\r\n";
        let sig = sign_detached(entity, &alice).unwrap();
        assert_eq!(
            verify_detached(entity, sig.as_bytes(), &[bob_pub.clone(), alice_pub.clone()]),
            Some(alice_fp.clone())
        );
        assert_eq!(verify_detached(b"changed", sig.as_bytes(), std::slice::from_ref(&alice_pub)), None);

        let msg = encrypt_signed(entity, &alice, std::slice::from_ref(&bob_pub)).unwrap();
        let opened = decrypt(msg.as_bytes(), &bob, std::slice::from_ref(&alice_pub)).unwrap();
        assert_eq!(&opened.plain[..], entity);
        assert_eq!(opened.signer, Some(alice_fp.clone()));
        // The sender can read her own copy, a stranger cannot.
        assert!(decrypt(msg.as_bytes(), &alice, &[]).is_ok());
        let (eve_sk, _) = generate("", "eve@example.org").unwrap();
        assert!(decrypt(msg.as_bytes(), &secret_key(&eve_sk).unwrap(), &[]).is_err());

        let header = autocrypt_header("alice@example.org", &alice_pub).unwrap();
        let (addr, key) = parse_autocrypt(&header).unwrap();
        assert_eq!((addr.as_str(), fingerprint(&key)), ("alice@example.org", alice_fp));
        // A header for another address than the key's is ignored.
        assert!(parse_autocrypt(&header.replace("addr=alice@", "addr=mallory@")).is_none());
        assert!(parse_autocrypt(&format!("{header}; critical=1")).is_none());
    }

    /// Interoperability with GnuPG in both directions (skipped if gpg is not installed).
    #[test]
    fn interoperates_with_gnupg() {
        use std::process::{Command, Stdio};
        // On Windows only Git's gpg with its own path rules may be around; checked on Linux.
        if cfg!(windows) || Command::new("gpg").arg("--version").output().is_err() {
            return;
        }
        let run = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let home = std::env::temp_dir().join(format!("ghul-gpg-{run}"));
        std::fs::create_dir_all(&home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let gpg = |args: &[&str], input: &[u8]| {
            let mut c = Command::new("gpg")
                .arg("--homedir")
                .arg(&home)
                .args(["--batch", "--yes", "--pinentry-mode", "loopback", "--passphrase", ""])
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            use std::io::Write;
            c.stdin.take().unwrap().write_all(input).unwrap();
            c.wait_with_output().unwrap()
        };
        // GnuPG user "Bob" with Ed25519 signing and Cv25519 encryption key.
        assert!(gpg(&["--quick-gen-key", "Bob <bob@example.org>", "ed25519", "sign", "never"], b"").status.success());
        let bob_fpr = String::from_utf8(gpg(&["--list-keys", "--with-colons", "bob@example.org"], b"").stdout).unwrap();
        let bob_fpr = bob_fpr.lines().find(|l| l.starts_with("fpr:")).unwrap().split(':').nth(9).unwrap().to_owned();
        assert!(gpg(&["--quick-add-key", &bob_fpr, "cv25519", "encr", "never"], b"").status.success());
        let bob_pub = parse_public(&gpg(&["--export", "bob@example.org"], b"").stdout).unwrap();

        // GhulSuite user "Alice".
        let (alice_sk, alice_fp) = generate("Alice", "alice@example.org").unwrap();
        let alice = secret_key(&alice_sk).unwrap();
        let alice_pub_armored = public_armored(&public_of(&alice)).unwrap();
        assert!(gpg(&["--import"], alice_pub_armored.as_bytes()).status.success());
        assert!(gpg(&["--quick-lsign-key", &alice_fp], b"").status.success());

        // GhulSuite -> GnuPG: encrypted and signed.
        let entity = "Content-Type: text/plain; charset=utf-8\r\n\r\nGrüße an Bob\r\n".as_bytes();
        let msg = encrypt_signed(entity, &alice, std::slice::from_ref(&bob_pub)).unwrap();
        let out = gpg(&["--status-fd", "2", "--decrypt"], msg.as_bytes());
        let status = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{status}");
        assert_eq!(out.stdout, entity);
        assert!(status.contains("GOODSIG") && status.contains("VALIDSIG"), "{status}");

        // GhulSuite -> GnuPG: detached signature as in PGP/MIME.
        let sig = sign_detached(entity, &alice).unwrap();
        let sig_file = home.join("entity.sig");
        let data_file = home.join("entity.txt");
        std::fs::write(&sig_file, &sig).unwrap();
        std::fs::write(&data_file, entity).unwrap();
        let out = gpg(&["--status-fd", "2", "--verify", sig_file.to_str().unwrap(), data_file.to_str().unwrap()], b"");
        assert!(String::from_utf8_lossy(&out.stderr).contains("GOODSIG"));

        // GnuPG -> GhulSuite: encrypted and signed by Bob.
        let out = gpg(
            &["--armor", "--sign", "--encrypt", "--recipient", "alice@example.org", "--local-user", "bob@example.org"],
            b"Hallo Alice",
        );
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let opened = decrypt(&out.stdout, &alice, std::slice::from_ref(&bob_pub)).unwrap();
        assert_eq!(&opened.plain[..], b"Hallo Alice");
        assert_eq!(opened.signer, Some(fingerprint(&bob_pub)));

        // GnuPG -> GhulSuite: detached signature.
        std::fs::write(&data_file, entity).unwrap();
        let out = gpg(
            &[
                "--armor",
                "--detach-sign",
                "--local-user",
                "bob@example.org",
                "--output",
                "-",
                data_file.to_str().unwrap(),
            ],
            b"",
        );
        assert_eq!(verify_detached(entity, &out.stdout, std::slice::from_ref(&bob_pub)), Some(fingerprint(&bob_pub)));
        let _ = std::fs::remove_dir_all(&home);
    }
}
