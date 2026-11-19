#!/bin/sh
set -e
unset DISPLAY WAYLAND_DISPLAY
START_ATTEMPTS=50
START_POLL=0.2
display_file=$(mktemp)
Xvfb -displayfd 3 -screen 0 1280x800x24 3>"$display_file" &
xvfb=$!
trap 'kill "$xvfb"' EXIT
attempts=0
until [ -s "$display_file" ]; do
  attempts=$((attempts + 1))
  if [ "$attempts" -gt "$START_ATTEMPTS" ]; then
    echo "Xvfb did not start" >&2
    exit 1
  fi
  sleep "$START_POLL"
done
DISPLAY=":$(head -n 1 "$display_file")"
export DISPLAY
status=0
dbus-run-session -- sh crates/linux/e2e/desktop.sh "$1" || status=$?
exit "$status"
