// SPDX-License-Identifier: AGPL-3.0-or-later
//! Import from other apps and plain-text export.
//!
//! Supported (unencrypted exports only):
//! - **Proton Pass:** ZIP with `data.json`, the JSON itself, or CSV
//! - **Bitwarden:** JSON and CSV
//! - **KeePass / KeePassXC**, Chrome, Firefox and other password CSV files (columns are recognised by name)
//! - **Proton Authenticator** and any JSON/text containing `otpauth://` links
//! - **Aegis** (plain JSON), **2FAS** (plain `.2fas` JSON)
//! - **Google Authenticator:** `otpauth-migration://` links (from the export QR code)
//!
//! Encrypted exports are recognised and rejected with a clear message.

use std::collections::BTreeMap;
use std::io::Read;

use serde_json::Value as Json;
use zeroize::Zeroizing;

use super::error::{GhulError, Result};
use super::tools::{base32_decode, parse_otpauth_uri, percent_decode, validate_totp, OtpAlgo, TotpInput};
use crate::vault::{Entry, Field};

/// Largest file accepted for import (also the limit for files inside a ZIP).
const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;

/// A GhulPass entry read from an import file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draft {
    pub entry_type: String,
    pub fields: BTreeMap<String, Field>,
    pub folder: Option<String>,
}

impl Draft {
    fn new(entry_type: &str) -> Self {
        Draft { entry_type: entry_type.to_owned(), fields: BTreeMap::new(), folder: None }
    }

    fn text(&mut self, name: &str, value: &str) {
        let v = value.trim();
        if !v.is_empty() {
            self.fields.insert(name.to_owned(), Field::Text(v.to_owned()));
        }
    }

    fn secret(&mut self, name: &str, value: &str) {
        if !value.is_empty() {
            self.fields.insert(name.to_owned(), Field::Secret(value.to_owned()));
        }
    }

    fn urls(&mut self, urls: Vec<String>) {
        let urls: Vec<String> = urls.into_iter().map(|u| u.trim().to_owned()).filter(|u| !u.is_empty()).collect();
        if !urls.is_empty() {
            self.fields.insert("urls".into(), Field::List(urls));
        }
    }

    fn get(&self, name: &str) -> String {
        match self.fields.get(name) {
            Some(Field::Text(s)) | Some(Field::Secret(s)) | Some(Field::Url(s)) => s.clone(),
            Some(Field::List(l)) => l.first().cloned().unwrap_or_default(),
            _ => String::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.fields.values().all(|f| matches!(f, Field::Text(s) | Field::Secret(s) if s.is_empty()))
    }
}

/// A TOTP account read from an import file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImportedTotp {
    pub input: TotpInput,
    /// Came from a login entry (password managers store TOTP inside logins).
    pub from_login: bool,
}

/// Why an item of an import file could not be converted.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// A counter-based code (HOTP): GhulAuth only shows time-based codes.
    Hotp,
    /// An item type GhulSuite has no counterpart for.
    Unsupported,
    /// A 2FA code whose data is incomplete or invalid (secret, digits, period …).
    InvalidCode,
}

/// An item of an import file that was not converted. Its original data can be kept as a note.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Skipped {
    pub name: String,
    /// Type in the source app, e.g. `alias`, `hotp`, `sshKey`.
    pub kind: String,
    pub reason: SkipReason,
    /// Technical detail (e.g. "secret is not valid Base32").
    pub detail: String,
    /// The original data (JSON, link or CSV line). Secret: only ever stored encrypted.
    pub raw: String,
    /// The data is already part of an imported entry (a 2FA code inside a login): nothing to keep.
    pub in_entry: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Parsed {
    pub format: String,
    pub entries: Vec<Draft>,
    pub totps: Vec<ImportedTotp>,
    /// Items that could not be converted, with their original data.
    pub skipped: Vec<Skipped>,
}

impl Parsed {
    fn skip(&mut self, name: &str, kind: &str, reason: SkipReason, detail: &str, raw: String) {
        self.skipped.push(Skipped {
            name: name.trim().to_owned(),
            kind: kind.to_owned(),
            reason,
            detail: detail.to_owned(),
            raw,
            in_entry: false,
        });
    }
}

/// Reason of a failed 2FA link.
fn otp_reason(uri: &str) -> SkipReason {
    if uri.trim_start().to_ascii_lowercase().starts_with("otpauth://hotp") {
        SkipReason::Hotp
    } else {
        SkipReason::InvalidCode
    }
}

/// Best-effort name of a 2FA link (its label) for the list of skipped items.
fn uri_label(uri: &str) -> String {
    let rest = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    let label = rest.split_once('/').map(|(_, l)| l).unwrap_or("");
    let label = label.split('?').next().unwrap_or("");
    percent_decode(label).unwrap_or_else(|| label.to_owned())
}

fn json_text(v: &Json) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn invalid(msg: &str) -> GhulError {
    GhulError::InvalidInput { msg: msg.to_owned() }
}

/// Reads an import file. `file_name` only helps to tell formats apart.
pub(crate) fn parse(data: &[u8], file_name: &str) -> Result<Parsed> {
    if data.len() as u64 > MAX_IMPORT_BYTES {
        return Err(invalid("file too large"));
    }
    if data.starts_with(b"PK\x03\x04") {
        return parse_zip(data);
    }
    let text = std::str::from_utf8(data).map_err(|_| GhulError::ImportUnknownFormat)?;
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.starts_with("-----BEGIN PGP") || file_name.ends_with(".pgp") {
        return Err(GhulError::ImportEncrypted);
    }
    if text.starts_with('{') || text.starts_with('[') {
        let json: Json = serde_json::from_str(text).map_err(|_| GhulError::ImportUnknownFormat)?;
        return parse_json(&json);
    }
    if text.lines().any(|l| l.trim_start().starts_with("otpauth") || l.trim_start().starts_with("steam://")) {
        let mut p = Parsed { format: "otpauth".into(), ..Default::default() };
        for line in text.lines().map(str::trim).filter(|l| l.starts_with("otpauth") || l.starts_with("steam://")) {
            add_uri(&mut p, line);
        }
        return Ok(p);
    }
    parse_csv(text)
}

fn parse_zip(data: &[u8]) -> Result<Parsed> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|_| GhulError::ImportUnknownFormat)?;
    let names: Vec<String> = zip.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).collect();
    let pick = names
        .iter()
        // 1Password (.1pux): the items are in "export.data" (JSON).
        .find(|n| n.ends_with("export.data"))
        .or_else(|| names.iter().find(|n| n.ends_with("data.json")))
        .or_else(|| names.iter().find(|n| n.ends_with(".json")))
        .or_else(|| names.iter().find(|n| n.ends_with(".csv")))
        .cloned()
        .ok_or(GhulError::ImportUnknownFormat)?;
    if names.iter().any(|n| n.ends_with(".pgp")) && !pick.ends_with(".json") {
        return Err(GhulError::ImportEncrypted);
    }
    let file = zip.by_name(&pick).map_err(|_| GhulError::ImportUnknownFormat)?;
    if file.size() > MAX_IMPORT_BYTES {
        return Err(invalid("file too large"));
    }
    let mut buf = Zeroizing::new(Vec::new());
    file.take(MAX_IMPORT_BYTES).read_to_end(&mut buf).map_err(|_| GhulError::ImportUnknownFormat)?;
    if pick.ends_with("export.data") {
        let text = std::str::from_utf8(&buf).map_err(|_| GhulError::ImportUnknownFormat)?;
        let json: Json = serde_json::from_str(text).map_err(|_| GhulError::ImportUnknownFormat)?;
        return parse_json(&json);
    }
    parse(&buf, &pick)
}

// --- JSON formats ---------------------------------------------------------------------------

fn s<'a>(v: &'a Json, key: &str) -> &'a str {
    v.get(key).and_then(Json::as_str).unwrap_or("")
}

fn parse_json(json: &Json) -> Result<Parsed> {
    // Aegis
    if let Some(db) = json.get("db") {
        if db.is_string() {
            return Err(GhulError::ImportEncrypted);
        }
        return Ok(parse_aegis(db));
    }
    // 2FAS
    if json.get("servicesEncrypted").is_some_and(|v| !v.is_null()) {
        return Err(GhulError::ImportEncrypted);
    }
    if let Some(services) = json.get("services").and_then(Json::as_array) {
        return Ok(parse_2fas(services));
    }
    // Bitwarden
    if json.get("encrypted").and_then(Json::as_bool) == Some(true) {
        return Err(GhulError::ImportEncrypted);
    }
    if let Some(items) = json.get("items").and_then(Json::as_array) {
        return Ok(parse_bitwarden(json, items));
    }
    // Proton Pass
    if json.get("encrypted").and_then(Json::as_bool) == Some(true) {
        return Err(GhulError::ImportEncrypted);
    }
    if let Some(vaults) = json.get("vaults").and_then(Json::as_object) {
        return Ok(parse_proton_pass(vaults));
    }
    // 1Password (.1pux)
    if let Some(accounts) = json.get("accounts").and_then(Json::as_array) {
        return Ok(parse_1password(accounts));
    }
    // Proton Authenticator: entries with content {uri, name, entry_type}
    if let Some(entries) = json.get("entries").and_then(Json::as_array) {
        if entries.iter().any(|e| e.get("content").and_then(|c| c.get("uri")).is_some()) {
            return Ok(parse_proton_authenticator(entries));
        }
    }
    // Anything else with otpauth links
    let mut uris = Vec::new();
    collect_uris(json, &mut uris);
    if uris.is_empty() {
        return Err(GhulError::ImportUnknownFormat);
    }
    let mut p = Parsed { format: "otpauth (JSON)".into(), ..Default::default() };
    if json.get("entries").is_some() {
        p.format = "Proton Authenticator".into();
    }
    for u in uris {
        add_uri(&mut p, &u);
    }
    Ok(p)
}

/// 1Password 1PUX export: accounts → vaults → items (logins, passwords, notes, cards).
fn parse_1password(accounts: &[Json]) -> Parsed {
    let mut p = Parsed { format: "1Password (1PUX)".into(), ..Default::default() };
    let items = accounts
        .iter()
        .flat_map(|a| a.get("vaults").and_then(Json::as_array).cloned().unwrap_or_default())
        .flat_map(|v| {
            let name = v.get("attrs").map(|a| s(a, "name").to_owned()).unwrap_or_default();
            v.get("items")
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(move |i| (name.clone(), i))
        });
    for (vault, item) in items {
        if s(&item, "state") == "archived" || item.get("trashed").and_then(Json::as_bool) == Some(true) {
            continue;
        }
        let overview = item.get("overview").cloned().unwrap_or(Json::Null);
        let details = item.get("details").cloned().unwrap_or(Json::Null);
        let title = s(&overview, "title").to_owned();
        let category = s(&item, "categoryUuid");
        let mut d = match category {
            "001" | "005" => Draft::new("login"),
            "003" => Draft::new("note"),
            "002" => Draft::new("card"),
            other => {
                p.skip(&title, other, SkipReason::Unsupported, "", json_text(&item));
                continue;
            }
        };
        d.text("title", &title);
        let mut username = String::new();
        for f in details.get("loginFields").and_then(Json::as_array).into_iter().flatten() {
            match s(f, "designation") {
                "username" => username = s(f, "value").to_owned(),
                "password" => d.secret("password", s(f, "value")),
                _ => {}
            }
        }
        if category == "005" {
            d.secret("password", s(&details, "password"));
        }
        if !username.is_empty() {
            d.text("username", &username);
        }
        let mut urls: Vec<String> = overview
            .get("urls")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .map(|u| s(u, "url").to_owned())
            .collect();
        if urls.is_empty() && !s(&overview, "url").is_empty() {
            urls.push(s(&overview, "url").to_owned());
        }
        if category != "003" && category != "002" {
            d.urls(urls);
        }
        d.text("notes", s(&details, "notesPlain"));
        for section in details.get("sections").and_then(Json::as_array).into_iter().flatten() {
            for f in section.get("fields").and_then(Json::as_array).into_iter().flatten() {
                let id = s(f, "id");
                let value = f.get("value").cloned().unwrap_or(Json::Null);
                let text = value
                    .as_object()
                    .and_then(|o| {
                        o.values()
                            .find_map(|v| v.as_str().map(str::to_owned).or_else(|| v.as_i64().map(|n| n.to_string())))
                    })
                    .unwrap_or_default();
                if let Some(otp) = value.get("totp").and_then(Json::as_str) {
                    if category != "003" {
                        d.secret("totp", otp);
                        login_totp(&mut p, otp, &title, &username);
                    }
                    continue;
                }
                match (category, id) {
                    ("002", "cardholder") => d.text("holder", &text),
                    ("002", "ccnum") => d.secret("number", &text),
                    ("002", "cvv") => d.secret("cvc", &text),
                    ("002", "expiry") => {
                        // 1Password stores the expiry as YYYYMM.
                        let e = if text.len() == 6 { format!("{}/{}", &text[4..], &text[2..4]) } else { text.clone() };
                        d.text("expiry", &e)
                    }
                    ("002", "pin") => d.secret("pin", &text),
                    _ if !text.is_empty() => {
                        let name = s(f, "title");
                        if !name.is_empty() {
                            if value.get("concealed").is_some() {
                                d.secret(name, &text)
                            } else {
                                d.text(name, &text)
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if !vault.is_empty() {
            d.folder = Some(vault);
        }
        if d.is_empty() {
            continue;
        }
        p.entries.push(d);
    }
    p
}

/// Proton Authenticator export: `{"version": 1, "entries": [{"content": {"uri", "name",
/// "entry_type": "Totp" | "Steam"}, "note"}]}`. The name fills in a missing label of the link.
fn parse_proton_authenticator(entries: &[Json]) -> Parsed {
    let mut p = Parsed { format: "Proton Authenticator".into(), ..Default::default() };
    for e in entries {
        let c = e.get("content").unwrap_or(&Json::Null);
        let uri = s(c, "uri").trim();
        let name = s(c, "name").trim();
        let parsed = if s(c, "entry_type").eq_ignore_ascii_case("steam") && !uri.starts_with("otpauth") {
            let secret = uri.strip_prefix("steam://").unwrap_or(uri);
            let input = super::tools::steam_input(if name.is_empty() { "Steam" } else { name }, "", secret);
            validate_totp(&input).map(|_| input)
        } else {
            parse_otpauth_uri(uri.to_owned())
        };
        match parsed {
            Ok(mut input) => {
                if !name.is_empty() && input.issuer.is_empty() {
                    input.issuer = name.to_owned();
                }
                p.totps.push(ImportedTotp { input, from_login: false });
            }
            Err(err) => {
                // A link without a label is fine when Proton has a name for it.
                let retry =
                    (!name.is_empty()).then(|| parse_otpauth_uri(with_label(uri, name)).ok()).flatten().map(|mut i| {
                        // Proton shows the name as the service: it becomes the issuer.
                        if i.issuer.is_empty() {
                            i.issuer = std::mem::take(&mut i.account);
                        }
                        i
                    });
                match retry {
                    Some(input) => p.totps.push(ImportedTotp { input, from_login: false }),
                    None => {
                        let label = if name.is_empty() { uri_label(uri) } else { name.to_owned() };
                        p.skip(&label, s(c, "entry_type"), otp_reason(uri), &err.to_string(), json_text(e));
                    }
                }
            }
        }
    }
    p
}

/// The link with `name` as its label (for links whose label is empty).
fn with_label(uri: &str, name: &str) -> String {
    let Some(rest) = uri.strip_prefix("otpauth://") else { return uri.to_owned() };
    let Some((kind, after)) = rest.split_once('/') else { return uri.to_owned() };
    let query = after.split_once('?').map(|(_, q)| q).unwrap_or("");
    let label: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("otpauth://{kind}/{label}?{query}")
}

fn collect_uris(v: &Json, out: &mut Vec<String>) {
    match v {
        Json::String(s) if s.starts_with("otpauth") || s.starts_with("steam://") => out.push(s.clone()),
        Json::Array(a) => a.iter().for_each(|x| collect_uris(x, out)),
        Json::Object(o) => o.values().for_each(|x| collect_uris(x, out)),
        _ => {}
    }
}

fn totp_from_parts(
    issuer: &str,
    account: &str,
    secret: &str,
    algo: &str,
    digits: i64,
    period: i64,
) -> Option<TotpInput> {
    let input = TotpInput {
        issuer: issuer.trim().to_owned(),
        account: account.trim().to_owned(),
        secret_base32: secret.trim().to_owned(),
        algorithm: OtpAlgo::from_name(algo).unwrap_or(OtpAlgo::Sha1),
        digits: if digits > 0 { digits as u32 } else { 6 },
        period: if period > 0 { period as u32 } else { 30 },
    };
    validate_totp(&input).ok().map(|_| input)
}

fn parse_aegis(db: &Json) -> Parsed {
    let mut p = Parsed { format: "Aegis".into(), ..Default::default() };
    for e in db.get("entries").and_then(Json::as_array).into_iter().flatten() {
        let info = e.get("info").unwrap_or(&Json::Null);
        let t = s(e, "type");
        let made = if t.eq_ignore_ascii_case("steam") {
            let input = super::tools::steam_input(s(e, "issuer"), s(e, "name"), s(info, "secret"));
            validate_totp(&input).ok().map(|_| input)
        } else {
            (t.is_empty() || t == "totp")
                .then(|| {
                    totp_from_parts(
                        s(e, "issuer"),
                        s(e, "name"),
                        s(info, "secret"),
                        s(info, "algo"),
                        info.get("digits").and_then(Json::as_i64).unwrap_or(6),
                        info.get("period").and_then(Json::as_i64).unwrap_or(30),
                    )
                })
                .flatten()
        };
        match made {
            Some(input) => p.totps.push(ImportedTotp { input, from_login: false }),
            None => {
                let reason = if t == "hotp" {
                    SkipReason::Hotp
                } else if t.is_empty() || t == "totp" {
                    SkipReason::InvalidCode
                } else {
                    SkipReason::Unsupported
                };
                let name = [s(e, "issuer"), s(e, "name")]
                    .iter()
                    .filter(|x| !x.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" · ");
                p.skip(&name, t, reason, "", json_text(e));
            }
        }
    }
    p
}

fn parse_2fas(services: &[Json]) -> Parsed {
    let mut p = Parsed { format: "2FAS".into(), ..Default::default() };
    for e in services {
        let otp = e.get("otp").unwrap_or(&Json::Null);
        let t = s(otp, "tokenType");
        let issuer = if s(otp, "issuer").is_empty() { s(e, "name") } else { s(otp, "issuer") };
        let made = if t.eq_ignore_ascii_case("steam") {
            let input = super::tools::steam_input(issuer, s(otp, "account"), s(e, "secret"));
            validate_totp(&input).ok().map(|_| input)
        } else {
            (t.is_empty() || t.eq_ignore_ascii_case("totp"))
                .then(|| {
                    totp_from_parts(
                        issuer,
                        s(otp, "account"),
                        s(e, "secret"),
                        s(otp, "algorithm"),
                        otp.get("digits").and_then(Json::as_i64).unwrap_or(6),
                        otp.get("period").and_then(Json::as_i64).unwrap_or(30),
                    )
                })
                .flatten()
        };
        match made {
            Some(input) => p.totps.push(ImportedTotp { input, from_login: false }),
            None => {
                let reason = if t.eq_ignore_ascii_case("hotp") {
                    SkipReason::Hotp
                } else if t.is_empty() || t.eq_ignore_ascii_case("totp") {
                    SkipReason::InvalidCode
                } else {
                    SkipReason::Unsupported
                };
                p.skip(issuer, t, reason, "", json_text(e));
            }
        }
    }
    p
}

/// TOTP stored in a login: an otpauth link or a bare Base32 secret.
fn login_totp(p: &mut Parsed, value: &str, title: &str, username: &str) {
    let v = value.trim();
    if v.is_empty() {
        return;
    }
    let input = if v.starts_with("otpauth://") {
        parse_otpauth_uri(v.to_owned()).ok().map(|mut i| {
            if i.issuer.is_empty() {
                i.issuer = title.to_owned();
            }
            i
        })
    } else {
        totp_from_parts(title, username, v, "SHA1", 6, 30)
    };
    match input {
        Some(input) => p.totps.push(ImportedTotp { input, from_login: true }),
        None => {
            // The value stays in the login's "totp" field; only GhulAuth does not get it.
            p.skipped.push(Skipped {
                name: title.trim().to_owned(),
                kind: "totp".into(),
                reason: otp_reason(v),
                detail: String::new(),
                raw: v.to_owned(),
                in_entry: true,
            });
        }
    }
}

fn parse_bitwarden(root: &Json, items: &[Json]) -> Parsed {
    let mut p = Parsed { format: "Bitwarden".into(), ..Default::default() };
    let folders: BTreeMap<String, String> = root
        .get("folders")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .map(|f| (s(f, "id").to_owned(), s(f, "name").to_owned()))
        .collect();
    for it in items {
        let name = s(it, "name");
        let mut d = match it.get("type").and_then(Json::as_i64) {
            Some(1) => {
                let l = it.get("login").unwrap_or(&Json::Null);
                let mut d = Draft::new("login");
                d.text("title", name);
                d.text("username", s(l, "username"));
                d.secret("password", s(l, "password"));
                d.urls(
                    l.get("uris")
                        .and_then(Json::as_array)
                        .into_iter()
                        .flatten()
                        .map(|u| s(u, "uri").to_owned())
                        .collect(),
                );
                if !s(l, "totp").is_empty() {
                    d.secret("totp", s(l, "totp"));
                    login_totp(&mut p, s(l, "totp"), name, s(l, "username"));
                }
                d
            }
            Some(2) => {
                let mut d = Draft::new("note");
                d.text("title", name);
                d
            }
            Some(3) => {
                let c = it.get("card").unwrap_or(&Json::Null);
                let mut d = Draft::new("card");
                d.text("title", name);
                d.text("holder", s(c, "cardholderName"));
                d.secret("number", s(c, "number"));
                let (m, y) = (s(c, "expMonth"), s(c, "expYear"));
                if !m.is_empty() || !y.is_empty() {
                    d.text("expiry", &format!("{m:0>2}/{y}"));
                }
                d.secret("cvc", s(c, "code"));
                d
            }
            Some(4) => {
                let i = it.get("identity").unwrap_or(&Json::Null);
                let mut d = Draft::new("identity");
                d.text("title", name);
                d.text("name", &format!("{} {}", s(i, "firstName"), s(i, "lastName")));
                d.text("email", s(i, "email"));
                d.text("phone", s(i, "phone"));
                let addr: Vec<&str> = ["address1", "address2", "postalCode", "city", "country"]
                    .iter()
                    .map(|k| s(i, k))
                    .filter(|x| !x.is_empty())
                    .collect();
                d.text("address", &addr.join("\n"));
                d
            }
            other => {
                let kind = other.map(|n| n.to_string()).unwrap_or_default();
                p.skip(name, &kind, SkipReason::Unsupported, "", json_text(it));
                continue;
            }
        };
        d.text("notes", s(it, "notes"));
        for f in it.get("fields").and_then(Json::as_array).into_iter().flatten() {
            let (n, v) = (s(f, "name"), s(f, "value"));
            if !n.is_empty() && !d.fields.contains_key(n) {
                if f.get("type").and_then(Json::as_i64) == Some(1) {
                    d.secret(n, v);
                } else {
                    d.text(n, v);
                }
            }
        }
        d.folder = folders.get(s(it, "folderId")).cloned().filter(|f| !f.is_empty());
        p.entries.push(d);
    }
    p
}

fn parse_proton_pass(vaults: &serde_json::Map<String, Json>) -> Parsed {
    let mut p = Parsed { format: "Proton Pass".into(), ..Default::default() };
    for v in vaults.values() {
        let vault_name = s(v, "name");
        for it in v.get("items").and_then(Json::as_array).into_iter().flatten() {
            // state 2 = in the trash
            if it.get("state").and_then(Json::as_i64) == Some(2) {
                continue;
            }
            let data = it.get("data").unwrap_or(&Json::Null);
            let meta = data.get("metadata").unwrap_or(&Json::Null);
            let c = data.get("content").unwrap_or(&Json::Null);
            let name = s(meta, "name");
            let mut d = match s(data, "type") {
                "login" => {
                    let mut d = Draft::new("login");
                    d.text("title", name);
                    let (user, email) = (s(c, "itemUsername"), s(c, "itemEmail"));
                    if user.is_empty() {
                        d.text("username", email);
                    } else {
                        d.text("username", user);
                        if email != user {
                            d.text("email", email);
                        }
                    }
                    d.secret("password", s(c, "password"));
                    d.urls(
                        c.get("urls")
                            .and_then(Json::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(Json::as_str)
                            .map(str::to_owned)
                            .collect(),
                    );
                    if !s(c, "totpUri").is_empty() {
                        d.secret("totp", s(c, "totpUri"));
                        let u = d.get("username");
                        login_totp(&mut p, s(c, "totpUri"), name, &u);
                    }
                    d
                }
                "note" => {
                    let mut d = Draft::new("note");
                    d.text("title", name);
                    d
                }
                "creditCard" => {
                    let mut d = Draft::new("card");
                    d.text("title", name);
                    d.text("holder", s(c, "cardholderName"));
                    d.secret("number", s(c, "number"));
                    d.text("expiry", s(c, "expirationDate"));
                    d.secret("cvc", s(c, "verificationNumber"));
                    d.secret("pin", s(c, "pin"));
                    d
                }
                "identity" => {
                    let mut d = Draft::new("identity");
                    d.text("title", name);
                    d.text("name", s(c, "fullName"));
                    d.text("email", s(c, "email"));
                    d.text("phone", s(c, "phoneNumber"));
                    let addr: Vec<&str> = ["streetAddress", "zipOrPostalCode", "city", "countryOrRegion"]
                        .iter()
                        .map(|k| s(c, k))
                        .filter(|x| !x.is_empty())
                        .collect();
                    d.text("address", &addr.join("\n"));
                    d
                }
                "wifi" => {
                    let mut d = Draft::new("wifi");
                    d.text("title", name);
                    d.text("ssid", s(c, "ssid"));
                    d.secret("password", s(c, "password"));
                    d
                }
                "alias" => {
                    // An e-mail alias: the address is what matters.
                    let mut d = Draft::new("login");
                    d.text("title", name);
                    d.text("username", s(it, "aliasEmail"));
                    d
                }
                other => {
                    // Item types without a counterpart (SSH keys, custom items, newer types):
                    // a note with every value of the item, so nothing is lost.
                    let mut d = Draft::new("note");
                    d.text("title", name);
                    d.text("type", other);
                    flatten_into(&mut d, c, "");
                    d
                }
            };
            d.text("notes", s(meta, "note"));
            for f in data.get("extraFields").and_then(Json::as_array).into_iter().flatten() {
                let n = s(f, "fieldName");
                let val = f.get("data").map(|x| s(x, "content")).unwrap_or("");
                if n.is_empty() || d.fields.contains_key(n) {
                    continue;
                }
                match s(f, "type") {
                    "hidden" | "totp" => d.secret(n, val),
                    _ => d.text(n, val),
                }
            }
            if !vault_name.is_empty() {
                d.folder = Some(vault_name.to_owned());
            }
            p.entries.push(d);
        }
    }
    p
}

/// Every text value of `v` as a field of `d` (nested keys joined with "."); keys that sound
/// secret become hidden fields.
fn flatten_into(d: &mut Draft, v: &Json, prefix: &str) {
    match v {
        Json::Object(o) => {
            for (k, x) in o {
                let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten_into(d, x, &key);
            }
        }
        Json::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                flatten_into(d, x, &format!("{prefix}.{}", i + 1));
            }
        }
        Json::String(t) if !t.is_empty() && !prefix.is_empty() && !d.fields.contains_key(prefix) => {
            let l = prefix.to_lowercase();
            if ["key", "password", "secret", "token", "pin", "private"].iter().any(|w| l.contains(w)) {
                d.secret(prefix, t);
            } else {
                d.text(prefix, t);
            }
        }
        Json::Number(n) if !prefix.is_empty() => d.text(prefix, &n.to_string()),
        _ => {}
    }
}

// --- otpauth and Google Authenticator migration ---------------------------------------------

fn add_uri(p: &mut Parsed, uri: &str) {
    if uri.starts_with("otpauth-migration://") {
        match parse_migration(uri) {
            Ok((list, skipped)) => {
                p.format = "Google Authenticator".into();
                p.skipped.extend(skipped);
                p.totps.extend(list.into_iter().map(|input| ImportedTotp { input, from_login: false }));
            }
            Err(e) => {
                p.skip("Google Authenticator", "migration", SkipReason::InvalidCode, &e.to_string(), uri.to_owned())
            }
        }
        return;
    }
    match parse_otpauth_uri(uri.to_owned()) {
        Ok(input) => p.totps.push(ImportedTotp { input, from_login: false }),
        Err(e) => p.skip(&uri_label(uri), "otpauth", otp_reason(uri), &e.to_string(), uri.to_owned()),
    }
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *data.get(*pos)?;
        *pos += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// A protobuf field value: varint (`Ok`) or length-delimited bytes (`Err`).
type ProtoValue<'a> = std::result::Result<u64, &'a [u8]>;

/// Iterates the fields of a protobuf message: (field number, value).
fn proto_fields(data: &[u8]) -> Option<Vec<(u64, ProtoValue<'_>)>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let key = varint(data, &mut pos)?;
        let (field, wire) = (key >> 3, key & 7);
        match wire {
            0 => out.push((field, Ok(varint(data, &mut pos)?))),
            2 => {
                let len = varint(data, &mut pos)? as usize;
                let end = pos.checked_add(len).filter(|e| *e <= data.len())?;
                out.push((field, Err(&data[pos..end])));
                pos = end;
            }
            1 => pos = pos.checked_add(8).filter(|e| *e <= data.len())?,
            5 => pos = pos.checked_add(4).filter(|e| *e <= data.len())?,
            _ => return None,
        }
    }
    Some(out)
}

fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::with_capacity(data.len() * 8 / 5 + 1);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &b in data {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Decodes a Google Authenticator export link (`otpauth-migration://offline?data=…`).
/// Returns the TOTP accounts and the accounts that were skipped (HOTP), with their data.
pub(crate) fn parse_migration(uri: &str) -> Result<(Vec<TotpInput>, Vec<Skipped>)> {
    let bad = || invalid("not a valid Google Authenticator export");
    let query = uri.split_once('?').map(|(_, q)| q).ok_or_else(bad)?;
    let data = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("data="))
        .and_then(percent_decode)
        .and_then(|d| base64_decode(&d))
        .ok_or_else(bad)?;
    let mut list = Vec::new();
    let mut skipped = Vec::new();
    let skip = |skipped: &mut Vec<Skipped>, name: &str, issuer: &str, secret: &[u8], kind: &str, reason: SkipReason| {
        let label = if issuer.is_empty() { name.to_owned() } else { format!("{issuer}: {name}") };
        skipped.push(Skipped {
            name: label.clone(),
            kind: kind.into(),
            reason,
            detail: String::new(),
            raw: format!("{label}\nSecret (Base32): {}\nType: {kind}", base32_encode(secret)),
            in_entry: false,
        });
    };
    for (field, value) in proto_fields(&data).ok_or_else(bad)? {
        let Err(otp) = value else { continue };
        if field != 1 {
            continue;
        }
        let (mut secret, mut name, mut issuer, mut algo, mut digits, mut otp_type) =
            (Zeroizing::new(Vec::new()), String::new(), String::new(), 1, 1, 2);
        for (f, v) in proto_fields(otp).ok_or_else(bad)? {
            match (f, v) {
                (1, Err(b)) => secret = Zeroizing::new(b.to_vec()),
                (2, Err(b)) => name = String::from_utf8_lossy(b).into_owned(),
                (3, Err(b)) => issuer = String::from_utf8_lossy(b).into_owned(),
                (4, Ok(v)) => algo = v,
                (5, Ok(v)) => digits = v,
                (6, Ok(v)) => otp_type = v,
                _ => {}
            }
        }
        if otp_type != 2 && otp_type != 0 {
            skip(&mut skipped, &name, &issuer, &secret, "hotp", SkipReason::Hotp);
            continue;
        }
        let account = match name.split_once(':') {
            Some((i, a)) if issuer.is_empty() || i == issuer => {
                if issuer.is_empty() {
                    issuer = i.to_owned();
                }
                a.trim().to_owned()
            }
            _ => name.clone(),
        };
        let input = TotpInput {
            issuer,
            account,
            secret_base32: base32_encode(&secret),
            algorithm: match algo {
                2 => OtpAlgo::Sha256,
                3 => OtpAlgo::Sha512,
                _ => OtpAlgo::Sha1,
            },
            digits: if digits == 2 { 8 } else { 6 },
            period: 30,
        };
        match validate_totp(&input) {
            Ok(_) => list.push(input),
            Err(_) => skip(&mut skipped, &input.account, &input.issuer, &secret, "totp", SkipReason::InvalidCode),
        }
    }
    Ok((list, skipped))
}

// --- CSV ------------------------------------------------------------------------------------

/// RFC 4180 CSV: quoted fields may contain separators, quotes (`""`) and line breaks.
fn csv_records(text: &str) -> Vec<Vec<String>> {
    let first = text.lines().next().unwrap_or("");
    let sep = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            c if c == sep => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
                row.clear();
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn parse_csv(text: &str) -> Result<Parsed> {
    let rows = csv_records(text);
    let (header, body) = rows.split_first().ok_or(GhulError::ImportUnknownFormat)?;
    let header: Vec<String> = header.iter().map(|h| h.trim().to_lowercase()).collect();
    let col = |names: &[&str]| names.iter().find_map(|n| header.iter().position(|h| h == n));
    let title = col(&["name", "title", "account", "item name"]);
    let user = col(&["login_username", "username", "user name", "user", "login", "email address"]);
    let email = col(&["email"]);
    let password = col(&["login_password", "password"]);
    let url = col(&["login_uri", "url", "uri", "website", "web site", "urls"]);
    let notes = col(&["notes", "note", "extra", "comments", "comment"]);
    let totp = col(&["login_totp", "totp", "otp", "otpauth"]);
    let kind = col(&["type"]);
    let folder = col(&["folder", "group", "vault", "grouping"]);
    if password.is_none() && title.is_none() {
        return Err(GhulError::ImportUnknownFormat);
    }
    let format = if header.iter().any(|h| h == "login_uri") {
        "Bitwarden (CSV)"
    } else if header.iter().any(|h| h == "createtime") && header.iter().any(|h| h == "vault") {
        "Proton Pass (CSV)"
    } else if header.first().is_some_and(|h| h == "group") {
        "KeePass (CSV)"
    } else if header.iter().any(|h| h == "grouping") && header.iter().any(|h| h == "extra") {
        "LastPass (CSV)"
    } else if header.iter().any(|h| h == "otpauth") && header.iter().any(|h| h == "archived") {
        "1Password (CSV)"
    } else {
        "CSV"
    };
    // GhulSuite's own export: values exactly as stored (spaces kept), with the apostrophe that
    // protects spreadsheet programs from formulas taken off again.
    let own = header == OWN_CSV_COLUMNS;
    let format = if own { "GhulSuite (CSV)" } else { format };
    let mut p = Parsed { format: format.into(), ..Default::default() };
    let get = |row: &Vec<String>, c: Option<usize>| {
        c.and_then(|i| row.get(i))
            .map(|s| if own { s.strip_prefix('\'').unwrap_or(s).to_owned() } else { s.trim().to_owned() })
            .unwrap_or_default()
    };
    for row in body {
        let t = get(row, kind).to_lowercase();
        let is_note = t == "note" || t == "securenote" || get(row, url) == "http://sn";
        if !t.is_empty() && !is_note && t != "login" {
            // Other types (aliases, cards, identities …): a note with every column, nothing is lost.
            let mut d = Draft::new("note");
            d.text("title", &get(row, title));
            d.text("type", &t);
            for (i, h) in header.iter().enumerate() {
                let v = row.get(i).map(|x| x.trim()).unwrap_or("");
                if v.is_empty() || d.fields.contains_key(h) || Some(i) == title {
                    continue;
                }
                if h.contains("password") || h.contains("totp") || h.contains("secret") {
                    d.secret(h, v);
                } else {
                    d.text(h, v);
                }
            }
            let f = get(row, folder);
            if !f.is_empty() && f != "Root" {
                d.folder = Some(f);
            }
            if !d.is_empty() {
                p.entries.push(d);
            }
            continue;
        }
        let mut d = Draft::new(if is_note { "note" } else { "login" });
        let name = get(row, title);
        d.text("title", &name);
        let username = match (get(row, user), get(row, email)) {
            (u, _) if !u.is_empty() => u,
            (_, e) => e,
        };
        if !is_note {
            d.text("username", &username);
            d.secret("password", &get(row, password));
            d.urls(get(row, url).split(['\n', ',']).map(str::to_owned).collect());
            let e = get(row, email);
            if !e.is_empty() && e != username {
                d.text("email", &e);
            }
        }
        d.text("notes", &get(row, notes));
        let otp = get(row, totp);
        if !otp.is_empty() && !is_note {
            d.secret("totp", &otp);
            login_totp(&mut p, &otp, &name, &username);
        }
        let f = get(row, folder);
        if !f.is_empty() && f != "Root" {
            d.folder = Some(f);
        }
        if d.is_empty() {
            continue;
        }
        p.entries.push(d);
    }
    Ok(p)
}

// --- plain export ---------------------------------------------------------------------------

/// Columns of GhulSuite's own CSV export.
const OWN_CSV_COLUMNS: [&str; 8] = ["type", "title", "username", "password", "url", "notes", "totp", "folder"];

fn csv_field(s: &str) -> String {
    // Spreadsheet programs run cells starting with = + - @ (or a tab/CR before them) as formulas
    // ("CSV injection"). A leading apostrophe makes them plain text; the own import removes it.
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r', '\'']) { format!("'{s}") } else { s.to_owned() };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn field_text(f: &Field) -> String {
    match f {
        Field::Text(s) | Field::Secret(s) | Field::Url(s) => s.clone(),
        Field::List(l) => l.join("\n"),
        Field::Int(i) => i.to_string(),
        Field::Bool(b) => b.to_string(),
        Field::Bytes(_) => String::new(),
    }
}

/// GhulPass entries as CSV (`type,title,username,password,url,notes,totp,folder`). Fields of
/// other entry types are written into `notes` as `name: value` lines.
pub(crate) fn export_csv(entries: &[Entry]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(OWN_CSV_COLUMNS.join(",") + "\n");
    for e in entries.iter().filter(|e| !e.deleted) {
        let get = |k: &str| e.fields.get(k).map(field_text).unwrap_or_default();
        let mut notes = get("notes");
        if e.entry_type != "login" && e.entry_type != "note" {
            let extra: Vec<String> = e
                .fields
                .iter()
                .filter(|(k, _)| !["title", "notes"].contains(&k.as_str()))
                .map(|(k, v)| format!("{k}: {}", field_text(v)))
                .collect();
            if !extra.is_empty() {
                notes = if notes.is_empty() { extra.join("\n") } else { format!("{}\n{}", extra.join("\n"), notes) };
            }
        }
        let (username, password, url) = if e.entry_type == "login" {
            (get("username"), get("password"), get("urls"))
        } else {
            (String::new(), String::new(), String::new())
        };
        let kind = if e.entry_type == "login" { "login" } else { "note" };
        let row =
            [kind, &get("title"), &username, &password, &url, &notes, &get("totp"), e.folder.as_deref().unwrap_or("")]
                .iter()
                .map(|s| csv_field(s))
                .collect::<Vec<_>>()
                .join(",");
        out.push_str(&row);
        out.push('\n');
    }
    out
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// GhulAuth entries as `otpauth://` links, one per line.
pub(crate) fn export_otpauth(entries: &[Entry]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::new());
    for e in entries.iter().filter(|e| !e.deleted && e.entry_type == "totp") {
        let Some(Field::Bytes(secret)) = e.fields.get("secret") else { continue };
        let text = |k: &str| e.fields.get(k).map(field_text).unwrap_or_default();
        let int = |k: &str, d: i64| match e.fields.get(k) {
            Some(Field::Int(x)) => *x,
            _ => d,
        };
        let (issuer, account) = (text("issuer"), text("account"));
        let label = if issuer.is_empty() {
            percent_encode(&account)
        } else {
            format!("{}:{}", percent_encode(&issuer), percent_encode(&account))
        };
        let algo = OtpAlgo::from_name(&text("algorithm")).unwrap_or(OtpAlgo::Sha1);
        out.push_str(&format!(
            "otpauth://totp/{label}?secret={}&issuer={}&algorithm={}&digits={}&period={}\n",
            base32_encode(secret),
            percent_encode(&issuer),
            algo.name(),
            int("digits", 6),
            int("period", 30),
        ));
    }
    out
}

/// Normalised secret bytes of a TOTP input (for duplicate detection).
pub(crate) fn totp_secret(input: &TotpInput) -> Option<Zeroizing<Vec<u8>>> {
    base32_decode(&input.secret_base32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn bitwarden_json() {
        let json = r#"{"encrypted":false,"folders":[{"id":"f1","name":"Arbeit"}],"items":[
          {"type":1,"name":"GitHub","folderId":"f1","notes":"n","login":{"username":"ghul","password":"pw",
           "uris":[{"uri":"https://github.com"}],"totp":"otpauth://totp/GitHub:ghul?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"},
           "fields":[{"name":"PIN","value":"1234","type":1}]},
          {"type":2,"name":"Notiz","notes":"geheim"},
          {"type":3,"name":"Visa","card":{"cardholderName":"G Hul","number":"4111","expMonth":"3","expYear":"2030","code":"123"}},
          {"type":5,"name":"SSH"}]}"#;
        let p = parse(json.as_bytes(), "export.json").unwrap();
        assert_eq!(p.format, "Bitwarden");
        assert_eq!((p.entries.len(), p.totps.len(), p.skipped.len()), (3, 1, 1));
        // The SSH key item is not lost: name, reason and the original data come along.
        assert_eq!((p.skipped[0].name.as_str(), p.skipped[0].reason), ("SSH", SkipReason::Unsupported));
        assert!(p.skipped[0].raw.contains("\"SSH\""));
        let gh = &p.entries[0];
        assert_eq!(gh.get("username"), "ghul");
        assert_eq!(gh.fields["PIN"], Field::Secret("1234".into()));
        assert_eq!(gh.folder.as_deref(), Some("Arbeit"));
        assert_eq!(p.entries[2].get("expiry"), "03/2030");
        assert!(p.totps[0].from_login && p.totps[0].input.issuer == "GitHub");
        assert!(matches!(parse(br#"{"encrypted":true,"items":[]}"#, "x.json"), Err(GhulError::ImportEncrypted)));
    }

    #[test]
    fn proton_pass_zip_json_and_csv() {
        let json = r#"{"version":"1.0","userId":"u","encrypted":false,"vaults":{"v1":{"name":"Persönlich","items":[
          {"state":1,"data":{"type":"login","metadata":{"name":"Mail","note":"x"},"content":{"itemEmail":"a@b.de","itemUsername":"",
           "password":"pw","urls":["https://mail.example"],"totpUri":"otpauth://totp/Mail:a%40b.de?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Mail"},
           "extraFields":[{"fieldName":"Kundennr","type":"text","data":{"content":"42"}}]}},
          {"state":2,"data":{"type":"note","metadata":{"name":"Papierkorb","note":"weg"},"content":{}}},
          {"state":1,"data":{"type":"creditCard","metadata":{"name":"Karte","note":""},"content":{"cardholderName":"G","number":"5555","expirationDate":"2030-01","verificationNumber":"1","pin":"9"}}},
          {"state":1,"data":{"type":"alias","metadata":{"name":"Alias","note":""},"content":{}}}]}}}"#;
        let mut zip_bytes = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            z.start_file("Proton Pass/data.json", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, json.as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let p = parse(&zip_bytes, "Proton Pass_export.zip").unwrap();
        assert_eq!(p.format, "Proton Pass");
        // The alias is taken over as a login (nothing skipped any more).
        assert_eq!((p.entries.len(), p.totps.len(), p.skipped.len()), (3, 1, 0));
        assert_eq!(p.entries[0].get("username"), "a@b.de");
        assert_eq!(p.entries[0].get("Kundennr"), "42");
        assert_eq!(p.entries[0].folder.as_deref(), Some("Persönlich"));
        assert_eq!(p.totps[0].input.account, "a@b.de");

        let csv = "type,name,url,email,username,password,note,totp,createTime,modifyTime,vault\n\
                   login,Shop,https://shop.example,a@b.de,shopper,\"p,w\"\"x\",\"zwei\nzeilen\",,1,2,Privat\n\
                   note,Notiz,,,,,text,,1,2,Privat\n";
        let p = parse(csv.as_bytes(), "export.csv").unwrap();
        assert_eq!(p.format, "Proton Pass (CSV)");
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].get("password"), "p,w\"x");
        assert_eq!(p.entries[0].get("notes"), "zwei\nzeilen");
        assert_eq!(p.entries[0].get("email"), "a@b.de");
        assert_eq!(p.entries[1].entry_type, "note");
        assert!(matches!(parse(b"-----BEGIN PGP MESSAGE-----\nxx", "a.pgp"), Err(GhulError::ImportEncrypted)));
    }

    #[test]
    fn lastpass_and_1password_csv() {
        let lp = "url,username,password,totp,extra,name,grouping,fav\n\
            https://example.org,max,geheim1,JBSWY3DPEHPK3PXP,Notiz,Example,Privat,0\n\
            http://sn,,,,Geheime Notiz,Notiz A,,0\n";
        let p = parse(lp.as_bytes(), "lastpass.csv").unwrap();
        assert_eq!(p.format, "LastPass (CSV)");
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].folder.as_deref(), Some("Privat"));
        assert_eq!(p.entries[1].entry_type, "note");
        assert_eq!(p.totps.len(), 1);
        let op = "Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\n\
            Codeberg,https://codeberg.org,ghul,pw2,,false,false,,\n";
        let p = parse(op.as_bytes(), "1password.csv").unwrap();
        assert_eq!(p.format, "1Password (CSV)");
        assert_eq!(p.entries.len(), 1);
    }

    #[test]
    fn onepassword_1pux() {
        let data = serde_json::json!({ "accounts": [{ "vaults": [{ "attrs": { "name": "Privat" }, "items": [
            { "categoryUuid": "001", "state": "active",
              "overview": { "title": "Codeberg", "urls": [{ "url": "https://codeberg.org" }] },
              "details": { "loginFields": [
                  { "designation": "username", "value": "ghul" }, { "designation": "password", "value": "pw" }],
                "notesPlain": "Hinweis",
                "sections": [{ "fields": [{ "id": "otp", "title": "Einmalcode", "value": { "totp": "otpauth://totp/Codeberg:ghul?secret=JBSWY3DPEHPK3PXP&issuer=Codeberg" } }] }] } },
            { "categoryUuid": "002", "overview": { "title": "Visa" }, "details": { "sections": [{ "fields": [
                { "id": "cardholder", "value": { "string": "Max" } }, { "id": "ccnum", "value": { "creditCardNumber": "4111111111111111" } },
                { "id": "expiry", "value": { "monthYear": 202812 } }] }] } },
            { "categoryUuid": "003", "overview": { "title": "Notiz" }, "details": { "notesPlain": "Text" } },
            { "categoryUuid": "001", "state": "archived", "overview": { "title": "Alt" }, "details": {} },
            { "categoryUuid": "110", "overview": { "title": "Server" }, "details": {} }
        ] }] }] });
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("export.attributes", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, b"{}").unwrap();
            z.start_file("export.data", zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut z, data.to_string().as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let p = parse(buf.get_ref(), "export.1pux").unwrap();
        assert_eq!(p.format, "1Password (1PUX)");
        assert_eq!(p.entries.len(), 3, "login, card, note; archived item left out");
        assert_eq!(p.skipped.len(), 1, "unsupported category listed");
        assert_eq!(p.skipped[0].name, "Server");
        assert_eq!(p.totps.len(), 1);
        let card = p.entries.iter().find(|e| e.entry_type == "card").unwrap();
        assert_eq!(card.folder.as_deref(), Some("Privat"));
    }

    #[test]
    fn keepass_and_bitwarden_csv() {
        let kp = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\"\n\
                  \"Root\",\"Bank\",\"me\",\"s3cret\",\"https://bank.example\",\"\",\"\"\n\
                  \"Root/Web\",\"Forum\",\"me2\",\"pw2\",\"\",\"note\",\"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\"\n";
        let p = parse(kp.as_bytes(), "keepass.csv").unwrap();
        assert_eq!(p.format, "KeePass (CSV)");
        assert_eq!((p.entries.len(), p.totps.len()), (2, 1));
        assert_eq!(p.entries[0].folder, None);
        assert_eq!(p.entries[1].folder.as_deref(), Some("Root/Web"));
        assert_eq!(p.totps[0].input.issuer, "Forum");

        let bw = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
                  ,,login,Site,,,0,https://site.example,user,pw,\n,,note,Secret note,content,,0,,,,\n";
        let p = parse(bw.as_bytes(), "bitwarden.csv").unwrap();
        assert_eq!(p.format, "Bitwarden (CSV)");
        assert_eq!(p.entries[1].entry_type, "note");
        assert!(matches!(parse(b"just some text\nwithout columns", "x.txt"), Err(GhulError::ImportUnknownFormat)));
    }

    #[test]
    fn authenticator_formats() {
        let aegis = format!(
            r#"{{"version":1,"header":{{"slots":null,"params":null}},"db":{{"version":2,"entries":[
              {{"type":"totp","name":"me@x","issuer":"X","info":{{"secret":"{SECRET}","algo":"SHA256","digits":8,"period":30}}}},
              {{"type":"hotp","name":"c","issuer":"H","info":{{"secret":"{SECRET}","algo":"SHA1","digits":6,"counter":1}}}}]}}}}"#
        );
        let p = parse(aegis.as_bytes(), "aegis.json").unwrap();
        assert_eq!((p.format.as_str(), p.totps.len(), p.skipped.len()), ("Aegis", 1, 1));
        assert_eq!((p.skipped[0].name.as_str(), p.skipped[0].reason), ("H · c", SkipReason::Hotp));
        assert_eq!((p.totps[0].input.algorithm, p.totps[0].input.digits), (OtpAlgo::Sha256, 8));
        assert!(matches!(parse(br#"{"db":"ENCRYPTED"}"#, "a.json"), Err(GhulError::ImportEncrypted)));

        let twofas = format!(
            r#"{{"services":[{{"name":"GitLab","secret":"{SECRET}","otp":{{"account":"ghul","digits":6,"period":30,"algorithm":"SHA1","tokenType":"TOTP"}}}}],"schemaVersion":4}}"#
        );
        let p = parse(twofas.as_bytes(), "x.2fas").unwrap();
        assert_eq!((p.format.as_str(), p.totps[0].input.issuer.as_str()), ("2FAS", "GitLab"));
        assert!(matches!(
            parse(br#"{"services":[],"servicesEncrypted":"abc"}"#, "x.2fas"),
            Err(GhulError::ImportEncrypted)
        ));

        let proton = format!(
            r#"{{"version":1,"entries":[{{"id":"1","content":{{"uri":"otpauth://totp/Proton:me?secret={SECRET}&issuer=Proton","entry_type":"Totp","name":"Proton"}},"note":null}}]}}"#
        );
        let p = parse(proton.as_bytes(), "proton.json").unwrap();
        assert_eq!((p.format.as_str(), p.totps.len()), ("Proton Authenticator", 1));

        let list = format!("otpauth://totp/A:a?secret={SECRET}\notpauth://hotp/B:b?secret={SECRET}&counter=1\n");
        let p = parse(list.as_bytes(), "list.txt").unwrap();
        assert_eq!((p.totps.len(), p.skipped.len()), (1, 1));
        assert_eq!((p.skipped[0].name.as_str(), p.skipped[0].reason), ("B:b", SkipReason::Hotp));
    }

    #[test]
    fn google_authenticator_migration() {
        // Built by hand: one TOTP (SHA1, 6 digits) and one HOTP account.
        fn ld(field: u8, data: &[u8]) -> Vec<u8> {
            let mut v = vec![(field << 3) | 2, data.len() as u8];
            v.extend_from_slice(data);
            v
        }
        let secret = b"12345678901234567890";
        let mut totp = ld(1, secret);
        totp.extend(ld(2, b"Example:alice@example.com"));
        totp.extend(ld(3, b"Example"));
        totp.extend([4 << 3, 1, 5 << 3, 1, 6 << 3, 2]);
        let mut hotp = ld(1, secret);
        hotp.extend(ld(2, b"counter"));
        hotp.extend([6 << 3, 1]);
        let mut payload = ld(1, &totp);
        payload.extend(ld(1, &hotp));
        payload.extend([2 << 3, 1]);
        const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        for chunk in payload.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |a, (i, b)| a | (*b as u32) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    b64.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    b64.push('=');
                }
            }
        }
        let uri = format!("otpauth-migration://offline?data={}", percent_encode(&b64));
        let p = parse(uri.as_bytes(), "qr").unwrap();
        assert_eq!((p.format.as_str(), p.totps.len(), p.skipped.len()), ("Google Authenticator", 1, 1));
        assert!(p.skipped[0].raw.contains("Secret (Base32)"));
        let t = &p.totps[0].input;
        assert_eq!(
            (t.issuer.as_str(), t.account.as_str(), t.secret_base32.as_str()),
            ("Example", "alice@example.com", SECRET)
        );
        assert!(parse_migration("otpauth-migration://offline?data=%%%").is_err());
    }

    #[test]
    fn export_round_trip() {
        let mut login = Entry::new([1; 16], "login", 0);
        login.fields.insert("title".into(), Field::Text("A, \"B\"".into()));
        login.fields.insert("username".into(), Field::Text("u".into()));
        login.fields.insert("password".into(), Field::Secret("p\nq".into()));
        login.fields.insert("urls".into(), Field::List(vec!["https://a.example".into()]));
        login.folder = Some("Ordner".into());
        let mut card = Entry::new([2; 16], "card", 0);
        card.fields.insert("title".into(), Field::Text("Karte".into()));
        card.fields.insert("number".into(), Field::Secret("4111".into()));
        let csv = export_csv(&[login, card]);
        let p = parse(csv.as_bytes(), "export.csv").unwrap();
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].get("title"), "A, \"B\"");
        assert_eq!(p.entries[0].get("password"), "p\nq");
        assert_eq!(p.entries[0].folder.as_deref(), Some("Ordner"));
        assert_eq!((p.entries[1].entry_type.as_str(), p.entries[1].get("notes").as_str()), ("note", "number: 4111"));

        let mut t = Entry::new([3; 16], "totp", 0);
        t.fields.insert("issuer".into(), Field::Text("Ex ample".into()));
        t.fields.insert("account".into(), Field::Text("a@b".into()));
        t.fields.insert("secret".into(), Field::Bytes(b"12345678901234567890".to_vec()));
        t.fields.insert("algorithm".into(), Field::Text("SHA256".into()));
        t.fields.insert("digits".into(), Field::Int(8));
        t.fields.insert("period".into(), Field::Int(60));
        let text = export_otpauth(&[t]);
        let p = parse(text.as_bytes(), "x.txt").unwrap();
        let i = &p.totps[0].input;
        assert_eq!((i.issuer.as_str(), i.account.as_str(), i.digits, i.period), ("Ex ample", "a@b", 8, 60));
        assert_eq!(i.secret_base32, SECRET);
        assert_eq!(base64_decode("TWFu").unwrap(), b"Man");
    }
}
