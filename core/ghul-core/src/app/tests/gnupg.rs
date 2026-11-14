// SPDX-License-Identifier: AGPL-3.0-or-later
//! OpenPGP interoperability with GnuPG for several key types of the other side. Runs at night:
//!
//! `cargo test -p ghul-core --lib -- --ignored --nocapture gnupg`
//!
//! For each key type GnuPG creates a key ("Bob"), GhulSuite has its own key ("Alice"), and both
//! directions are checked: encrypted and signed messages, and detached signatures as in PGP/MIME.
//! Forged signatures must be refused on both sides. Skipped with a message if gpg is missing.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use crate::app::mailcrypt::{
    decrypt, encrypt_signed, fingerprint, generate, parse_public, public_armored, public_of, secret_key, sign_detached,
    verify_detached,
};

/// A throwaway GnuPG home directory.
struct Gpg {
    home: PathBuf,
}

impl Gpg {
    fn new(tag: &str) -> Gpg {
        let mut id = [0u8; 6];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut id);
        // Short path: gpg-agent's socket path must stay below about 100 bytes.
        let home = std::env::temp_dir().join(format!("ghul-gpg-{tag}-{}", crate::kdf::hex(&id)));
        std::fs::create_dir_all(&home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        Gpg { home }
    }

    fn run(&self, args: &[&str], input: &[u8]) -> Output {
        let mut c = Command::new("gpg")
            .arg("--homedir")
            .arg(&self.home)
            .args(["--batch", "--yes", "--status-fd", "2", "--pinentry-mode", "loopback", "--passphrase", ""])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("gpg does not start");
        use std::io::Write;
        c.stdin.take().unwrap().write_all(input).unwrap();
        c.wait_with_output().unwrap()
    }

    /// Runs gpg and insists on success; returns stdout and the status lines.
    fn ok(&self, args: &[&str], input: &[u8]) -> (Vec<u8>, String) {
        let out = self.run(args, input);
        let status = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(out.status.success(), "gpg {args:?} failed:\n{status}");
        (out.stdout, status)
    }

    fn file(&self, name: &str, data: &[u8]) -> String {
        let p = self.home.join(name);
        std::fs::write(&p, data).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn primary_fingerprint(&self, uid: &str) -> String {
        let (out, _) = self.ok(&["--with-colons", "--list-keys", uid], b"");
        let text = String::from_utf8(out).unwrap();
        text.lines().find(|l| l.starts_with("fpr:")).unwrap().split(':').nth(9).unwrap().to_owned()
    }
}

impl Drop for Gpg {
    fn drop(&mut self) {
        let _ = Command::new("gpgconf").arg("--homedir").arg(&self.home).args(["--kill", "all"]).output();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// Status line `[GNUPG:] VALIDSIG <fpr> ...` names this primary key (fingerprint field 10).
fn valid_sig_by(status: &str, fpr: &str) -> bool {
    status.contains("[GNUPG:] GOODSIG ")
        && status
            .lines()
            .filter(|l| l.starts_with("[GNUPG:] VALIDSIG "))
            .any(|l| l.split(' ').nth(11).is_some_and(|p| p.eq_ignore_ascii_case(fpr)))
}

fn interop_with(gpg_sign: &str, gpg_encrypt: &str) {
    eprintln!("GnuPG key {gpg_sign} (sign) + {gpg_encrypt} (encrypt)");
    let gpg = Gpg::new(gpg_sign);

    // GnuPG user "Bob": primary key for signing, subkey for encryption.
    gpg.ok(&["--quick-gen-key", "Bob <bob@example.org>", gpg_sign, "sign", "never"], b"");
    let bob_fp = gpg.primary_fingerprint("bob@example.org");
    gpg.ok(&["--quick-add-key", &bob_fp, gpg_encrypt, "encr", "never"], b"");
    let (binary, _) = gpg.ok(&["--export", "bob@example.org"], b"");
    let (armored, _) = gpg.ok(&["--armor", "--export", "bob@example.org"], b"");
    let bob_pub = parse_public(&binary).expect("GnuPG's public key (binary) is not readable");
    assert_eq!(fingerprint(&parse_public(&armored).expect("GnuPG's public key (armored)")), fingerprint(&bob_pub));
    assert!(fingerprint(&bob_pub).eq_ignore_ascii_case(&bob_fp));

    // GhulSuite user "Alice", known to GnuPG and locally certified (so signatures count as valid).
    let (alice_sk, alice_fp) = generate("Alice", "alice@example.org").unwrap();
    let alice = secret_key(&alice_sk).unwrap();
    let alice_pub = public_of(&alice);
    gpg.ok(&["--import"], public_armored(&alice_pub).unwrap().as_bytes());
    gpg.ok(&["--quick-lsign-key", &alice_fp], b"");

    let entity =
        "Content-Type: text/plain; charset=utf-8\r\n\r\nGr\u{fc}\u{df}e, \u{263a} und 0x00\0 Ende\r\n".as_bytes();

    // (a) GhulSuite -> GnuPG: encrypted and signed; GnuPG decrypts and checks the signature.
    let msg = encrypt_signed(entity, &alice, std::slice::from_ref(&bob_pub)).unwrap();
    let (plain, status) = gpg.ok(&["--decrypt"], msg.as_bytes());
    assert_eq!(plain, entity);
    assert!(status.contains("[GNUPG:] DECRYPTION_OKAY"), "{status}");
    assert!(valid_sig_by(&status, &alice_fp), "GnuPG did not see Alice's signature:\n{status}");

    // (b) GnuPG -> GhulSuite: encrypted and signed by Bob, armored and binary.
    for armor in [true, false] {
        let mut args =
            vec!["--sign", "--encrypt", "--recipient", "alice@example.org", "--local-user", "bob@example.org"];
        if armor {
            args.insert(0, "--armor");
        }
        let (cipher, _) = gpg.ok(&args, b"Hallo Alice \xe2\x9c\x93");
        let opened = decrypt(&cipher, &alice, std::slice::from_ref(&bob_pub)).expect("GnuPG's message");
        assert_eq!(&opened.plain[..], "Hallo Alice \u{2713}".as_bytes());
        assert!(opened.signed);
        assert_eq!(opened.signer, Some(fingerprint(&bob_pub)), "Bob's signature not verified (armor {armor})");
        // Without Bob's key the signature is seen but not attributed.
        let opened = decrypt(&cipher, &alice, &[]).unwrap();
        assert!(opened.signed && opened.signer.is_none());
    }

    // (c) Detached signatures, as in PGP/MIME, both ways; changed data must fail on both sides.
    let sig = sign_detached(entity, &alice).unwrap();
    let sig_file = gpg.file("alice.sig", sig.as_bytes());
    let data_file = gpg.file("entity.txt", entity);
    let (_, status) = gpg.ok(&["--verify", &sig_file, &data_file], b"");
    assert!(valid_sig_by(&status, &alice_fp), "{status}");
    let changed_file = gpg.file("changed.txt", b"Content-Type: text/plain\r\n\r\nchanged\r\n");
    let out = gpg.run(&["--verify", &sig_file, &changed_file], b"");
    let status = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && status.contains("[GNUPG:] BADSIG"),
        "GnuPG accepted a forged signature:\n{status}"
    );

    let (bob_sig, _) =
        gpg.ok(&["--armor", "--detach-sign", "--local-user", "bob@example.org", "--output", "-", &data_file], b"");
    assert_eq!(verify_detached(entity, &bob_sig, std::slice::from_ref(&bob_pub)), Some(fingerprint(&bob_pub)));
    assert_eq!(verify_detached(b"changed", &bob_sig, std::slice::from_ref(&bob_pub)), None);
    assert_eq!(verify_detached(entity, &bob_sig, std::slice::from_ref(&alice_pub)), None);
    let (bob_sig_binary, _) =
        gpg.ok(&["--detach-sign", "--local-user", "bob@example.org", "--output", "-", &data_file], b"");
    assert_eq!(
        verify_detached(entity, &bob_sig_binary, std::slice::from_ref(&bob_pub)),
        Some(fingerprint(&bob_pub)),
        "binary detached signature"
    );
    eprintln!("  ok");
}

#[test]
#[ignore = "needs gpg: cargo test -p ghul-core -- --ignored gnupg"]
fn gnupg_interop_all_key_types() {
    if cfg!(windows) || Command::new("gpg").arg("--version").output().is_err() {
        eprintln!("gpg is not installed, GnuPG interoperability skipped");
        return;
    }
    let version = Command::new("gpg").arg("--version").output().unwrap().stdout;
    eprintln!("{}", String::from_utf8_lossy(&version).lines().next().unwrap_or("gpg"));
    interop_with("ed25519", "cv25519");
    interop_with("rsa3072", "rsa3072");
    interop_with("nistp256", "nistp256");
}
