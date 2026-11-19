import argparse
import json
import subprocess
import sys
import threading
import time

import harness

SEED = "hi "
SUGGESTION = "ghost"
SCREENSHOT = "ime-preedit.png"
EVENT_SECONDS = 20.0


def wtype(text):
    subprocess.run(["wtype", text], check=True)


def wait_for_event(lines, name):
    deadline = time.monotonic() + EVENT_SECONDS
    while time.monotonic() < deadline:
        for line in list(lines):
            if line["event"] == name:
                return line
        time.sleep(harness.POLL_SECONDS)
    return None


def collect(process, lines):
    for raw in process.stdout:
        lines.append(json.loads(raw))


def main():
    harness.refuse_outside_ci()
    parser = argparse.ArgumentParser()
    parser.add_argument(harness.CI_FLAG, action="store_true", required=True)
    parser.add_argument("--probe", required=True)
    parser.add_argument("--spike", required=True)
    options = parser.parse_args()
    rows = []
    fixture = [sys.executable, str(harness.FIXTURES / "gtk4_field.py"), "entry"]
    with harness.running(fixture, options.probe) as probe:
        probe.wait(lambda s: s["before"] == "")
        wtype(SEED)
        probe.wait(lambda s: s["before"] == SEED)
        spike = subprocess.Popen([options.spike], stdout=subprocess.PIPE, text=True)
        lines = []
        threading.Thread(target=collect, args=(spike, lines), daemon=True).start()
        try:
            activated = wait_for_event(lines, "activated")
            rows.append(
                (
                    "sway input-method-v2",
                    "activation with surrounding text",
                    activated is not None and activated["detail"] is not None
                    and activated["detail"][0] == SEED,
                    json.dumps(activated),
                )
            )
            preedit = wait_for_event(lines, "preedit")
            if preedit is not None:
                subprocess.run(["grim", SCREENSHOT], check=False)
            rows.append(
                (
                    "sway input-method-v2",
                    "preedit accepted by the compositor",
                    preedit is not None and spike.poll() is None,
                    json.dumps(preedit),
                )
            )
            committed = wait_for_event(lines, "committed")
            after = probe.wait(lambda s: s["before"] == SEED + SUGGESTION)
            rows.append(
                (
                    "sway input-method-v2",
                    "committed text reaches the app",
                    committed is not None and after is not None,
                    harness.short(after if after is not None else probe.latest()),
                )
            )
        finally:
            spike.kill()
            spike.wait()
    harness.report(rows)
    sys.exit(0 if all(ok for _, _, ok, _ in rows) else 1)


if __name__ == "__main__":
    main()
