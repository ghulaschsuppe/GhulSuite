// SPDX-License-Identifier: AGPL-3.0-or-later
//! HTML mail as simple building blocks for the app: paragraphs, headings, list items, quotes,
//! rules and images with plain text spans (bold, italic, link).
//!
//! Nothing of the HTML is ever executed or shown as HTML: scripts, styles, forms, frames and
//! comments are dropped, links are only kept for `https`, `http` and `mailto`, and images become
//! placeholders, inline images (`cid:`) are shown, remote images only when the user asks for them
//! (they can tell the sender that and when a mail was read). Tracking pixels (1×1, hidden) are
//! removed completely.

/// Upper limit for the blocks of one mail, so a hostile mail cannot flood the screen.
const MAX_BLOCKS: usize = 2_000;

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailBlockKind {
    Paragraph,
    Heading,
    ListItem,
    Quote,
    Rule,
    Image,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq, Default)]
pub struct MailSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// Link target (https, http or mailto); empty for plain text.
    pub link: String,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MailBlock {
    pub kind: MailBlockKind,
    /// Heading level 1 to 6, list nesting, quote depth.
    pub level: u8,
    pub spans: Vec<MailSpan>,
    /// Images: `cid:…` for inline images, otherwise the https/http address.
    pub image_src: String,
    pub image_alt: String,
}

impl MailBlock {
    fn new(kind: MailBlockKind, level: u8) -> Self {
        MailBlock { kind, level, spans: vec![], image_src: String::new(), image_alt: String::new() }
    }
}

/// Parsed HTML of a mail.
#[derive(Debug, Default)]
pub struct HtmlMail {
    pub blocks: Vec<MailBlock>,
    /// Number of images from the internet (shown only on request).
    pub remote_images: u32,
}

struct Tag {
    name: String,
    closing: bool,
    attrs: Vec<(String, String)>,
}

impl Tag {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// Splits HTML into text pieces and tags. Tolerant of broken markup.
fn tokenize(html: &str) -> Vec<Result<String, Tag>> {
    let mut out = Vec::new();
    let b = html.as_bytes();
    let mut i = 0;
    let mut text = String::new();
    while i < b.len() {
        if b[i] == b'<' {
            if html[i..].starts_with("<!--") {
                i = html[i..].find("-->").map(|e| i + e + 3).unwrap_or(b.len());
                continue;
            }
            let Some(end) = html[i..].find('>').map(|e| i + e) else { break };
            let inner = &html[i + 1..end];
            i = end + 1;
            if inner.starts_with('!') || inner.starts_with('?') {
                continue;
            }
            if !text.is_empty() {
                out.push(Ok(std::mem::take(&mut text)));
            }
            out.push(Err(parse_tag(inner)));
        } else {
            let next = html[i..].find('<').map(|e| i + e).unwrap_or(b.len());
            text.push_str(&html[i..next]);
            i = next;
        }
    }
    if !text.is_empty() {
        out.push(Ok(text));
    }
    out
}

fn parse_tag(inner: &str) -> Tag {
    let inner = inner.trim().trim_end_matches('/').trim();
    let closing = inner.starts_with('/');
    let inner = inner.trim_start_matches('/');
    let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let rest = &inner[name_end..];
    let rb = rest.as_bytes();
    let mut j = 0;
    while j < rb.len() {
        while j < rb.len() && rb[j].is_ascii_whitespace() {
            j += 1;
        }
        let start = j;
        while j < rb.len() && !rb[j].is_ascii_whitespace() && rb[j] != b'=' {
            j += 1;
        }
        let key = rest[start..j].to_ascii_lowercase();
        while j < rb.len() && rb[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut value = String::new();
        if j < rb.len() && rb[j] == b'=' {
            j += 1;
            while j < rb.len() && rb[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < rb.len() && (rb[j] == b'"' || rb[j] == b'\'') {
                let q = rb[j];
                j += 1;
                let vs = j;
                while j < rb.len() && rb[j] != q {
                    j += 1;
                }
                value = decode_entities(&rest[vs..j]);
                j += 1;
            } else {
                let vs = j;
                while j < rb.len() && !rb[j].is_ascii_whitespace() {
                    j += 1;
                }
                value = decode_entities(&rest[vs..j]);
            }
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
        if j == start {
            j += 1;
        }
    }
    Tag { name, closing, attrs }
}

/// Decodes the usual named and numeric character references.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace()).map(|e| e + 1);
        match end {
            Some(e) if rest.as_bytes().get(e) == Some(&b';') && e <= 12 => {
                let name = &rest[1..e];
                let ch = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" | "#39" => Some('\''),
                    "nbsp" => Some(' '),
                    "ndash" => Some('\u{2013}'),
                    "mdash" => Some('\u{2014}'),
                    "hellip" => Some('…'),
                    "euro" => Some('€'),
                    "copy" => Some('©'),
                    "reg" => Some('®'),
                    "auml" => Some('ä'),
                    "ouml" => Some('ö'),
                    "uuml" => Some('ü'),
                    "Auml" => Some('Ä'),
                    "Ouml" => Some('Ö'),
                    "Uuml" => Some('Ü'),
                    "szlig" => Some('ß'),
                    "laquo" => Some('«'),
                    "raquo" => Some('»'),
                    "bdquo" => Some('„'),
                    "ldquo" => Some('“'),
                    "rdquo" => Some('”'),
                    "lsquo" => Some('‘'),
                    "rsquo" => Some('’'),
                    "zwnj" | "zwj" | "shy" => Some('\u{200b}'),
                    n if n.starts_with("#x") || n.starts_with("#X") => {
                        u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32)
                    }
                    n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                match ch {
                    Some('\u{200b}') => {}
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[..=e]),
                }
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn safe_link(href: &str) -> String {
    let h = href.trim();
    let lower = h.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:") {
        h.to_owned()
    } else {
        String::new()
    }
}

fn is_tracking_pixel(t: &Tag) -> bool {
    let small =
        |a: &str| t.attr(a).and_then(|v| v.trim_end_matches("px").trim().parse::<f32>().ok()).is_some_and(|v| v <= 2.0);
    let style = t.attr("style").unwrap_or("").to_ascii_lowercase().replace(' ', "");
    small("width") || small("height") || style.contains("display:none") || style.contains("visibility:hidden")
}

/// Turns the HTML of a mail into blocks (see the module description).
pub fn parse(html: &str) -> HtmlMail {
    let mut mail = HtmlMail::default();
    let mut current = MailBlock::new(MailBlockKind::Paragraph, 0);
    let (mut bold, mut italic) = (0u32, 0u32);
    let mut links: Vec<String> = vec![];
    let mut quote = 0u8;
    let mut lists = 0u8;
    let mut skip: Option<String> = None;
    let mut pre = 0u32;

    fn flush(mail: &mut HtmlMail, b: &mut MailBlock, next: MailBlock) {
        let old = std::mem::replace(b, next);
        // Trim and drop empty text blocks.
        let mut old = old;
        if let Some(f) = old.spans.first_mut() {
            f.text = f.text.trim_start().to_owned();
        }
        if let Some(l) = old.spans.last_mut() {
            l.text = l.text.trim_end().to_owned();
        }
        old.spans.retain(|s| !s.text.is_empty());
        let keep = matches!(old.kind, MailBlockKind::Rule | MailBlockKind::Image) || !old.spans.is_empty();
        if keep && mail.blocks.len() < MAX_BLOCKS {
            mail.blocks.push(old);
        }
    }
    let base = |quote: u8| {
        if quote > 0 {
            MailBlock::new(MailBlockKind::Quote, quote)
        } else {
            MailBlock::new(MailBlockKind::Paragraph, 0)
        }
    };

    for tok in tokenize(html) {
        match tok {
            Ok(raw) => {
                if skip.is_some() {
                    continue;
                }
                let text = decode_entities(&raw);
                let text = if pre > 0 {
                    text
                } else {
                    let mut t = String::with_capacity(text.len());
                    let mut space = false;
                    for c in text.chars() {
                        if c.is_whitespace() {
                            if !space {
                                t.push(' ');
                            }
                            space = true;
                        } else {
                            t.push(c);
                            space = false;
                        }
                    }
                    t
                };
                if text.is_empty() {
                    continue;
                }
                let span = MailSpan {
                    text,
                    bold: bold > 0,
                    italic: italic > 0,
                    link: links.last().cloned().unwrap_or_default(),
                };
                match current.spans.last_mut() {
                    Some(l) if l.bold == span.bold && l.italic == span.italic && l.link == span.link => {
                        l.text.push_str(&span.text)
                    }
                    _ => current.spans.push(span),
                }
            }
            Err(tag) => {
                let n = tag.name.as_str();
                if let Some(s) = &skip {
                    if tag.closing && n == s {
                        skip = None;
                    }
                    continue;
                }
                match (n, tag.closing) {
                    (
                        "script" | "style" | "head" | "title" | "template" | "noscript" | "select" | "textarea" | "svg",
                        false,
                    ) => skip = Some(n.to_owned()),
                    ("b" | "strong", c) => bold = if c { bold.saturating_sub(1) } else { bold + 1 },
                    ("i" | "em", c) => italic = if c { italic.saturating_sub(1) } else { italic + 1 },
                    ("a", false) => links.push(tag.attr("href").map(safe_link).unwrap_or_default()),
                    ("a", true) => {
                        links.pop();
                    }
                    ("br", _) => current.spans.push(MailSpan {
                        text: "\n".into(),
                        bold: false,
                        italic: false,
                        link: String::new(),
                    }),
                    ("pre", c) => {
                        pre = if c { pre.saturating_sub(1) } else { pre + 1 };
                        let next = base(quote);
                        flush(&mut mail, &mut current, next);
                    }
                    ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                        let level = n[1..].parse().unwrap_or(1);
                        flush(&mut mail, &mut current, MailBlock::new(MailBlockKind::Heading, level));
                        bold += 1;
                    }
                    ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => {
                        bold = bold.saturating_sub(1);
                        let next = base(quote);
                        flush(&mut mail, &mut current, next);
                    }
                    ("ul" | "ol", c) => {
                        lists = if c { lists.saturating_sub(1) } else { lists.saturating_add(1) };
                        let next = base(quote);
                        flush(&mut mail, &mut current, next);
                    }
                    ("li", false) => {
                        flush(&mut mail, &mut current, MailBlock::new(MailBlockKind::ListItem, lists.max(1)))
                    }
                    ("blockquote", c) => {
                        quote = if c { quote.saturating_sub(1) } else { quote.saturating_add(1) };
                        let next = base(quote);
                        flush(&mut mail, &mut current, next);
                    }
                    ("hr", _) => {
                        flush(&mut mail, &mut current, MailBlock::new(MailBlockKind::Rule, 0));
                        let next = base(quote);
                        flush(&mut mail, &mut current, next);
                    }
                    ("td" | "th", false) => {
                        if current.spans.last().is_some_and(|s| !s.text.trim().is_empty()) {
                            current.spans.push(MailSpan { text: "  ".into(), ..Default::default() });
                        }
                    }
                    (
                        "p" | "div" | "tr" | "table" | "section" | "article" | "header" | "footer" | "center" | "li"
                        | "dt" | "dd" | "address" | "figure",
                        _,
                    ) => {
                        let next = base(quote);
                        flush(&mut mail, &mut current, next);
                    }
                    ("img", false) => {
                        let src = tag.attr("src").unwrap_or("").trim().to_owned();
                        let lower = src.to_ascii_lowercase();
                        if is_tracking_pixel(&tag)
                            || !(lower.starts_with("cid:")
                                || lower.starts_with("https://")
                                || lower.starts_with("http://"))
                        {
                            continue;
                        }
                        if !lower.starts_with("cid:") {
                            mail.remote_images += 1;
                        }
                        let mut img = MailBlock::new(MailBlockKind::Image, 0);
                        img.image_src = src;
                        img.image_alt = tag.attr("alt").unwrap_or("").trim().to_owned();
                        let next = base(quote);
                        flush(&mut mail, &mut current, img);
                        flush(&mut mail, &mut current, next);
                    }
                    _ => {}
                }
            }
        }
    }
    let next = base(0);
    flush(&mut mail, &mut current, next);
    reveal_deceptive_links(&mut mail);
    mail
}

/// A link whose text looks like an address but points somewhere else ("https://bank.example"
/// leading to evil.example), a classic phishing trick. The real destination is added to the
/// text, so the reader sees where the link goes.
fn reveal_deceptive_links(mail: &mut HtmlMail) {
    for b in &mut mail.blocks {
        for span in &mut b.spans {
            if span.link.is_empty() {
                continue;
            }
            let shown = span.text.trim();
            let looks_like_address = !shown.contains(char::is_whitespace)
                && (shown.contains("://")
                    || shown.starts_with("www.")
                    || shown.matches('.').count() >= 1 && shown.len() > 3);
            if !looks_like_address {
                continue;
            }
            let target = super::autofill::host_of(&span.link);
            let claimed = super::autofill::host_of(shown);
            if let (Some(t), Some(c)) = (target, claimed) {
                if super::autofill::base_domain(&t) != super::autofill::base_domain(&c) {
                    span.text = format!("{shown} \u{2192} {t}");
                }
            }
        }
    }
}

/// Whether an address is on the public internet (not this device, the home network, link-local,
/// carrier NAT, multicast or reserved ranges).
pub(crate) fn public_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (64..128).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..20).contains(&o[1]))
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && s[1] == 0x0db8)
                || (s[0] == 0x64 && s[1] == 0xff9b))
        }
    }
}

/// https to a host whose every address is public.
fn public_https(u: &crate::net::http::Url) -> bool {
    public_address(u).is_some()
}

/// For https to a host whose every address is public: the address to connect to. The image is
/// then fetched from exactly this address (B16: a second lookup could return a private one).
fn public_address(u: &crate::net::http::Url) -> Option<std::net::SocketAddr> {
    use std::net::ToSocketAddrs;
    if !u.tls {
        return None;
    }
    if let Ok(ip) = u.host.parse::<std::net::IpAddr>() {
        return public_ip(ip).then(|| std::net::SocketAddr::new(ip, u.port));
    }
    let name = u.host.trim_end_matches('.');
    if name == "localhost"
        || name.ends_with(".localhost")
        || name.ends_with(".local")
        || name.ends_with(".internal")
        || !name.contains('.')
    {
        return None;
    }
    let addrs: Vec<std::net::SocketAddr> = (name, u.port).to_socket_addrs().ok()?.collect();
    (!addrs.is_empty() && addrs.iter().all(|a| public_ip(a.ip()))).then(|| addrs[0])
}

/// Loads an image from the internet for an HTML mail, only when the user asked for it.
/// At most 5 MB and three redirects; only answers that are images.
#[uniffi::export]
pub fn mail_fetch_image(url: String) -> Result<Vec<u8>, super::error::GhulError> {
    use crate::net::http;
    let mut url = url;
    for _ in 0..4 {
        let u = http::Url::parse(&url).map_err(|e| super::error::GhulError::Network { msg: e.to_string() })?;
        // A mail must not make the device call itself, the router or other devices at home
        // (directly or through a redirect): only https to public addresses.
        let Some(addr) = public_address(&u) else { break };
        let r = crate::net::block_on(http::request_at(addr, "GET", &u, &[], b""), std::time::Duration::from_secs(15))
            .map_err(|e| super::error::GhulError::Network { msg: e.to_string() })?;
        if (300..400).contains(&r.status) {
            let Some(loc) = r.header("location") else { break };
            url = if loc.starts_with("http") { loc.to_owned() } else { u.join(loc).to_string() };
            continue;
        }
        let image = r.header("content-type").is_some_and(|t| t.trim().to_ascii_lowercase().starts_with("image/"));
        if r.status != 200 || !image || r.body.len() > 5 * 1024 * 1024 {
            break;
        }
        return Ok(r.body);
    }
    Err(super::error::GhulError::Network { msg: "image not available".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &MailBlock) -> String {
        b.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Images in mails never reach this device, the home network or other private ranges.
    #[test]
    fn mail_images_cannot_reach_private_addresses() {
        let private = [
            "127.0.0.1",
            "127.8.9.1",
            "10.0.0.1",
            "172.16.5.4",
            "172.31.255.255",
            "192.168.178.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
            "192.0.0.8",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.1.1",
            "ff02::1",
            "64:ff9b::7f00:1",
        ];
        for ip in private {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "1.1.1.1", "8.8.8.8", "172.32.0.1", "2606:4700::1111", "2a00:1450:4001::1"] {
            assert!(public_ip(ip.parse().unwrap()), "{ip}");
        }
        for url in [
            "http://127.0.0.1:8080/x.png",
            "http://localhost/x.png",
            "https://localhost/x.png",
            "https://127.0.0.1/x.png",
            "https://192.168.178.1/logo.png",
            "https://[::1]/x.png",
            "https://router/x.png",
            "https://fritz.box.local/x.png",
            "https://169.254.169.254/latest/meta-data",
            "https://0x7f000001/x.png",
            "https://service.internal/x.png",
        ] {
            assert!(mail_fetch_image(url.into()).is_err(), "{url}");
            if let Ok(u) = crate::net::http::Url::parse(url) {
                assert!(!public_https(&u), "{url}");
            }
        }
    }

    #[test]
    fn newsletter_becomes_safe_blocks() {
        let html = r#"<html><head><title>T</title><style>p{color:red}</style>
            <script>alert(1)</script></head><body>
            <h1>Hallo &amp; willkommen</h1>
            <p>Das ist <b>wichtig</b> und <i>schön</i>.<br>Neue Zeile &euro; 5&nbsp;&#8364;</p>
            <p><a href="https://example.org/x">Zum Angebot</a> <a href="javascript:evil()">Böse</a></p>
            <ul><li>Eins</li><li>Zwei</li></ul>
            <blockquote>Zitat</blockquote><hr>
            <img src="cid:logo@x" alt="Logo"><img src="https://t.example/p.gif" width="1" height="1">
            <img src="https://img.example/banner.png" alt="Banner">
            <form><input value="x">Formular</form><!-- Kommentar --></body></html>"#;
        let m = parse(html);
        let kinds: Vec<_> = m.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(m.blocks[0].kind, MailBlockKind::Heading);
        assert_eq!(text(&m.blocks[0]), "Hallo & willkommen");
        let p = &m.blocks[1];
        assert_eq!(text(p), "Das ist wichtig und schön.\nNeue Zeile € 5 €");
        assert!(p.spans.iter().any(|s| s.bold && s.text == "wichtig"));
        assert!(p.spans.iter().any(|s| s.italic && s.text == "schön"));
        let links = &m.blocks[2];
        assert!(links.spans.iter().any(|s| s.link == "https://example.org/x" && s.text == "Zum Angebot"));
        assert!(links.spans.iter().all(|s| !s.link.contains("javascript")), "script links are dropped");
        assert!(
            kinds.contains(&MailBlockKind::ListItem)
                && kinds.contains(&MailBlockKind::Quote)
                && kinds.contains(&MailBlockKind::Rule)
        );
        let images: Vec<_> = m.blocks.iter().filter(|b| b.kind == MailBlockKind::Image).collect();
        assert_eq!(images.len(), 2, "tracking pixel removed");
        assert_eq!(images[0].image_src, "cid:logo@x");
        assert_eq!(m.remote_images, 1);
        let all: String = m.blocks.iter().map(text).collect();
        assert!(
            !all.contains("alert") && !all.contains("color:red") && !all.contains("Kommentar") && !all.contains("T")
        );
    }

    #[test]
    fn broken_markup_does_not_panic() {
        for html in ["<", "<p", "<a href=", "&#xFFFFFFFF;", "<b><i>x</b>", "<img src='cid:", "&&&;;", "<h7>x</h7>"] {
            let _ = parse(html);
        }
        assert_eq!(decode_entities("a&amp;b&unknown;c&#65;"), "a&b&unknown;cA");
    }
}
