#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Starts local test servers for the network tests of ghul-core:
#   SFTP: OpenSSH on 127.0.0.1:2222 (key login, writes into a temporary folder)
#   IMAP: Dovecot on 127.0.0.1:10993 (TLS, self-signed) and 10143 (STARTTLS)
#   SMTP: a small test server on 127.0.0.1:10587 (STARTTLS) that delivers into the Dovecot mailbox
#   With GHUL_TEST_SFTP_PASSWORD=1 also SFTP with user name and password on 127.0.0.1:2223
#   (for the emulator flows; the emulator reaches all of them as 10.0.2.2)
# Prints the environment variables the tests read; use:  eval "$(tools/test-servers.sh)"
# Needs: openssh-server, dovecot-imapd, python3, openssl (Debian/Ubuntu packages). Runs as root
# (CI: sudo). Nothing here is used by the app itself.
set -euo pipefail
D=$(mktemp -d /tmp/ghul-servers.XXXXXX)
chmod 755 "$D"
HERE=$(cd "$(dirname "$0")" && pwd)

# --- SFTP ------------------------------------------------------------------------------------
mkdir -p "$D/ssh" "$D/sftp-root" /run/sshd
ssh-keygen -q -t ed25519 -N "" -f "$D/ssh/host_key"
touch "$D/ssh/authorized_keys"
chmod 600 "$D/ssh/authorized_keys"
# The tests (run without sudo) add their keys to this file.
if [ -n "${SUDO_USER:-}" ]; then chown "$SUDO_USER" "$D/ssh/authorized_keys"; fi
cat > "$D/ssh/sshd_config" <<CFG
Port 2222
ListenAddress 127.0.0.1
HostKey $D/ssh/host_key
PidFile $D/ssh/sshd.pid
AuthorizedKeysFile $D/ssh/authorized_keys
PermitRootLogin yes
PasswordAuthentication no
KbdInteractiveAuthentication no
StrictModes no
UsePAM no
Subsystem sftp internal-sftp
CFG
/usr/sbin/sshd -f "$D/ssh/sshd_config" -E "$D/ssh/sshd.log"
HOST_FP=$(ssh-keygen -lf "$D/ssh/host_key.pub" -E sha256 | awk '{print $2}')

# --- TLS certificate for IMAP/SMTP ------------------------------------------------------------
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
  -keyout "$D/tls.key" -out "$D/tls.crt" >/dev/null 2>&1
TLS_FP=$(openssl x509 -in "$D/tls.crt" -noout -fingerprint -sha256 | cut -d= -f2 | tr -d ':' | tr 'A-F' 'a-f')

# --- IMAP (Dovecot, one static test user) -------------------------------------------------------
if command -v dovecot >/dev/null; then
  # Dovecot never accesses mail as root: mail belongs to "nobody", the SMTP test server too.
  MAIL_UID=$(id -u nobody)
  MAIL_GID=$(id -g nobody)
  mkdir -p "$D/mail" "$D/dovecot"
  chown "$MAIL_UID:$MAIL_GID" "$D/mail"
  chmod 644 "$D/tls.key"
  cat > "$D/dovecot/dovecot.conf" <<CFG
base_dir = $D/dovecot/run
state_dir = $D/dovecot/state
log_path = $D/dovecot/dovecot.log
protocols = imap
listen = 127.0.0.1
ssl = yes
ssl_cert = <$D/tls.crt
ssl_key = <$D/tls.key
disable_plaintext_auth = yes
auth_mechanisms = plain login
mail_location = maildir:$D/mail/%u
passdb {
  driver = static
  args = password=test-pw-123
}
userdb {
  driver = static
  args = uid=$MAIL_UID gid=$MAIL_GID home=$D/mail/%u
}
service imap-login {
  inet_listener imap {
    address = 127.0.0.1
    port = 10143
  }
  inet_listener imaps {
    address = 127.0.0.1
    port = 10993
    ssl = yes
  }
}
namespace inbox {
  inbox = yes
  mailbox Sent {
    special_use = \\Sent
    auto = create
  }
  mailbox Trash {
    special_use = \\Trash
    auto = create
  }
  mailbox Drafts {
    special_use = \\Drafts
    auto = create
  }
}
CFG
  dovecot -c "$D/dovecot/dovecot.conf"
  # --- SMTP: delivers into the user's Maildir ---
  # Copy the server next to the mail data: "nobody" may not read the checkout (e.g. CI home).
  install -m 644 "$HERE/test-smtp.py" "$D/test-smtp.py"
  setpriv --reuid="$MAIL_UID" --regid="$MAIL_GID" --clear-groups \
    python3 "$D/test-smtp.py" 127.0.0.1 10587 "$D/tls.crt" "$D/tls.key" "$D/mail" >"$D/smtp.log" 2>&1 &
  echo $! > "$D/smtp.pid"
fi

# --- FTPS (vsftpd, explicit TLS, data channel encrypted with session reuse) ---------------------
if command -v vsftpd >/dev/null; then
  id ghulftp >/dev/null 2>&1 || useradd -M -s /usr/sbin/nologin ghulftp
  echo 'ghulftp:test-pw-123' | chpasswd
  grep -qx /usr/sbin/nologin /etc/shells || echo /usr/sbin/nologin >> /etc/shells
  mkdir -p "$D/ftp-home" "$D/vsftpd-empty"
  chown ghulftp "$D/ftp-home"
  usermod -d "$D/ftp-home" ghulftp
  cat > "$D/vsftpd.conf" <<CFG
listen=YES
listen_address=127.0.0.1
listen_port=10021
anonymous_enable=NO
local_enable=YES
write_enable=YES
local_umask=022
ssl_enable=YES
rsa_cert_file=$D/tls.crt
rsa_private_key_file=$D/tls.key
force_local_logins_ssl=YES
force_local_data_ssl=YES
require_ssl_reuse=YES
pasv_enable=YES
pasv_min_port=10090
pasv_max_port=10099
pasv_address=127.0.0.1
seccomp_sandbox=NO
secure_chroot_dir=$D/vsftpd-empty
pam_service_name=vsftpd
utf8_filesystem=YES
vsftpd_log_file=$D/vsftpd.log
CFG
  vsftpd "$D/vsftpd.conf" >/dev/null 2>&1 &
  echo $! > "$D/vsftpd.pid"
fi

# --- SFTP with a password (GHUL_TEST_SFTP_PASSWORD=1) ---------------------------------------------
# For the emulator flows: a person sets up GhulDrive with user name and password, and the app
# puts its own SSH key on the server. A second sshd, so the key-only server above stays as it is.
if [ "${GHUL_TEST_SFTP_PASSWORD:-0}" = 1 ]; then
  id ghulsftp >/dev/null 2>&1 || useradd -M -s /bin/sh ghulsftp
  echo 'ghulsftp:test-pw-123' | chpasswd
  mkdir -p "$D/sftp-pw"
  chown ghulsftp "$D/sftp-pw"
  usermod -d "$D/sftp-pw" ghulsftp
  cat > "$D/ssh/sshd_config_pw" <<CFG
Port 2223
ListenAddress 127.0.0.1
HostKey $D/ssh/host_key
PidFile $D/ssh/sshd_pw.pid
AllowUsers ghulsftp
PermitRootLogin no
PasswordAuthentication yes
KbdInteractiveAuthentication no
PubkeyAuthentication yes
StrictModes no
UsePAM no
Subsystem sftp internal-sftp
CFG
  /usr/sbin/sshd -f "$D/ssh/sshd_config_pw" -E "$D/ssh/sshd_pw.log"
fi

# Wait until every server listens; otherwise stop with its log (tests must not silently miss one).
PORTS="2222"
if [ "${GHUL_TEST_SFTP_PASSWORD:-0}" = 1 ]; then PORTS="$PORTS 2223"; fi
if command -v dovecot >/dev/null; then PORTS="$PORTS 10993 10143 10587"; fi
if command -v vsftpd >/dev/null; then PORTS="$PORTS 10021"; fi
for port in $PORTS; do
  for _ in $(seq 1 50); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then continue 2; fi
    sleep 0.2
  done
  echo "test server on port $port did not start" >&2
  cat "$D/smtp.log" "$D/dovecot/dovecot.log" "$D/ssh/sshd.log" 2>/dev/null >&2 || true
  exit 1
done
echo "export GHUL_TEST_DIR=$D"
echo "export GHUL_TEST_SFTP=127.0.0.1:2222"
echo "export GHUL_TEST_SFTP_USER=$(id -un)"
echo "export GHUL_TEST_SFTP_ROOT=$D/sftp-root"
echo "export GHUL_TEST_SFTP_AUTHKEYS=$D/ssh/authorized_keys"
echo "export GHUL_TEST_SFTP_HOSTKEY=$HOST_FP"
if [ "${GHUL_TEST_SFTP_PASSWORD:-0}" = 1 ]; then
  echo "export GHUL_TEST_SFTP_PW=127.0.0.1:2223"
  echo "export GHUL_TEST_SFTP_PW_USER=ghulsftp"
  echo "export GHUL_TEST_SFTP_PW_PASSWORD=test-pw-123"
  echo "export GHUL_TEST_SFTP_PW_ROOT=$D/sftp-pw"
fi
if command -v vsftpd >/dev/null; then
  echo "export GHUL_TEST_FTPS=127.0.0.1:10021"
  echo "export GHUL_TEST_FTPS_USER=ghulftp"
  echo "export GHUL_TEST_FTPS_PASSWORD=test-pw-123"
  echo "export GHUL_TEST_FTPS_TLSFP=$TLS_FP"
fi
if command -v dovecot >/dev/null; then
  echo "export GHUL_TEST_MAIL=127.0.0.1"
  echo "export GHUL_TEST_MAIL_TLSFP=$TLS_FP"
fi
