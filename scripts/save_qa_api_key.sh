#!/bin/zsh
set -euo pipefail

readonly key_directory="${NONOSUB_QA_SECRET_DIR:-$HOME/Library/Application Support/com.nono.nonosub}"
readonly key_file="${NONOSUB_QA_KEY_FILE:-$key_directory/qa-openai-api-key}"

mkdir -p "$key_directory"
chmod 700 "$key_directory"
umask 077

temporary_file="$(mktemp "$key_directory/.qa-openai-api-key.XXXXXX")"
cleanup() {
  rm -f "$temporary_file"
}
trap cleanup EXIT INT TERM

echo "macOS may ask once for permission to read NonoSub's existing Keychain item."
echo "The API key will be written directly to a private local file and will not be printed."
/usr/bin/security find-generic-password \
  -s "com.nono.nonosub" \
  -a "openai-api-key" \
  -w > "$temporary_file"

if [[ ! -s "$temporary_file" ]]; then
  echo "No API key was retrieved. NonoSub's Keychain item may not exist." >&2
  exit 1
fi

mv -f "$temporary_file" "$key_file"
chmod 600 "$key_file"
trap - EXIT INT TERM

echo "QA key prepared at: $key_file"
echo "Remove that file after unattended testing is finished."
