#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Independent verification of docs/testvectors/container-v1.json and container-v2.json.

Written only from docs/FORMAT.md with different libraries than ghul-core
(argon2-cffi, cryptography, PyNaCl/libsodium, cbor2), so that the specification,
not just the Rust code, is checked.

    pip install argon2-cffi cryptography pynacl cbor2
    python3 tools/verify_testvectors.py
"""

import hashlib
import json
import struct
import sys
import unicodedata
from pathlib import Path

import cbor2
from argon2.low_level import Type, hash_secret_raw
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from nacl.bindings import crypto_aead_xchacha20poly1305_ietf_decrypt

CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
MAGIC = {b"GHULPASS": "pass", b"GHULTOTP": "totp", b"GHULSUIT": "suite"}


def hkdf(ikm: bytes, info: bytes) -> bytes:
    return HKDF(algorithm=hashes.SHA256(), length=32, salt=None, info=info).derive(ikm)


def aead_open(key: bytes, nonce: bytes, ct: bytes, aad: bytes) -> bytes:
    return crypto_aead_xchacha20poly1305_ietf_decrypt(ct, aad, nonce, key)


def x25519_open(seed: bytes, slot_id: bytes, pub: bytes, eph: bytes, nonce: bytes, ct: bytes, aad: bytes, v: dict) -> bytes:
    """Slot types 4 and 5: the slot key comes from X25519 between the derived private key and the
    ephemeral public key in the slot (FORMAT.md, section 4.2)."""
    x = v["factor"]["x25519"]
    assert seed.hex() == x["private_key"], "private key"
    private = X25519PrivateKey.from_private_bytes(seed)  # clamped as in RFC 7748
    raw = serialization.Encoding.Raw
    assert private.public_key().public_bytes(raw, serialization.PublicFormat.Raw) == pub, "public key"
    assert pub.hex() == x["public_key"] and eph.hex() == x["ephemeral_public_key"]
    shared = private.exchange(X25519PublicKey.from_public_bytes(eph))
    assert shared.hex() == x["shared_secret"], "shared secret"
    slot_key = hkdf(shared + eph + pub, b"ghul/v2/slot/x25519-wrap" + slot_id)
    assert slot_key.hex() == x["slot_key"], "slot key"
    return aead_open(slot_key, nonce, ct, aad)


def parse_container(data: bytes) -> dict:
    kind = MAGIC[data[:8]]
    version, header_len = struct.unpack_from("<HI", data, 8)
    assert version in (1, 2), version
    header = data[14 : 14 + header_len]
    c = {"kind": kind, "aad": data[: 14 + header_len], "slots": []}
    c["nonce"] = data[14 + header_len : 14 + header_len + 24]
    c["ciphertext"] = data[14 + header_len + 24 :]
    pos, last = 0, 0
    while pos < len(header):
        tag, length = struct.unpack_from("<HI", header, pos)
        value = header[pos + 6 : pos + 6 + length]
        pos += 6 + length
        assert tag >= last
        last = tag
        if tag == 0x0001:
            c["container_id"] = value
        elif tag == 0x0002:
            c["suite_id"] = value
        elif tag == 0x0003:
            alg, m, t, p = struct.unpack_from("<BIII", value)
            assert alg == 1 and len(value) == 45
            c["kdf"] = (m, t, p, value[13:45])
        elif tag == 0x0004:
            assert value == b"\x01"
        elif tag == 0x0005:
            c["flags"] = struct.unpack("<I", value)[0]
        elif tag == 0x0010:
            stype, slot_id = value[0], value[1:17]
            assert stype in (1, 2, 3) or version == 2, "slot types 4 and 5 need format version 2"
            if stype in (1, 4):
                rp_len = value[17]
                rp_id = value[18 : 18 + rp_len].decode()
                q = 18 + rp_len
                (cred_len,) = struct.unpack_from("<H", value, q)
                cred = value[q + 2 : q + 2 + cred_len]
                q += 2 + cred_len
                prf_input, options = value[q : q + 32], value[q + 32]
                assert options == 1
                q += 33
                pub = eph = None
                if stype == 4:
                    pub, eph = value[q : q + 32], value[q + 32 : q + 64]
                    q += 64
                nonce, ct = value[q : q + 24], value[q + 24 : q + 72]
                assert len(value) == q + 72
                c["slots"].append(("fido2", slot_id, rp_id, cred, prf_input, nonce, ct, pub, eph))
            elif stype == 2:
                assert len(value) == 1 + 16 + 72
                c["slots"].append(("recovery", slot_id, value[17:41], value[41:89]))
            elif stype == 3:
                assert len(value) == 1 + 16 + 8 + 72
                c["slots"].append(("keyfile", slot_id, value[17:25], value[25:49], value[49:97], None, None))
            elif stype == 5:
                assert len(value) == 1 + 16 + 8 + 64 + 72
                pub, eph = value[25:57], value[57:89]
                c["slots"].append(("keyfile", slot_id, value[17:25], value[89:113], value[113:161], pub, eph))
            else:
                raise ValueError("unknown slot type")
        elif tag == 0x0020:
            c["vmk"] = (value[:24], value[24:72])
        else:
            assert tag & 0x8000, hex(tag)
    return c


def parse_recovery_code(text: str) -> bytes:
    vals = []
    for ch in text.upper():
        if ch in "- ":
            continue
        ch = {"O": "0", "I": "1", "L": "1"}.get(ch, ch)
        vals.append(CROCKFORD.index(ch))
    assert len(vals) == 55
    n = 0
    for v in vals:
        n = (n << 5) | v
    assert n & 0b111 == 0  # 3 padding bits
    raw = (n >> 3).to_bytes(34, "big")
    secret, check = raw[:32], raw[32:]
    assert hashlib.sha256(b"ghul/v1/recovery-check" + secret).digest()[:2] == check
    return secret


def verify(v: dict) -> None:
    h = bytes.fromhex
    data = h(v["file"])
    c = parse_container(data)
    cid, sid = c["container_id"], c["suite_id"]
    assert c["kind"] == v["kind"]
    assert cid.hex() == v["container_id"] and sid.hex() == v["suite_id"]

    # Password key (Argon2id v19, NFC-normalized password)
    m, t, p, salt = c["kdf"]
    pw = unicodedata.normalize("NFC", v["password"]).encode()
    pk = hash_secret_raw(pw, salt, time_cost=t, memory_cost=m, parallelism=p, hash_len=32, type=Type.ID, version=19)
    assert pk.hex() == v["password_key"], "password key"

    # Second factor: FIDO2 slot or key file slot -> factor secret S
    ftype = v["factor"]["type"]
    if ftype in ("fido2", "fido2-x25519"):
        fido = next(x for x in c["slots"] if x[0] == "fido2")
        _, slot_id, rp_id, cred, prf_input, nonce, ct, pub, eph = fido
        assert rp_id == v["factor"]["rp_id"] and cred.hex() == v["factor"]["credential_id"]
        hmac_salt = hashlib.sha256(b"WebAuthn PRF\x00" + prf_input).digest()
        assert hmac_salt.hex() == v["factor"]["hmac_salt"], "hmac salt"
        hmac_output = h(v["factor"]["hmac_output"])
        aad = b"ghul/v1/factor" + sid + slot_id
        if ftype == "fido2":
            assert pub is None
            slot_key = hkdf(hmac_output, b"ghul/v1/slot/fido2" + slot_id)
            assert slot_key.hex() == v["factor"]["slot_key"]
            s = aead_open(slot_key, nonce, ct, aad)
        else:
            seed = hkdf(hmac_output, b"ghul/v2/slot/fido2-x25519" + slot_id)
            s = x25519_open(seed, slot_id, pub, eph, nonce, ct, aad, v)
    else:
        assert ftype in ("keyfile", "keyfile-x25519")
        kf = h(v["keyfile"])
        assert len(kf) == 46 and kf[:8] == b"GHULKEY\x00" and kf[8:10] == b"\x01\x00"
        assert hashlib.sha256(kf[:42]).digest()[:4] == kf[42:46]
        secret = kf[10:42]
        _, slot_id, keyfile_id, nonce, ct, pub, eph = next(x for x in c["slots"] if x[0] == "keyfile")
        assert hashlib.sha256(b"ghul/v1/keyfile-id" + secret).digest()[:8] == keyfile_id
        assert keyfile_id.hex() == v["factor"]["keyfile_id"]
        aad = b"ghul/v1/factor" + sid + slot_id
        if ftype == "keyfile":
            assert pub is None
            slot_key = hkdf(secret, b"ghul/v1/slot/keyfile" + slot_id)
            assert slot_key.hex() == v["factor"]["slot_key"]
            s = aead_open(slot_key, nonce, ct, aad)
        else:
            seed = hkdf(secret, b"ghul/v2/slot/keyfile-x25519" + slot_id)
            s = x25519_open(seed, slot_id, pub, eph, nonce, ct, aad, v)
    assert s.hex() == v["factor_secret"], "factor secret"
    assert c["flags"] == 0

    kek = hkdf(pk + s, b"ghul/v1/kek" + cid)
    assert kek.hex() == v["kek"], "KEK"
    vmk = aead_open(kek, c["vmk"][0], c["vmk"][1], b"ghul/v1/vmk" + cid)
    assert vmk.hex() == v["vmk"], "VMK"
    pkey = hkdf(vmk, b"ghul/v1/payload" + cid)
    assert pkey.hex() == v["payload_key"]

    plain = aead_open(pkey, c["nonce"], c["ciphertext"], c["aad"])
    (length,) = struct.unpack_from("<I", plain)
    assert len(plain) == max(4096, 1 << (length + 4 - 1).bit_length())
    assert not any(plain[4 + length :]), "padding"
    payload = plain[4 : 4 + length]
    assert payload.hex() == v["payload_cbor"]
    doc = cbor2.loads(payload)
    assert doc["schema"] == 1 and doc["kind"] == v["kind"] and doc["revision"] == 1
    assert doc["factor_secret"] == s, "payload keeps a copy of S"
    if v["recovery"]:
        assert doc["recovery_secret"] == h(v["recovery"]["code_secret"]), "payload keeps the suite's code"
    else:
        assert "recovery_secret" not in doc

    # Recovery code alone -> container key (no password, key or key file)
    rec_slots = [x for x in c["slots"] if x[0] == "recovery"]
    if v["recovery"]:
        code = parse_recovery_code(v["recovery"]["code"])
        assert code.hex() == v["recovery"]["code_secret"]
        rec = rec_slots[0]
        rkey = hkdf(code, b"ghul/v1/slot/recovery" + rec[1] + cid)
        assert rkey.hex() == v["recovery"]["slot_key"]
        assert aead_open(rkey, rec[2], rec[3], b"ghul/v1/recovery" + cid + rec[1]) == vmk
    else:
        assert not rec_slots
    print(f"ok  {v['description']}: {len(data)} bytes")


def main() -> int:
    total = 0
    for version in (1, 2):
        path = Path(__file__).resolve().parent.parent / "docs" / "testvectors" / f"container-v{version}.json"
        vectors = json.loads(path.read_text())["vectors"]
        for v in vectors:
            assert struct.unpack_from("<H", bytes.fromhex(v["file"]), 8)[0] == version
            verify(v)
        total += len(vectors)
    print(f"{total} test vectors verified independently")
    return 0


if __name__ == "__main__":
    sys.exit(main())
