// SPDX-License-Identifier: AGPL-3.0-or-later
//! IMAP over TLS (port 993) or STARTTLS (port 143), via async-imap.

use std::collections::HashSet;

use futures::TryStreamExt;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use zeroize::Zeroizing;

use super::{tls, NetError, NetResult};

pub type ImapSession = async_imap::Session<TlsStream<TcpStream>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Security {
    /// TLS from the start (IMAP 993, SMTP 465).
    Tls,
    /// Plain connection upgraded with STARTTLS (IMAP 143, SMTP 587). Never without TLS.
    StartTls,
}

#[derive(Clone)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub security: Security,
    pub user: String,
    pub password: Zeroizing<String>,
    /// Confirmed certificate fingerprint for self-signed certificates.
    pub cert: Option<String>,
}

fn proto(e: impl std::fmt::Display) -> NetError {
    NetError::Protocol(e.to_string())
}

async fn greeting<T>(client: &mut async_imap::Client<T>) -> NetResult<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    match client.read_response().await {
        Some(Ok(_)) => Ok(()),
        Some(Err(e)) => Err(NetError::Connect(e.to_string())),
        None => Err(NetError::Connect("connection closed".into())),
    }
}

/// Connects and logs in.
pub async fn connect(cfg: &ImapConfig) -> NetResult<ImapSession> {
    let tcp = TcpStream::connect((cfg.host.as_str(), cfg.port)).await.map_err(|e| NetError::Connect(e.to_string()))?;
    let client = match cfg.security {
        Security::Tls => {
            let tls = tls::start(tcp, &cfg.host, cfg.cert.as_deref()).await?;
            let mut client = async_imap::Client::new(tls);
            greeting(&mut client).await?;
            client
        }
        Security::StartTls => {
            let mut plain = async_imap::Client::new(tcp);
            greeting(&mut plain).await?;
            plain.run_command_and_check_ok("STARTTLS", None).await.map_err(proto)?;
            let tcp = plain.into_inner();
            async_imap::Client::new(tls::start(tcp, &cfg.host, cfg.cert.as_deref()).await?)
        }
    };
    client.login(&cfg.user, cfg.password.as_str()).await.map_err(|(e, _)| match e {
        async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_) => NetError::AuthFailed,
        other => NetError::Connect(other.to_string()),
    })
}

/// Folder with its special use (`\Sent`, `\Trash`, …).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    pub role: String,
}

pub async fn folders(s: &mut ImapSession) -> NetResult<Vec<Folder>> {
    use async_imap::types::NameAttribute as A;
    let names: Vec<async_imap::types::Name> =
        s.list(Some(""), Some("*")).await.map_err(proto)?.try_collect().await.map_err(proto)?;
    let mut out = Vec::new();
    for n in names {
        if n.attributes().iter().any(|a| matches!(a, A::NoSelect)) {
            continue;
        }
        let name = n.name().to_owned();
        let role = n
            .attributes()
            .iter()
            .find_map(|a| match a {
                A::Sent => Some("sent"),
                A::Trash => Some("trash"),
                A::Drafts => Some("drafts"),
                A::Junk => Some("junk"),
                A::Archive => Some("archive"),
                _ => None,
            })
            .unwrap_or(if name.eq_ignore_ascii_case("INBOX") { "inbox" } else { "" })
            .to_owned();
        out.push(Folder { name, role });
    }
    Ok(out)
}

/// One message as listed: UID, flags, size and the raw header.
pub struct Listed {
    pub uid: u32,
    pub seen: bool,
    pub flagged: bool,
    pub size: u32,
    pub header: Vec<u8>,
    /// Start of the body (for a preview line); may be cut off anywhere.
    pub text: Vec<u8>,
    /// Own keywords set on the message (labels), e.g. "Wichtig".
    pub labels: Vec<String>,
}

fn listed(f: &async_imap::types::Fetch) -> Option<Listed> {
    use async_imap::types::Flag;
    let flags: Vec<Flag<'_>> = f.flags().collect();
    Some(Listed {
        uid: f.uid?,
        seen: flags.iter().any(|x| matches!(x, Flag::Seen)),
        flagged: flags.iter().any(|x| matches!(x, Flag::Flagged)),
        size: f.size.unwrap_or(0),
        header: f.header().unwrap_or_default().to_vec(),
        text: f.text().unwrap_or_default().to_vec(),
        labels: flags
            .iter()
            .filter_map(|x| match x {
                Flag::Custom(c)
                    if !c.starts_with('\\')
                        && !c.starts_with('$')
                        && !c.eq_ignore_ascii_case("NonJunk")
                        && !c.eq_ignore_ascii_case("Junk") =>
                {
                    Some(c.replace('_', " "))
                }
                _ => None,
            })
            .collect(),
    })
}

const LIST_QUERY: &str = "(UID FLAGS RFC822.SIZE BODY.PEEK[HEADER] BODY.PEEK[TEXT]<0.2048>)";

/// Newest messages first: skips `offset`, returns at most `limit`, plus the total count.
pub async fn list(s: &mut ImapSession, folder: &str, offset: u32, limit: u32) -> NetResult<(u32, Vec<Listed>)> {
    let mb = s.select(folder).await.map_err(proto)?;
    let total = mb.exists;
    if total == 0 || offset >= total {
        return Ok((total, Vec::new()));
    }
    let hi = total - offset;
    let lo = hi.saturating_sub(limit.max(1) - 1).max(1);
    let fetches: Vec<async_imap::types::Fetch> =
        s.fetch(format!("{lo}:{hi}"), LIST_QUERY).await.map_err(proto)?.try_collect().await.map_err(proto)?;
    let mut out: Vec<(u32, Listed)> = fetches.iter().filter_map(|f| Some((f.message, listed(f)?))).collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.0));
    Ok((total, out.into_iter().map(|(_, l)| l).collect()))
}

/// Messages matching a search text (newest first, at most `limit`).
pub async fn search(s: &mut ImapSession, folder: &str, text: &str, limit: u32) -> NetResult<Vec<Listed>> {
    s.select(folder).await.map_err(proto)?;
    let q = text.replace(['"', '\\', '\r', '\n'], " ");
    // Not every server accepts CHARSET; then search without it.
    let found: HashSet<u32> = match s.uid_search(format!("CHARSET UTF-8 TEXT \"{q}\"")).await {
        Ok(u) => u,
        Err(_) => s.uid_search(format!("TEXT \"{q}\"")).await.map_err(proto)?,
    };
    let mut uids: Vec<u32> = found.into_iter().collect();
    uids.sort_unstable_by(|a, b| b.cmp(a));
    uids.truncate(limit as usize);
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    let set = uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    let fetches: Vec<async_imap::types::Fetch> =
        s.uid_fetch(set, LIST_QUERY).await.map_err(proto)?.try_collect().await.map_err(proto)?;
    let mut out: Vec<Listed> = fetches.iter().filter_map(listed).collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.uid));
    Ok(out)
}

/// The complete message (RFC 822), without changing its flags.
pub async fn fetch(s: &mut ImapSession, folder: &str, uid: u32) -> NetResult<Vec<u8>> {
    s.select(folder).await.map_err(proto)?;
    let fetches: Vec<async_imap::types::Fetch> =
        s.uid_fetch(uid.to_string(), "(UID BODY.PEEK[])").await.map_err(proto)?.try_collect().await.map_err(proto)?;
    fetches
        .iter()
        .find(|f| f.uid == Some(uid))
        .and_then(|f| f.body())
        .map(<[u8]>::to_vec)
        .ok_or_else(|| NetError::NotFound(format!("message {uid}")))
}

/// Adds (`true`) or removes a flag such as `\Seen` or `\Flagged`.
pub async fn set_flag(s: &mut ImapSession, folder: &str, uid: u32, flag: &str, on: bool) -> NetResult<()> {
    s.select(folder).await.map_err(proto)?;
    let op = if on { "+FLAGS" } else { "-FLAGS" };
    let _: Vec<_> = s
        .uid_store(uid.to_string(), format!("{op} ({flag})"))
        .await
        .map_err(proto)?
        .try_collect()
        .await
        .map_err(proto)?;
    Ok(())
}

/// Moves a message (MOVE, or COPY + delete where MOVE is missing).
pub async fn move_to(s: &mut ImapSession, folder: &str, uid: u32, target: &str) -> NetResult<()> {
    s.select(folder).await.map_err(proto)?;
    let caps = s.capabilities().await.map_err(proto)?;
    if caps.has_str("MOVE") {
        s.uid_mv(uid.to_string(), target).await.map_err(proto)?;
        return Ok(());
    }
    s.uid_copy(uid.to_string(), target).await.map_err(proto)?;
    expunge_uid(s, uid).await
}

/// Deletes a message for good.
pub async fn delete(s: &mut ImapSession, folder: &str, uid: u32) -> NetResult<()> {
    s.select(folder).await.map_err(proto)?;
    expunge_uid(s, uid).await
}

async fn expunge_uid(s: &mut ImapSession, uid: u32) -> NetResult<()> {
    expunge_set(s, &uid.to_string()).await
}

/// At most this many UIDs per command: long UID lists stay well below any server's line limit.
const UID_CHUNK: usize = 400;

/// UID sets for commands: sorted, runs joined (`1:5,9,12:20`), split into chunks.
pub(crate) fn uid_sets(uids: &[u32]) -> Vec<String> {
    let mut sorted: Vec<u32> = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
        .chunks(UID_CHUNK)
        .map(|chunk| {
            let mut parts: Vec<String> = Vec::new();
            let mut i = 0;
            while i < chunk.len() {
                let start = chunk[i];
                let mut end = start;
                while i + 1 < chunk.len() && chunk[i + 1] == end + 1 {
                    i += 1;
                    end = chunk[i];
                }
                parts.push(if start == end { start.to_string() } else { format!("{start}:{end}") });
                i += 1;
            }
            parts.join(",")
        })
        .collect()
}

/// All UIDs of a folder (for "select all mails in this folder").
pub async fn uids(s: &mut ImapSession, folder: &str) -> NetResult<Vec<u32>> {
    s.select(folder).await.map_err(proto)?;
    let mut all: Vec<u32> = s.uid_search("ALL").await.map_err(proto)?.into_iter().collect();
    all.sort_unstable();
    Ok(all)
}

/// [`set_flag`] for many messages.
pub async fn set_flag_many(s: &mut ImapSession, folder: &str, uids: &[u32], flag: &str, on: bool) -> NetResult<()> {
    s.select(folder).await.map_err(proto)?;
    let op = if on { "+FLAGS" } else { "-FLAGS" };
    for set in uid_sets(uids) {
        let _: Vec<_> =
            s.uid_store(set, format!("{op} ({flag})")).await.map_err(proto)?.try_collect().await.map_err(proto)?;
    }
    Ok(())
}

/// [`move_to`] for many messages.
pub async fn move_many(s: &mut ImapSession, folder: &str, uids: &[u32], target: &str) -> NetResult<()> {
    s.select(folder).await.map_err(proto)?;
    let caps = s.capabilities().await.map_err(proto)?;
    let has_move = caps.has_str("MOVE");
    for set in uid_sets(uids) {
        if has_move {
            s.uid_mv(set, target).await.map_err(proto)?;
        } else {
            s.uid_copy(set.clone(), target).await.map_err(proto)?;
            expunge_set(s, &set).await?;
        }
    }
    Ok(())
}

/// Deletes many messages for good.
pub async fn delete_many(s: &mut ImapSession, folder: &str, uids: &[u32]) -> NetResult<()> {
    s.select(folder).await.map_err(proto)?;
    for set in uid_sets(uids) {
        expunge_set(s, &set).await?;
    }
    Ok(())
}

/// Deletes exactly the messages of `set`. Without UIDPLUS a plain EXPUNGE would also remove
/// messages another program has marked as deleted; so then only these are expunged: every other
/// message marked \Deleted is unmarked first and marked again afterwards.
async fn expunge_set(s: &mut ImapSession, set: &str) -> NetResult<()> {
    let _: Vec<_> = s.uid_store(set, "+FLAGS (\\Deleted)").await.map_err(proto)?.try_collect().await.map_err(proto)?;
    let caps = s.capabilities().await.map_err(proto)?;
    if caps.has_str("UIDPLUS") {
        let _: Vec<_> = s.uid_expunge(set).await.map_err(proto)?.try_collect().await.map_err(proto)?;
        return Ok(());
    }
    let mine: std::collections::HashSet<u32> =
        s.uid_search(format!("UID {set}")).await.map_err(proto)?.into_iter().collect();
    let others: Vec<u32> =
        s.uid_search("DELETED").await.map_err(proto)?.into_iter().filter(|u| !mine.contains(u)).collect();
    for o in uid_sets(&others) {
        let _: Vec<_> =
            s.uid_store(o, "-FLAGS (\\Deleted)").await.map_err(proto)?.try_collect().await.map_err(proto)?;
    }
    let _: Vec<_> = s.expunge().await.map_err(proto)?.try_collect().await.map_err(proto)?;
    for o in uid_sets(&others) {
        let _: Vec<_> =
            s.uid_store(o, "+FLAGS (\\Deleted)").await.map_err(proto)?.try_collect().await.map_err(proto)?;
    }
    Ok(())
}

/// Stores a message in a folder (e.g. a sent message in "Sent").
pub async fn append(s: &mut ImapSession, folder: &str, message: &[u8], seen: bool) -> NetResult<()> {
    s.append(folder, if seen { Some("(\\Seen)") } else { None }, None, message).await.map_err(proto)
}

/// Stores a message with a given arrival date, e.g. "02-Mar-2025 10:00:00 +0000" (tests: old mail).
#[cfg(test)]
pub async fn append_dated(s: &mut ImapSession, folder: &str, message: &[u8], date: &str) -> NetResult<()> {
    // async-imap passes the date as it is: quoted here, as RFC 3501 wants it.
    s.append(folder, None, Some(&format!("\"{date}\"")), message).await.map_err(proto)
}

/// Next UID and number of unread messages of a folder (for notifications).
pub async fn status(s: &mut ImapSession, folder: &str) -> NetResult<(u32, u32)> {
    let mb = s.status(folder, "(UIDNEXT UNSEEN)").await.map_err(proto)?;
    Ok((mb.uid_next.unwrap_or(0), mb.unseen.unwrap_or(0)))
}

/// Header of the message with the highest UID in a folder, if any is at or above `from_uid`.
pub async fn newest(s: &mut ImapSession, folder: &str, from_uid: u32) -> NetResult<Option<Listed>> {
    s.examine(folder).await.map_err(proto)?;
    let fetches: Vec<async_imap::types::Fetch> = s
        .uid_fetch(format!("{}:*", from_uid.max(1)), LIST_QUERY)
        .await
        .map_err(proto)?
        .try_collect()
        .await
        .map_err(proto)?;
    Ok(fetches.iter().filter_map(listed).filter(|l| l.uid >= from_uid).max_by_key(|l| l.uid))
}

/// Waits in the inbox until the server reports a change or `wait` has passed (IMAP IDLE, RFC 2177),
/// so new mail is noticed within seconds instead of at the next poll. Servers without IDLE: just
/// waits. Returns the session for further commands.
pub async fn idle_inbox(mut s: ImapSession, wait: std::time::Duration) -> NetResult<ImapSession> {
    let caps = s.capabilities().await.map_err(proto)?;
    if !caps.has_str("IDLE") {
        tokio::time::sleep(wait).await;
        return Ok(s);
    }
    s.examine("INBOX").await.map_err(proto)?;
    let mut idle = s.idle();
    idle.init().await.map_err(proto)?;
    {
        let (changes, _stop) = idle.wait_with_timeout(wait);
        // The inner timeout restarts with every keep-alive of the server; this one does not.
        if let Ok(r) = tokio::time::timeout(wait, changes).await {
            r.map_err(proto)?;
        }
    }
    idle.done().await.map_err(proto)
}

/// Flagged ("starred") messages of a folder, newest first.
pub async fn flagged(s: &mut ImapSession, folder: &str, limit: u32) -> NetResult<Vec<Listed>> {
    s.select(folder).await.map_err(proto)?;
    let mut uids: Vec<u32> = s.uid_search("FLAGGED").await.map_err(proto)?.into_iter().collect();
    uids.sort_unstable_by(|a, b| b.cmp(a));
    uids.truncate(limit as usize);
    if uids.is_empty() {
        return Ok(Vec::new());
    }
    let set = uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
    let fetches: Vec<async_imap::types::Fetch> =
        s.uid_fetch(set, LIST_QUERY).await.map_err(proto)?.try_collect().await.map_err(proto)?;
    let mut out: Vec<Listed> = fetches.iter().filter_map(listed).collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.uid));
    Ok(out)
}

/// A message as the archive takes it: complete and unchanged, with its flags.
pub struct Archivable {
    pub uid: u32,
    pub raw: Vec<u8>,
    pub seen: bool,
    pub flagged: bool,
    pub labels: Vec<String>,
    /// When the server received it (Unix seconds, 0 if unknown).
    pub received: i64,
}

/// IMAP date for SEARCH (`7-Mar-2026`) of a day given as days since 1970-01-01.
pub fn search_date(days: i64) -> String {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let d = crate::app::caldate::Date::from_days(days);
    format!("{}-{}-{}", d.d, M[(d.m - 1) as usize], d.y)
}

/// UIDVALIDITY of a folder (0 if the server sends none) and the UIDs of the messages the server
/// received before the day `before` (days since 1970-01-01). Read-only (EXAMINE).
pub async fn old_uids(s: &mut ImapSession, folder: &str, before: i64) -> NetResult<(u32, Vec<u32>)> {
    let mb = s.examine(folder).await.map_err(proto)?;
    let validity = mb.uid_validity.unwrap_or(0);
    if mb.exists == 0 {
        return Ok((validity, Vec::new()));
    }
    let mut uids: Vec<u32> =
        s.uid_search(format!("BEFORE {}", search_date(before))).await.map_err(proto)?.into_iter().collect();
    uids.sort_unstable();
    uids.dedup();
    Ok((validity, uids))
}

/// One complete message for the archive, without changing its flags (EXAMINE, BODY.PEEK).
/// `None` if the folder has another UIDVALIDITY now or the message is gone.
pub async fn fetch_archivable(
    s: &mut ImapSession,
    folder: &str,
    validity: u32,
    uid: u32,
) -> NetResult<Option<Archivable>> {
    use async_imap::types::Flag;
    let mb = s.examine(folder).await.map_err(proto)?;
    if mb.uid_validity.unwrap_or(0) != validity {
        return Ok(None);
    }
    let fetches: Vec<async_imap::types::Fetch> = s
        .uid_fetch(uid.to_string(), "(UID FLAGS INTERNALDATE BODY.PEEK[])")
        .await
        .map_err(proto)?
        .try_collect()
        .await
        .map_err(proto)?;
    let Some(f) = fetches.iter().find(|f| f.uid == Some(uid)) else { return Ok(None) };
    let Some(raw) = f.body() else { return Ok(None) };
    let flags: Vec<Flag<'_>> = f.flags().collect();
    Ok(Some(Archivable {
        uid,
        raw: raw.to_vec(),
        seen: flags.iter().any(|x| matches!(x, Flag::Seen)),
        flagged: flags.iter().any(|x| matches!(x, Flag::Flagged)),
        labels: listed(f).map(|l| l.labels).unwrap_or_default(),
        received: f.internal_date().map(|d| d.timestamp()).unwrap_or(0),
    }))
}

/// Deletes exactly these messages for good, but only while the folder still has UIDVALIDITY
/// `validity` (otherwise the UIDs could name other messages now). Returns whether it deleted.
pub async fn delete_if_valid(s: &mut ImapSession, folder: &str, validity: u32, uids: &[u32]) -> NetResult<bool> {
    let mb = s.select(folder).await.map_err(proto)?;
    if validity == 0 || mb.uid_validity.unwrap_or(0) != validity {
        return Ok(false);
    }
    for set in uid_sets(uids) {
        expunge_set(s, &set).await?;
    }
    Ok(true)
}

#[cfg(test)]
mod uid_set_tests {
    use super::uid_sets;

    #[test]
    fn uid_sets_join_runs_and_split_long_lists() {
        assert_eq!(uid_sets(&[5, 1, 2, 3, 9, 9, 12, 13]), vec!["1:3,5,9,12:13".to_string()]);
        assert!(uid_sets(&[]).is_empty());
        let many: Vec<u32> = (1..=1000).map(|i| i * 2).collect();
        let sets = uid_sets(&many);
        assert_eq!(sets.len(), 3, "400 UIDs per command");
        // Every UID appears exactly once over all sets.
        let back: Vec<u32> = sets.iter().flat_map(|s| s.split(',')).map(|p| p.parse::<u32>().unwrap()).collect();
        assert_eq!(back, many);
    }

    #[test]
    fn search_dates_in_imap_form() {
        assert_eq!(super::search_date(0), "1-Jan-1970");
        assert_eq!(super::search_date(20_520), "8-Mar-2026");
        assert_eq!(super::search_date(19_782), "29-Feb-2024");
    }
}
