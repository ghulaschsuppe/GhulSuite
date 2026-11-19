#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Independent verification of docs/testvectors/container-v3.json and
docs/testvectors/docs-blob-v1.json (GhulDocs content files).

Written only from docs/FORMAT.md with different libraries than ghul-core
(argon2-cffi, cryptography, PyNaCl/libsodium, cbor2), so that the specification,
not just the Rust code, is checked.

    pip install argon2-cffi cryptography pynacl cbor2
    python3 tools/verify_testvectors.py
"""

import hashlib
import hmac
import json
import struct
import sys
import unicodedata
import zlib
from pathlib import Path

import cbor2
from argon2.low_level import Type, hash_secret_raw
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from nacl.bindings import crypto_aead_xchacha20poly1305_ietf_decrypt, crypto_aead_xchacha20poly1305_ietf_encrypt

CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
MAGIC = {b"GHULPASS": "pass", b"GHULTOTP": "totp", b"GHULSUIT": "suite", b"GHULDOCS": "docs"}
AUTH_LEN = 32


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
    assert version == 3, version
    header = data[14 : 14 + header_len]
    c = {"kind": kind, "aad": data[: 14 + header_len], "slots": []}
    c["nonce"] = data[14 + header_len : 14 + header_len + 24]
    # Version 3 ends with the write tag (HMAC-SHA256, FORMAT.md section 2).
    c["ciphertext"] = data[14 + header_len + 24 : -AUTH_LEN]
    c["signed"], c["auth"] = data[:-AUTH_LEN], data[-AUTH_LEN:]
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
            assert stype in (2, 4, 5), "version 3 only knows slot types 2, 4 and 5"
            if stype == 4:
                rp_len = value[17]
                rp_id = value[18 : 18 + rp_len].decode()
                q = 18 + rp_len
                (cred_len,) = struct.unpack_from("<H", value, q)
                cred = value[q + 2 : q + 2 + cred_len]
                q += 2 + cred_len
                prf_input, options = value[q : q + 32], value[q + 32]
                assert options == 1
                q += 33
                pub, eph = value[q : q + 32], value[q + 32 : q + 64]
                q += 64
                nonce, ct = value[q : q + 24], value[q + 24 : q + 72]
                assert len(value) == q + 72
                c["slots"].append(("fido2", slot_id, rp_id, cred, prf_input, nonce, ct, pub, eph))
            elif stype == 2:
                assert len(value) == 1 + 16 + 72
                c["slots"].append(("recovery", slot_id, value[17:41], value[41:89]))
            else:
                assert len(value) == 1 + 16 + 8 + 64 + 72
                pub, eph = value[25:57], value[57:89]
                c["slots"].append(("keyfile", slot_id, value[17:25], value[89:113], value[113:161], pub, eph))
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
    if ftype == "fido2-x25519":
        fido = next(x for x in c["slots"] if x[0] == "fido2")
        _, slot_id, rp_id, cred, prf_input, nonce, ct, pub, eph = fido
        assert rp_id == v["factor"]["rp_id"] and cred.hex() == v["factor"]["credential_id"]
        hmac_salt = hashlib.sha256(b"WebAuthn PRF\x00" + prf_input).digest()
        assert hmac_salt.hex() == v["factor"]["hmac_salt"], "hmac salt"
        hmac_output = h(v["factor"]["hmac_output"])
        aad = b"ghul/v1/factor" + sid + slot_id
        seed = hkdf(hmac_output, b"ghul/v2/slot/fido2-x25519" + slot_id)
        s = x25519_open(seed, slot_id, pub, eph, nonce, ct, aad, v)
    else:
        assert ftype == "keyfile-x25519"
        kf = h(v["keyfile"])
        assert len(kf) == 46 and kf[:8] == b"GHULKEY\x00" and kf[8:10] == b"\x01\x00"
        assert hashlib.sha256(kf[:42]).digest()[:4] == kf[42:46]
        secret = kf[10:42]
        _, slot_id, keyfile_id, nonce, ct, pub, eph = next(x for x in c["slots"] if x[0] == "keyfile")
        assert hashlib.sha256(b"ghul/v1/keyfile-id" + secret).digest()[:8] == keyfile_id
        assert keyfile_id.hex() == v["factor"]["keyfile_id"]
        aad = b"ghul/v1/factor" + sid + slot_id
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

    # Write tag: only whoever has the password (and S) can make it.
    ak = hkdf(pk + s, b"ghul/v3/auth" + cid)
    assert ak.hex() == v["auth_key"], "write key"
    assert hmac.compare_digest(hmac.new(ak, c["signed"], hashlib.sha256).digest(), c["auth"]), "write tag"

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
    if v["kind"] == "docs":
        verify_docs_index(doc)
    print(f"ok  {v['description']}: {len(data)} bytes")


# --- GhulDocs (FORMAT.md section 14) ------------------------------------------------------------

BLOB_SECTION = 4 * 1024 * 1024
BLOB_PAD = 64 * 1024
BLOBS = {}  # content id (hex) -> (vector, file bytes), filled by verify_blob


def blob_prefix(content_id: bytes) -> bytes:
    return b"GHULBLOB" + struct.pack("<H", 1) + content_id


def blob_layout(size: int) -> tuple:
    """Plaintext length of each section, the end of the last one and the file length."""
    n = 1 if size == 0 else -(-size // BLOB_SECTION)
    lens = [min(BLOB_SECTION, size - i * BLOB_SECTION) for i in range(n)]
    end = 26 + sum(40 + k for k in lens)
    total = -(-(end + 48) // BLOB_PAD) * BLOB_PAD
    return lens, end, total


def blob_encrypt(content_id: bytes, key: bytes, content: bytes, nonces: list, trailer_nonce: bytes) -> bytes:
    bk = hkdf(key, b"ghul/v1/docs/blob" + content_id)
    prefix = blob_prefix(content_id)
    lens, end, total = blob_layout(len(content))
    out, pos = bytearray(prefix), 0
    for i, k in enumerate(lens):
        aad = prefix + struct.pack("<QB", i, 1 if i == len(lens) - 1 else 0)
        out += nonces[i] + crypto_aead_xchacha20poly1305_ietf_encrypt(content[pos : pos + k], aad, nonces[i], bk)
        pos += k
    assert len(out) == end
    out += bytes(total - 48 - end)
    out += trailer_nonce + crypto_aead_xchacha20poly1305_ietf_encrypt(struct.pack("<Q", len(content)), prefix + b"end", trailer_nonce, bk)
    assert len(out) == total
    return bytes(out)


def blob_decrypt(content_id: bytes, key: bytes, data: bytes) -> bytes:
    """Reads a content file from its key alone: length from the trailer, then every section."""
    bk = hkdf(key, b"ghul/v1/docs/blob" + content_id)
    prefix = blob_prefix(content_id)
    assert data[:26] == prefix, "magic, version, id"
    assert len(data) % BLOB_PAD == 0
    (size,) = struct.unpack("<Q", aead_open(bk, data[-48:-24], data[-24:], prefix + b"end"))
    lens, end, total = blob_layout(size)
    assert len(data) == total, "file length"
    assert not any(data[end : total - 48]), "filling is zero"
    out, pos = bytearray(), 26
    for i, k in enumerate(lens):
        nonce, ct = data[pos : pos + 24], data[pos + 24 : pos + 40 + k]
        out += aead_open(bk, nonce, ct, prefix + struct.pack("<QB", i, 1 if i == len(lens) - 1 else 0))
        pos += 40 + k
    return bytes(out)


def verify_blob(v: dict) -> None:
    h = bytes.fromhex
    content_id, key = h(v["id"]), h(v["key"])
    assert hkdf(key, b"ghul/v1/docs/blob" + content_id).hex() == v["blob_key"], "content file key"
    if v["content"] is not None:
        content = h(v["content"])
    else:
        assert v["content_pattern"] == "byte i = i mod 251"
        content = bytes(i % 251 for i in range(v["size"]))
    assert len(content) == v["size"]
    # Encrypted again from its inputs: the same file, byte for byte.
    nonces = [h(x) for x in v["section_nonces"]]
    file = blob_encrypt(content_id, key, content, nonces, h(v["trailer_nonce"]))
    assert len(file) == v["file_len"] == blob_layout(len(content))[2]
    assert hashlib.sha256(file).hexdigest() == v["file_sha256"], "file SHA-256"
    assert zlib.crc32(file) == v["file_crc32"], "file CRC-32"
    if v["file"] is not None:
        assert file == h(v["file"]), "published file"
    assert blob_decrypt(content_id, key, file) == content
    # Every change is noticed: a flipped byte in the prefix, a section, the filling, the trailer;
    # cut off; sections of another file id.
    for at in [5, 12, 30, 26 + 24 + 3, len(file) - 1000, len(file) - 1]:
        bad = bytearray(file)
        bad[at] ^= 1
        try:
            blob_decrypt(content_id, key, bytes(bad))
        except Exception:
            continue
        raise AssertionError(f"a changed byte at {at} was not noticed")
    for bad in [file[:-BLOB_PAD], file + bytes(BLOB_PAD)]:
        try:
            blob_decrypt(content_id, key, bad)
        except Exception:
            continue
        raise AssertionError("a cut or longer file was not noticed")
    BLOBS[v["id"]] = (v, file)
    print(f"ok  {v['description']}: {len(file)} bytes")


def field(entry: dict, name: str, t: str):
    f = entry["fields"][name]
    assert f["t"] == t, (name, f)
    return f["v"]


def verify_docs_index(doc: dict) -> None:
    """The index of a GhulDocs vault: folders and files; each file's content file opens with the
    key in the index and has the SHA-256, CRC-32 and size the index says."""
    entries = doc["entries"]
    folders = {e["id"]: e for e in entries if e["type"] == "folder"}
    files = [e for e in entries if e["type"] == "file"]
    assert folders and files and len(folders) + len(files) == len(entries)
    for e in entries:
        assert len(e["id"]) == 16 and not e["deleted"]
        assert field(e, "name", "text")
        if "parent" in e["fields"]:
            assert field(e, "parent", "bytes") in folders
    for e in files:
        content_id = field(e, "blob", "bytes")
        v, file = BLOBS[content_id.hex()]
        key = field(e, "key", "bytes")
        assert len(key) == 32 and key.hex() == v["key"]
        assert field(e, "sha256", "bytes") == hashlib.sha256(file).digest()
        assert field(e, "crc32", "int") == zlib.crc32(file)
        assert field(e, "size", "int") == v["size"]
        assert field(e, "mime", "text")
        assert blob_decrypt(content_id, key, file).hex() == v["content"]


def main() -> int:
    vectors_dir = Path(__file__).resolve().parent.parent / "docs" / "testvectors"
    blobs = json.loads((vectors_dir / "docs-blob-v1.json").read_text())["vectors"]
    for v in blobs:
        verify_blob(v)
    vectors = json.loads((vectors_dir / "container-v3.json").read_text())["vectors"]
    for v in vectors:
        assert struct.unpack_from("<H", bytes.fromhex(v["file"]), 8)[0] == 3
        verify(v)
    assert any(v["kind"] == "docs" for v in vectors)
    print(f"{len(vectors)} container and {len(blobs)} content file test vectors verified independently")
    return 0


if __name__ == "__main__":
    sys.exit(main())
