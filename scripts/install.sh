#!/usr/bin/env bash
# Builds Buffer.app and installs (or updates) it in ~/Applications, then relaunches it.
# The Dock pin points at the installed path, so it survives updates. Task data is untouched.
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="Buffer"
BIN="buffer"

./scripts/bundle.sh

# Fixed location: writable without admin, so every shell installs to the same place.
# Two copies with the same bundle id show up as two Dock icons.
dest_dir="$HOME/Applications"
mkdir -p "$dest_dir"
dest="$dest_dir/$APP_NAME.app"
stale="/Applications/$APP_NAME.app"
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

if pgrep -x "$BIN" >/dev/null; then
  echo "quitting running ${APP_NAME}…"
  osascript -e "tell application \"$APP_NAME\" to quit" >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do pgrep -x "$BIN" >/dev/null || break; sleep 0.1; done
  pkill -x "$BIN" 2>/dev/null || true
fi

# Before the rename the app was Workdesk: quit and remove that copy so only one icon remains.
# Its data folder is moved by the app itself on first launch (storage::data_dir).
if pgrep -x workdesk >/dev/null; then
  osascript -e 'tell application "Workdesk" to quit' >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do pgrep -x workdesk >/dev/null || break; sleep 0.1; done
  pkill -x workdesk 2>/dev/null || true
fi
for old in "$dest_dir/Workdesk.app" "/Applications/Workdesk.app"; do
  if [ -d "$old" ]; then
    "$lsregister" -u "$old" 2>/dev/null || true
    rm -rf "$old" 2>/dev/null && echo "removed old $old" || echo "warning: could not remove $old" >&2
  fi
done

if [ -d "$stale" ]; then
  "$lsregister" -u "$stale" 2>/dev/null || true
  if rm -rf "$stale" 2>/dev/null; then
    echo "removed old copy $stale"
  else
    echo "warning: could not remove $stale; delete it manually to avoid a duplicate Dock icon" >&2
  fi
fi

rm -rf "$dest"
ditto "target/release/$APP_NAME.app" "$dest"
# Refresh Launch Services so Finder/Dock pick up the new icon and version.
"$lsregister" -f "$dest" || true

echo "installed $dest"
open "$dest"
