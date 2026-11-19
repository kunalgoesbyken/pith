#!/bin/sh
ATTEMPTS=50
POLL=0.2
description=$1
shift
attempts=0
until "$@" > /dev/null 2>&1; do
  attempts=$((attempts + 1))
  if [ "$attempts" -gt "$ATTEMPTS" ]; then
    echo "$description did not become ready" >&2
    exit 1
  fi
  sleep "$POLL"
done
