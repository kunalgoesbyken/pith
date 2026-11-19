import argparse
import subprocess
import sys

import harness
import inject
from app import GHOST_SECONDS, ghost, screenshot, wait_for

SAMPLES = {
    "Latin": " thank you for your patience",
    "Devanagari": " नमस्ते दुनिया, आप कैसे हैं",
    "Arabic": " مرحبا بالعالم كيف حالك",
}
READY_LINE = "shown"
CARET_OFFSET = 20
CARET_TOP = 60
CARET_HEIGHT = 20
INK_THRESHOLD = 0.9
WINDOW_TITLE = "pith-gtk3-entry"


def darkest(window):
    crop = f"{window['width']}x{window['height']}+{window['x']}+{window['y']}"
    captured = subprocess.run(
        ["import", "-window", "root", "-crop", crop, "png:-"],
        capture_output=True,
        check=True,
    )
    measured = subprocess.run(
        ["convert", "png:-", "-colorspace", "Gray", "-format", "%[fx:minima]", "info:"],
        input=captured.stdout,
        capture_output=True,
        check=True,
    )
    return float(measured.stdout.decode())


def render(ghost_binary, label, text, field):
    caret = f"{field['x'] + CARET_OFFSET},{field['y'] + CARET_TOP},1,{CARET_HEIGHT}"
    process = subprocess.Popen(
        [ghost_binary, "inline", caret, text],
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        ready = process.stderr.readline().strip() == READY_LINE
        window = wait_for(ghost, GHOST_SECONDS) if ready else None
        screenshot(f"script-{label}")
        ink = darkest(window) if window else None
        return (
            "ghost renders " + label,
            window is not None and ink is not None and ink < INK_THRESHOLD,
            f"ghost {window} darkest pixel {ink}",
        )
    finally:
        process.kill()
        process.wait()


def main():
    inject.require_ci()
    parser = argparse.ArgumentParser()
    parser.add_argument(inject.CI_FLAG, action="store_true", required=True)
    parser.add_argument("--ghost", required=True)
    options = parser.parse_args()
    rows = []
    app = subprocess.Popen([sys.executable, harness.GTK_FIXTURE, "3.0", "entry"])
    try:
        field = harness.find_window(WINDOW_TITLE)
        for label, text in SAMPLES.items():
            rows.append(("Shaping", *render(options.ghost, label, text, field)))
    finally:
        app.kill()
        app.wait()
    harness.report(rows)
    sys.exit(0 if all(ok is not False for _, _, ok, _ in rows) else 1)


if __name__ == "__main__":
    main()
