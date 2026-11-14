<?php
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// GhulSuite: receives an error report sent from the app (Options > About > Error report) and
// forwards it by e-mail to the developer. Nothing is stored except short-lived counters against
// spam (hashed, deleted after a day). PHP 8.1, no database.
//
// Protection against spam and abuse:
//   - only POST with JSON, at most 64 KiB; report at most 32 KiB, note 4,000 characters,
//   - the report must look like one from the app ("GhulSuite <version>" in the first line),
//   - proof of work: SHA-256("ghulsuite-report|<time>|<content hash>|<nonce>") must start with
//     `bits` zero bits; the time must be within 15 minutes; each proof is accepted once,
//   - at most 5 reports per hour from one address (the address is only kept as a salted hash),
//     at most 100 per day in total.
// Configuration: report-config.php next to this file (written when the site is uploaded):
//   <?php return ['to' => 'developer@example.org', 'bits' => 20];

declare(strict_types=1);

header('Content-Type: application/json; charset=utf-8');
header('Cache-Control: no-store');
header('X-Content-Type-Options: nosniff');

function answer(int $code, string $status): never
{
    http_response_code($code);
    echo json_encode(['status' => $status]);
    exit;
}

$config = @include __DIR__ . '/report-config.php';
if (!is_array($config) || empty($config['to'])) {
    answer(503, 'not_configured');
}
$bits = max(8, (int) ($config['bits'] ?? 20));
$data = $config['data'] ?? (__DIR__ . '/report-data');

if (($_SERVER['REQUEST_METHOD'] ?? '') !== 'POST') {
    header('Allow: POST');
    answer(405, 'post_only');
}
if (stripos($_SERVER['CONTENT_TYPE'] ?? '', 'application/json') !== 0) {
    answer(415, 'json_only');
}
$raw = file_get_contents('php://input', false, null, 0, 65537);
if ($raw === false || strlen($raw) > 65536) {
    answer(413, 'too_large');
}
$in = json_decode($raw, true);
if (!is_array($in)) {
    answer(400, 'bad_request');
}
$report = $in['report'] ?? null;
$note = $in['note'] ?? '';
$contact = $in['contact'] ?? '';
$time = $in['time'] ?? null;
$nonce = $in['nonce'] ?? null;
if (!is_string($report) || !is_string($note) || !is_string($contact) || !is_int($time) || !is_string($nonce)) {
    answer(400, 'bad_request');
}
if (!mb_check_encoding($report . $note . $contact, 'UTF-8')) {
    answer(400, 'bad_request');
}
if (strlen($report) > 32768 || mb_strlen($note) > 4000 || strlen($contact) > 200 || strlen($nonce) > 32) {
    answer(413, 'too_large');
}
if (!preg_match('/^GhulSuite \d+\.\d+\.\d+[^\n]*\n/', $report)) {
    answer(400, 'not_a_report');
}
if ($contact !== '' && !filter_var($contact, FILTER_VALIDATE_EMAIL)) {
    answer(400, 'bad_contact');
}
$now = time();
if (abs($now - $time) > 900) {
    answer(400, 'clock');
}

// Proof of work over exactly this content.
$content = hash('sha256', $report . "\0" . $note . "\0" . $contact);
$proof = hash('sha256', "ghulsuite-report|$time|$content|$nonce", true);
$zero = 0;
foreach (str_split($proof) as $byte) {
    $b = ord($byte);
    if ($b === 0) {
        $zero += 8;
        continue;
    }
    $zero += 8 - strlen(decbin($b));
    break;
}
if ($zero < $bits) {
    answer(400, 'proof');
}

// Counters: proofs used, reports per address and per day. One file, locked.
if (!is_dir($data) && !@mkdir($data, 0700, true)) {
    answer(503, 'storage');
}
$file = $data . '/.counters.json';
$fh = @fopen($file, 'c+');
if ($fh === false || !flock($fh, LOCK_EX)) {
    answer(503, 'storage');
}
$state = json_decode((string) stream_get_contents($fh), true);
if (!is_array($state) || ($state['day'] ?? '') !== gmdate('Y-m-d')) {
    // A new day: new salt, everything of yesterday is gone.
    $state = ['day' => gmdate('Y-m-d'), 'salt' => bin2hex(random_bytes(16)), 'total' => 0, 'ips' => [], 'used' => []];
}
$state['used'] = array_filter($state['used'], fn ($t) => $t > $now - 1800);
$proofHex = bin2hex($proof);
if (isset($state['used'][$proofHex])) {
    flock($fh, LOCK_UN);
    answer(409, 'already_sent');
}
$ip = hash('sha256', $state['salt'] . ($_SERVER['REMOTE_ADDR'] ?? ''));
$recent = array_values(array_filter($state['ips'][$ip] ?? [], fn ($t) => $t > $now - 3600));
if (count($recent) >= 5 || $state['total'] >= 100) {
    flock($fh, LOCK_UN);
    answer(429, 'busy');
}

// Mail: fixed subject and sender, no user input in the headers except a checked reply address.
$clean = fn (string $s) => str_replace("\r", '', $s);
$first = strtok($report, "\n");
$body = "Fehlerbericht aus der App\n"
    . "=========================\n\n"
    . "Notiz:\n" . ($note !== '' ? $clean($note) : '(keine)') . "\n\n"
    . "Kontakt für Rückfragen: " . ($contact !== '' ? $contact : '(keiner)') . "\n\n"
    . "Bericht:\n" . $clean($report) . "\n";
$headers = [
    'From' => 'GhulSuite <noreply@' . preg_replace('/^www\./', '', $_SERVER['HTTP_HOST'] ?? 'ghulsuite.de') . '>',
    'Content-Type' => 'text/plain; charset=utf-8',
    'Content-Transfer-Encoding' => '8bit',
    'X-Mailer' => 'GhulSuite report',
];
if ($contact !== '') {
    $headers['Reply-To'] = $contact;
}
$subject = '=?UTF-8?B?' . base64_encode('[GhulSuite] Fehlerbericht ' . substr((string) $first, 10, 40)) . '?=';
$sent = isset($config['outbox'])
    ? (bool) file_put_contents($config['outbox'] . '/' . $proofHex . '.txt', "To: {$config['to']}\nSubject: $subject\n\n$body")
    : mail((string) $config['to'], $subject, $body, $headers);
if (!$sent) {
    flock($fh, LOCK_UN);
    answer(503, 'mail');
}

$state['used'][$proofHex] = $now;
$recent[] = $now;
$state['ips'][$ip] = $recent;
$state['total']++;
ftruncate($fh, 0);
rewind($fh);
fwrite($fh, json_encode($state));
fflush($fh);
flock($fh, LOCK_UN);
fclose($fh);
answer(200, 'ok');
