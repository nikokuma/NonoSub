#!/bin/zsh
set -euo pipefail

readonly repository_root="${0:A:h:h}"
readonly key_file="${NONOSUB_QA_KEY_FILE:-$HOME/Library/Application Support/com.nono.nonosub/qa-openai-api-key}"

if [[ ! -f "$key_file" ]]; then
  echo "The unattended QA key is missing." >&2
  echo "While Nico is present, run: scripts/save_qa_api_key.sh" >&2
  exit 1
fi

permissions="$(stat -f '%Lp' "$key_file")"
if [[ "$permissions" != "600" && "$permissions" != "400" ]]; then
  echo "Refusing to read the QA key because its permissions are $permissions, not 600 or 400." >&2
  exit 1
fi

OPENAI_API_KEY="$(<"$key_file")"
if [[ -z "${OPENAI_API_KEY//[[:space:]]/}" ]]; then
  echo "The unattended QA key file is empty." >&2
  unset OPENAI_API_KEY
  exit 1
fi
export OPENAI_API_KEY

cd "$repository_root"
if (( $# == 0 )); then
  exec pnpm tauri dev
fi
exec "$@"
