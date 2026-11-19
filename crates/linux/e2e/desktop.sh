#!/bin/sh
set -e
START_ATTEMPTS=50
START_POLL=0.2
openbox &
attempts=0
until xprop -root _NET_SUPPORTING_WM_CHECK | grep -q "window id"; do
  attempts=$((attempts + 1))
  if [ "$attempts" -gt "$START_ATTEMPTS" ]; then
    echo "window manager did not start" >&2
    exit 1
  fi
  sleep "$START_POLL"
done
exec python3 crates/linux/e2e/harness.py --ci-runner --probe target/debug/examples/probe --target "$1"
