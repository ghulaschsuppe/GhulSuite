// SPDX-License-Identifier: AGPL-3.0-or-later
//! Project Wycheproof (C2SP, Apache-2.0): known edge and attack cases for the crypto building
//! blocks exactly as GhulSuite uses them. Every "valid" case must give the expected result,
//! every "invalid" case must be rejected (no forged tag, no point off the curve, ...).

use serde_json::Value as Json;

fn data(name: &str) -> Json {
    let path = format!("{}/tests/data/wycheproof_{name}_test.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// (group, test) pairs of a vector file.
fn cases(d: &Json) -> Vec<(Json, Json)> {
    let mut out = Vec::new();
    for g in d["testGroups"].as_array().unwrap() {
        for t in g["tests"].as_array().unwrap() {
            out.push((g.clone(), t.clone()));
        }
    }
    out
}

fn h(t: &Json, k: &str) -> Vec<u8> {
    unhex(t[k].as_str().unwrap())
}

#[test]
fn xchacha20_poly1305_as_used_by_the_container_format() {
    let mut checked = 0;
    for (g, t) in cases(&data("xchacha20_poly1305")) {
        // The container format uses 256-bit keys, 192-bit nonces, 128-bit tags only.
        if g["keySize"] != 256 || g["ivSize"] != 192 || g["tagSize"] != 128 {
            continue;
        }
        let key: [u8; 32] = h(&t, "key").try_into().unwrap();
        let nonce: [u8; 24] = h(&t, "iv").try_into().unwrap();
        let (aad, msg) = (h(&t, "aad"), h(&t, "msg"));
        let mut sealed = h(&t, "ct");
        sealed.extend(h(&t, "tag"));
        let opened = crate::vault::keys::open(&key, &nonce, &sealed, &aad);
        match t["result"].as_str().unwrap() {
            "valid" => {
                assert_eq!(opened.as_deref().map(|v| v.as_slice()), Some(msg.as_slice()), "tc {}", t["tcId"]);
                assert_eq!(crate::vault::keys::seal(&key, &nonce, &msg, &aad), sealed, "tc {}", t["tcId"]);
            }
            _ => assert!(opened.is_none(), "invalid case accepted: tc {}", t["tcId"]),
        }
        checked += 1;
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn aes_256_gcm_as_used_by_shares() {
    use aes_gcm::aead::{Aead, KeyInit, Payload};
    use aes_gcm::{Aes256Gcm, Nonce};
    let mut checked = 0;
    for (g, t) in cases(&data("aes_gcm")) {
        // Share packages: AES-256-GCM, 96-bit IV, 128-bit tag (as in the browser).
        if g["keySize"] != 256 || g["ivSize"] != 96 || g["tagSize"] != 128 {
            continue;
        }
        let cipher = Aes256Gcm::new_from_slice(&h(&t, "key")).unwrap();
        let iv = h(&t, "iv");
        let (aad, msg) = (h(&t, "aad"), h(&t, "msg"));
        let mut sealed = h(&t, "ct");
        sealed.extend(h(&t, "tag"));
        let opened = cipher.decrypt(Nonce::from_slice(&iv), Payload { msg: &sealed, aad: &aad });
        match t["result"].as_str().unwrap() {
            "valid" => assert_eq!(opened.as_deref().ok(), Some(msg.as_slice()), "tc {}", t["tcId"]),
            _ => assert!(opened.is_err(), "invalid case accepted: tc {}", t["tcId"]),
        }
        checked += 1;
    }
    assert!(checked > 30, "{checked}");
}

#[test]
fn hkdf_sha256() {
    let mut checked = 0;
    for (_, t) in cases(&data("hkdf_sha256")) {
        let size = t["size"].as_u64().unwrap() as usize;
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&h(&t, "salt")), &h(&t, "ikm"));
        let mut okm = vec![0u8; size];
        let r = hk.expand(&h(&t, "info"), &mut okm);
        match t["result"].as_str().unwrap() {
            "valid" => {
                r.unwrap();
                assert_eq!(okm, h(&t, "okm"), "tc {}", t["tcId"]);
            }
            _ => assert!(r.is_err(), "tc {}", t["tcId"]),
        }
        checked += 1;
    }
    // GhulSuite derives with an empty salt (`kdf::hkdf`): same as an all-zero salt of 32 bytes.
    let a = hkdf::Hkdf::<sha2::Sha256>::new(None, b"ikm");
    let b = hkdf::Hkdf::<sha2::Sha256>::new(Some(&[0u8; 32]), b"ikm");
    let (mut x, mut y) = ([0u8; 32], [0u8; 32]);
    a.expand(b"info", &mut x).unwrap();
    b.expand(b"info", &mut y).unwrap();
    assert_eq!(x, y);
    assert!(checked > 50);
}

#[test]
fn hmac_sha256_as_used_by_the_php_module_and_totp() {
    use hmac::{Hmac, Mac};
    let mut checked = 0;
    for (g, t) in cases(&data("hmac_sha256")) {
        let tag_bytes = g["tagSize"].as_u64().unwrap() as usize / 8;
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&h(&t, "key")).unwrap();
        mac.update(&h(&t, "msg"));
        let full = mac.finalize().into_bytes();
        let ok = full[..tag_bytes] == h(&t, "tag")[..];
        assert_eq!(ok, t["result"] == "valid", "tc {}", t["tcId"]);
        checked += 1;
    }
    assert!(checked > 100);
}

#[test]
fn p256_points_from_a_security_key_are_checked() {
    use ciborium::value::Value;
    let mut checked = 0;
    for (_, t) in cases(&data("ecdh_secp256r1_ecpoint")) {
        let public = h(&t, "public");
        // A security key sends x and y; only uncompressed points have that form.
        if public.len() != 65 || public[0] != 4 {
            continue;
        }
        let cose = Value::Map(vec![
            (Value::Integer(1.into()), Value::Integer(2.into())),
            (Value::Integer((-2).into()), Value::Bytes(public[1..33].to_vec())),
            (Value::Integer((-3).into()), Value::Bytes(public[33..].to_vec())),
        ]);
        let parsed = crate::pin::cose_to_public_key(&cose);
        match t["result"].as_str().unwrap() {
            "valid" => {
                let peer = parsed.unwrap_or_else(|e| panic!("tc {}: {e}", t["tcId"]));
                let mut d = h(&t, "private");
                // Private keys may carry a leading zero byte.
                while d.len() > 32 && d[0] == 0 {
                    d.remove(0);
                }
                let mut k = [0u8; 32];
                k[32 - d.len()..].copy_from_slice(&d);
                let secret = p256::SecretKey::from_slice(&k).unwrap();
                let z = p256::ecdh::diffie_hellman(secret.to_nonzero_scalar(), peer.as_affine());
                assert_eq!(z.raw_secret_bytes().as_slice(), h(&t, "shared").as_slice(), "tc {}", t["tcId"]);
            }
            "invalid" => assert!(parsed.is_err(), "point accepted: tc {} {}", t["tcId"], t["comment"]),
            _ => {}
        }
        checked += 1;
    }
    assert!(checked > 20, "{checked}");
}
