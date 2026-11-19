import os
import subprocess
import sys

CI_FLAG = "--ci-runner"
SESSION_DISPLAY = "PITH_E2E_DISPLAY"


def require_ci():
    display = os.environ.get("DISPLAY")
    if (
        os.environ.get("CI") != "true"
        or CI_FLAG not in sys.argv
        or not display
        or display != os.environ.get(SESSION_DISPLAY)
    ):
        sys.exit(
            f"refusing to inject input: needs CI=true, {CI_FLAG} and the display "
            f"session.sh started ({SESSION_DISPLAY})"
        )


def xdotool(*args, delay_ms=None):
    require_ci()
    command = ["xdotool", *args]
    if delay_ms is not None:
        command[2:2] = ["--delay", str(delay_ms)]
    subprocess.run(command, check=True)


def type_text(text, delay_ms):
    xdotool("type", "--clearmodifiers", "--", text, delay_ms=delay_ms)


def press(*keys):
    xdotool("key", "--clearmodifiers", *keys)


def focus_window(window):
    xdotool("windowfocus", window)
