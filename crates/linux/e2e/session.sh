#!/bin/sh
set -e
openbox &
sleep 1
exec python3 crates/linux/e2e/harness.py --ci-runner --probe target/debug/examples/probe --target "$1"
