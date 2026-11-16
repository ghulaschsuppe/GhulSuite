<?php
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// GhulSuite server module: share links on any web hosting with PHP 8.1+, no database.
//
// The GhulSuite app installs this file over SFTP or FTPS next to the website and fills in where
// the GhulSuite folder of the same server is. Do not edit by hand.
//
// It has no write access over the web and holds no secret. The app writes a small description of
// each share (`shares/<id>.json`) into the GhulSuite folder over SFTP/FTPS; this file only reads
// it and hands out the encrypted GhulDrive blocks of that share, nothing else. Everything is
// encrypted on the device; the keys are only in the link after `#`, which browsers never send.
//
//   GET                       the page that decrypts a share in the recipient's browser
//   GET ?s=ID                 opens a share: expiry, download limit and one-time links enforced
//                             here; answers the encrypted package and a ticket for the blocks
//   GET ?s=ID&t=T&f=F&b=N     block N of file F of the share, with the ticket
//   GET ?v                    version and whether the GhulSuite folder can be read (for the app)
//   GET ?check=NAME           content of a test file the app wrote (proves the folder is right)
//                             and whether PHP may write the state of shares there

declare(strict_types=1);

const GHUL_VERSION = 3;
const GHUL_BASE = '__GHUL_BASE__';
const GHUL_VIEWER_HTML = '__GHUL_VIEWER_HTML__';
const GHUL_VIEWER_JS = '__GHUL_VIEWER_JS__';
const GHUL_VIEWER_CSS = '__GHUL_VIEWER_CSS__';
/// A ticket for the blocks of a share is valid this long (seconds) after opening it.
const GHUL_TICKET_SECONDS = 86400;
/// Valid tickets per share at most; further openings are refused until some run out, so a
/// flood of openings cannot push out the tickets of people already downloading.
const GHUL_MAX_TICKETS = 1000;

header('X-Content-Type-Options: nosniff');
header('Referrer-Policy: no-referrer');
header('X-Robots-Tag: noindex, nofollow');
header('Cache-Control: no-store');
header('X-Frame-Options: DENY');

function ghul_fail(int $code): never
{
    http_response_code($code);
    header('Content-Type: text/plain; charset=utf-8');
    exit;
}

function ghul_json(mixed $value): never
{
    header('Content-Type: application/json');
    echo json_encode($value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    exit;
}

/** The GhulSuite folder of this server (filled in by the app at installation). */
function ghul_base(): string
{
    $b = GHUL_BASE;
    return str_starts_with($b, '/') ? $b : __DIR__ . '/' . $b;
}

/** Only exact, fixed-length lowercase hex: no path can be built from anything else. */
function ghul_hex(mixed $v, int $len): string
{
    $v = is_string($v) ? $v : '';
    if (strlen($v) !== $len || preg_match('/^[0-9a-f]+$/D', $v) !== 1) {
        ghul_fail(404);
    }
    return $v;
}

function ghul_index(mixed $v): int
{
    $v = is_string($v) ? $v : '';
    if (preg_match('/^(0|[1-9][0-9]{0,8})$/D', $v) !== 1) {
        ghul_fail(404);
    }
    return (int) $v;
}

/** The share as the app described it, or 404. */
function ghul_share(string $id): array
{
    $raw = @file_get_contents(ghul_base() . '/shares/' . $id . '.json');
    $m = $raw === false ? null : json_decode($raw, true);
    if (!is_array($m) || !is_string($m['package'] ?? null) || !is_array($m['files'] ?? null)) {
        ghul_fail(404);
    }
    $now = (int) (microtime(true) * 1000);
    if ((int) ($m['exp'] ?? 0) > 0 && (int) $m['exp'] < $now) {
        ghul_fail(410);
    }
    return $m;
}

/** The state file's content as an array; missing = nobody opened the share yet. */
function ghul_state_parse(string|false $raw): array
{
    if ($raw === false) {
        return ['count' => 0, 'tickets' => []];
    }
    $s = json_decode($raw, true);
    if (!is_array($s)) {
        // Empty or broken: never start counting from zero again (a used-up link would reopen).
        ghul_fail(503);
    }
    return $s;
}

/**
 * Runs $f on the share's state (downloads, tickets) under a lock and stores what it returns,
 * unless it returns null (nothing changed). The new state is written to a temporary file and
 * renamed over the old one: an interrupted write or a full disk leaves the old state as it was.
 * The state is the only thing this file writes.
 */
function ghul_state(string $id, callable $f): void
{
    $dir = ghul_base() . '/shares/';
    $lock = @fopen($dir . $id . '.lock', 'c');
    if ($lock === false || !flock($lock, LOCK_EX)) {
        ghul_fail(503);
    }
    $s = $f(ghul_state_parse(@file_get_contents($dir . $id . '.state')));
    if ($s !== null) {
        $tmp = $dir . $id . '.state.tmp';
        $json = (string) json_encode($s, JSON_FORCE_OBJECT);
        if (@file_put_contents($tmp, $json) !== strlen($json) || !@rename($tmp, $dir . $id . '.state')) {
            @unlink($tmp);
            ghul_fail(503);
        }
    }
    flock($lock, LOCK_UN);
    fclose($lock);
}

/** The share's state, read without writing (it is only ever replaced as a whole). */
function ghul_state_read(string $id): array
{
    return ghul_state_parse(@file_get_contents(ghul_base() . '/shares/' . $id . '.state'));
}

/** GET ?s=ID: counts the download (refused once the limit is reached) and hands out a ticket. */
function ghul_open_share(string $id): never
{
    $m = ghul_share($id);
    $max = max(0, (int) ($m['max'] ?? 0));
    $ticket = bin2hex(random_bytes(16));
    $refused = 0;
    ghul_state($id, function (array $s) use ($max, $ticket, &$refused): ?array {
        if ($max > 0 && (int) ($s['count'] ?? 0) >= $max) {
            $refused = 410;
            return null;
        }
        $now = time();
        $tickets = array_filter((array) ($s['tickets'] ?? []), fn($until) => (int) $until > $now);
        if (count($tickets) >= GHUL_MAX_TICKETS) {
            $refused = 429;
            return null;
        }
        $tickets[hash('sha256', $ticket)] = $now + GHUL_TICKET_SECONDS;
        $s['tickets'] = $tickets;
        $s['count'] = (int) ($s['count'] ?? 0) + 1;
        return $s;
    });
    if ($refused !== 0) {
        ghul_fail($refused);
    }
    $blocks = [];
    foreach ($m['files'] as $f) {
        $blocks[] = array_values(array_filter((array) $f, 'is_string'));
    }
    ghul_json(['package' => $m['package'], 'blocks' => $blocks, 'ticket' => $ticket]);
}

/** GET ?s=ID&t=T&f=F&b=N: one encrypted block of the share, only with a valid ticket. */
function ghul_block(string $id): never
{
    $m = ghul_share($id);
    $ticket = ghul_hex($_GET['t'] ?? null, 32);
    $f = ghul_index($_GET['f'] ?? null);
    $n = ghul_index($_GET['b'] ?? null);
    $s = ghul_state_read($id);
    $until = (int) (($s['tickets'] ?? [])[hash('sha256', $ticket)] ?? 0);
    if ($until < time()) {
        ghul_fail(403);
    }
    $block = ghul_hex(($m['files'][$f] ?? [])[$n] ?? null, 64);
    $path = ghul_base() . '/drive/blocks/' . substr($block, 0, 2) . '/' . $block;
    if (!is_file($path)) {
        ghul_fail(404);
    }
    header('Content-Type: application/octet-stream');
    header('Content-Disposition: attachment');
    header('Content-Length: ' . filesize($path));
    readfile($path);
    exit;
}

function ghul_viewer(): never
{
    header("Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'");
    $v = $_GET['asset'] ?? '';
    if ($v === 'js') {
        header('Content-Type: text/javascript; charset=utf-8');
        echo base64_decode(GHUL_VIEWER_JS);
        exit;
    }
    if ($v === 'css') {
        header('Content-Type: text/css; charset=utf-8');
        echo base64_decode(GHUL_VIEWER_CSS);
        exit;
    }
    header('Content-Type: text/html; charset=utf-8');
    $self = rawurlencode(basename(__FILE__));
    echo str_replace(
        ['href="viewer.css"', 'src="viewer.js"'],
        ['href="' . $self . '?asset=css"', 'src="' . $self . '?asset=js"'],
        base64_decode(GHUL_VIEWER_HTML)
    );
    exit;
}

// --- routing -----------------------------------------------------------------------------------
if (($_SERVER['REQUEST_METHOD'] ?? 'GET') !== 'GET') {
    ghul_fail(405);
}
if (isset($_GET['v'])) {
    ghul_json(['version' => GHUL_VERSION, 'folder' => is_dir(ghul_base() . '/drive') || is_dir(ghul_base() . '/suite')]);
}
if (isset($_GET['check'])) {
    $name = ghul_hex($_GET['check'], 32);
    $raw = @file_get_contents(ghul_base() . '/shares/check-' . $name);
    if ($raw === false) {
        ghul_fail(404);
    }
    // Opening a share writes its state next to the descriptions: PHP must be allowed to.
    $probe = ghul_base() . '/shares/write-' . $name;
    $writes = @file_put_contents($probe, 'x') === 1 && @unlink($probe);
    ghul_json(['token' => $raw, 'write' => $writes]);
}
if (isset($_GET['s'])) {
    $id = ghul_hex($_GET['s'], 32);
    if (isset($_GET['t']) || isset($_GET['b'])) {
        ghul_block($id);
    }
    ghul_open_share($id);
}
ghul_viewer();
