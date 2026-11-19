#!/bin/sh
set -e
XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
export XDG_RUNTIME_DIR
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
export GDK_BACKEND=wayland GSK_RENDERER=cairo
unset DISPLAY
: > "$XDG_RUNTIME_DIR/sway.conf"
sway -c "$XDG_RUNTIME_DIR/sway.conf" &
until WAYLAND_DISPLAY=$(ls "$XDG_RUNTIME_DIR" | grep -m1 '^wayland-[0-9]*$'); do sleep 0.2; done
export WAYLAND_DISPLAY
exec python3 crates/linux/e2e/ime.py --ci-runner --probe target/debug/examples/probe --spike target/debug/examples/ime_spike
