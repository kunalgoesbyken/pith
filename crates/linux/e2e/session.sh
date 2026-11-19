#!/bin/sh
set -e
unset DISPLAY WAYLAND_DISPLAY XDG_SESSION_TYPE
display_file=$(mktemp)
Xvfb -displayfd 3 -screen 0 1280x800x24 3>"$display_file" &
xvfb=$!
trap 'kill "$xvfb"; rm -f "$display_file"' EXIT
sh crates/linux/e2e/until.sh "Xvfb" test -s "$display_file"
DISPLAY=":$(head -n 1 "$display_file")"
export DISPLAY
export PITH_E2E_DISPLAY="$DISPLAY"
export GDK_BACKEND=x11
status=0
dbus-run-session -- sh crates/linux/e2e/desktop.sh "$@" || status=$?
exit "$status"
