#!/bin/sh
set -e
if [ -z "$PITH_E2E_DISPLAY" ] || [ "$DISPLAY" != "$PITH_E2E_DISPLAY" ]; then
  echo "refusing to run outside the display session.sh started" >&2
  exit 1
fi
openbox &
sh crates/linux/e2e/until.sh "window manager" sh -c 'xprop -root _NET_SUPPORTING_WM_CHECK | grep -q "window id"'
if [ "$PITH_E2E_COMPOSITOR" = "1" ]; then
  xcompmgr &
  sh crates/linux/e2e/until.sh "compositor" pgrep -x xcompmgr
fi
exec python3 "$@"
