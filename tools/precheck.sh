#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Runs every check of the CI that works on this computer, one after the other, and does NOT stop
# at the first failure: at the end a list shows what passed and what failed, with a log for each.
# Run it before every push, so the CI only has to find what really needs its machines (Windows,
# emulator, test servers).
#
#   tools/precheck.sh            all checks (Rust, texts, PHP module, workflows, Gradle builds)
#   tools/precheck.sh --quick    without the Gradle builds (a few minutes instead of ~20)
#   tools/precheck.sh --full     also the long release tests and the browser tests
#
# Logs: $PRECHECK_OUT (default: a new temporary folder, shown at the end).
# Gradle needs ANDROID_HOME. The signed build (with Android lint, as in CI pushes) uses a throwaway
# key made here and deleted afterwards; no real key is ever needed or read.
set -uo pipefail

mode="normal"
case "${1:-}" in
  --quick) mode="quick" ;;
  --full) mode="full" ;;
  "") ;;
  *) echo "Unbekannte Option: $1 (erlaubt: --quick, --full)"; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${PRECHECK_OUT:-$(mktemp -d -t ghul-precheck-XXXXXX)}"
mkdir -p "$out"
declare -a names=() results=() logs=()
start_all=$(date +%s)

# step <name> <command...>: runs the command, keeps its output, remembers the result.
step() {
  local name="$1"; shift
  local log="$out/$(printf '%02d' "${#names[@]}")-$(printf '%s' "$name" | tr -c 'A-Za-z0-9' '-' | cut -c1-40).log"
  local t0; t0=$(date +%s)
  printf '▶ %-55s' "$name"
  if (cd "$root" && "$@") > "$log" 2>&1; then
    results+=("ok"); printf ' ok (%ss)\n' "$(( $(date +%s) - t0 ))"
  else
    results+=("FEHLER"); printf ' FEHLER (%ss)\n' "$(( $(date +%s) - t0 ))"
  fi
  names+=("$name"); logs+=("$log")
}

# skip <name> <reason>
skip() {
  names+=("$1"); results+=("übersprungen: $2"); logs+=("-")
  printf '▶ %-55s übersprungen (%s)\n' "$1" "$2"
}

have() { command -v "$1" > /dev/null 2>&1; }

# --- texts, manifest, test vectors -------------------------------------------------------------
step "Texte (Übersetzungen, Platzhalter, Striche)" python3 tools/check-texts.py
step "Android-Manifest" python3 tools/check-android-manifest.py
# Like the CI: an own Python environment with the independent crypto libraries (made once, reused).
venv="${XDG_CACHE_HOME:-$HOME/.cache}/ghul-precheck-venv"
if [ ! -x "$venv/bin/python" ]; then
  python3 -m venv "$venv" > /dev/null 2>&1 && "$venv/bin/pip" install -q argon2-cffi cryptography pynacl cbor2 > /dev/null 2>&1 \
    || rm -rf "$venv"
fi
if [ -x "$venv/bin/python" ]; then
  step "Testvektoren unabhängig (Python)" "$venv/bin/python" tools/verify_testvectors.py
else
  skip "Testvektoren unabhängig (Python)" "Python-Umgebung ließ sich nicht anlegen"
fi

# --- workflows and scripts ---------------------------------------------------------------------
workflows() {
  python3 - <<'EOF'
import glob, sys, yaml
bad = 0
for f in sorted(glob.glob(".github/workflows/*.yml") + glob.glob(".github/actions/*/action.yml")):
    try:
        yaml.safe_load(open(f))
    except Exception as e:
        print(f"{f}: {e}"); bad += 1
sys.exit(1 if bad else 0)
EOF
}
if python3 -c "import yaml" 2> /dev/null; then step "Workflows lesbar (YAML)" workflows; else skip "Workflows lesbar (YAML)" "python3-yaml fehlt"; fi
scripts() { local f rc=0; for f in tools/*.sh; do bash -n "$f" || rc=1; done; return $rc; }
step "Shell-Skripte (Syntax)" scripts
if have shellcheck; then step "Shell-Skripte (shellcheck)" shellcheck -S warning tools/*.sh; fi

# --- Rust core -------------------------------------------------------------------------------------
cd_core() { (cd core && "$@"); }
step "Rust: Formatierung" cd_core cargo fmt --all -- --check
step "Rust: Clippy (Linux)" cd_core cargo clippy --locked -p ghul-core --all-targets --features mock -- -D warnings
if rustup target list --installed 2> /dev/null | grep -q x86_64-pc-windows-gnu; then
  step "Rust: Clippy (Windows)" cd_core cargo clippy --locked -p ghul-core --all-targets --target x86_64-pc-windows-gnu -- -D warnings
else
  skip "Rust: Clippy (Windows)" "rustup target add x86_64-pc-windows-gnu"
fi
step "Rust: alle Tests" cd_core cargo test --locked --no-fail-fast -p ghul-core
if [ "$mode" = "full" ]; then
  step "Rust: Alltags- und Stresstest (Release)" cd_core cargo test --locked --release -p ghul-core --lib everyday -- --include-ignored --test-threads=1
fi

# --- PHP module --------------------------------------------------------------------------------
if have php; then
  step "PHP-Modul (Syntax)" php -l core/ghul-core/server/ghulsuite.php
  step "PHP-Modul (Angriffe)" python3 tools/attack-php-module.py
else
  skip "PHP-Modul" "php fehlt"
fi
if [ "$mode" = "full" ]; then
  if have node && [ -n "${NODE_PATH:-}" ]; then
    browser() { (cd core && cargo test --locked -p ghul-core --no-run) && node tools/test-share-php.js && node tools/test-share-viewer.js; }
    step "Browser: Empfängerseite und PHP-Modul" browser
  else
    skip "Browser: Empfängerseite und PHP-Modul" "node und NODE_PATH mit playwright nötig"
  fi
fi

# --- Gradle: the same tasks as the CI, with the dependency checksums ------------------------------
if [ "$mode" = "quick" ]; then
  skip "Gradle (Desktop, Android, Paketierung)" "--quick"
elif [ -z "${ANDROID_HOME:-}" ]; then
  skip "Gradle (Desktop, Android, Paketierung)" "ANDROID_HOME fehlt"
else
  key="$out/throwaway.jks"
  rm -f "$key"
  keytool -genkeypair -keystore "$key" -storepass precheck -keypass precheck -alias ghulsuite \
    -keyalg RSA -keysize 2048 -validity 2 -dname "CN=precheck" -noprompt > /dev/null 2>&1
  gradle_all() {
    # Gradle builds the Rust core for Android in its own build folder (removed below), not in a shared
    # CARGO_TARGET_DIR: the Android builds are large.
    (cd app && env -u CARGO_TARGET_DIR GHUL_KEYSTORE_FILE="$key" GHUL_KEYSTORE_PASSWORD=precheck ./gradlew --console=plain --continue \
      :composeApp:desktopTest :composeApp:assembleDebug :composeApp:createDistributable)
  }
  step "Gradle: Desktop-Tests, signierte APK mit Lint, Paketierung" gradle_all
  rm -f "$key"
  # The Rust builds of the app are large; the disk of a cloud session is small.
  rm -rf app/composeApp/build/rust app/composeApp/build/intermediates app/composeApp/build/outputs
fi

# --- summary -----------------------------------------------------------------------------------
echo
echo "Ergebnis ($(( ($(date +%s) - start_all) / 60 )) min, Logs in $out):"
failed=0
for i in "${!names[@]}"; do
  case "${results[$i]}" in
    ok) printf '  ✔ %s\n' "${names[$i]}" ;;
    FEHLER) printf '  ✘ %s  →  %s\n' "${names[$i]}" "${logs[$i]}"; failed=$((failed + 1)) ;;
    *) printf '  - %s (%s)\n' "${names[$i]}" "${results[$i]}" ;;
  esac
done
echo
echo "Nur in der CI: Windows-Build und -Tests, Emulator, Netzwerk-Tests gegen echte Testserver."
if [ "$failed" -gt 0 ]; then
  echo "$failed Prüfung(en) fehlgeschlagen. Erst beheben, dann pushen."
  exit 1
fi
echo "Alles grün."
