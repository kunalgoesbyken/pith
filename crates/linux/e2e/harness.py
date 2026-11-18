import argparse
import contextlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import threading
import time

CI_FLAG = "--ci-runner"
READY_SECONDS = 2.0
WAIT_SECONDS = 10.0
POLL_SECONDS = 0.1
WINDOW_SECONDS = 40
TYPE_DELAY_MS = "40"
WINDOW_MARGIN = 150
FIXTURES = pathlib.Path(__file__).parent / "fixtures"
FIREFOX_PREFERENCES = {
    "browser.shell.checkDefaultBrowser": False,
    "browser.aboutwelcome.enabled": False,
    "browser.startup.page": 0,
    "browser.tabs.warnOnClose": False,
    "datareporting.policy.dataSubmissionEnabled": False,
    "startup.homepage_welcome_url": "",
    "startup.homepage_welcome_url.additional": "",
    "toolkit.telemetry.reportingpolicy.firstRun": False,
}


def refuse_outside_ci():
    if os.environ.get("CI") != "true" or CI_FLAG not in sys.argv:
        sys.exit(f"refusing to inject input: needs CI=true and {CI_FLAG}")


def xdotool(*args):
    subprocess.run(["xdotool", *args], check=True)


def type_text(text):
    xdotool("type", "--clearmodifiers", "--delay", TYPE_DELAY_MS, "--", text)


def press(*keys):
    xdotool("key", "--clearmodifiers", *keys)


class Probe:
    def __init__(self, binary, *args):
        self.snapshots = []
        self.lock = threading.Lock()
        self.process = subprocess.Popen(
            [binary, "--allow-accessibility", *args],
            stdout=subprocess.PIPE,
            text=True,
        )
        threading.Thread(target=self.collect, daemon=True).start()
        time.sleep(READY_SECONDS)
        if self.process.poll() is not None:
            raise RuntimeError("probe exited early")

    def collect(self):
        for line in self.process.stdout:
            with self.lock:
                self.snapshots.append(json.loads(line))

    def latest(self):
        with self.lock:
            return self.snapshots[-1] if self.snapshots else None

    def wait(self, predicate):
        deadline = time.monotonic() + WAIT_SECONDS
        while time.monotonic() < deadline:
            snapshot = self.latest()
            if snapshot is not None and predicate(snapshot):
                return snapshot
            time.sleep(POLL_SECONDS)
        return None

    def stop(self):
        self.process.kill()
        self.process.wait()


@contextlib.contextmanager
def running(command, probe_binary, *probe_args):
    probe = Probe(probe_binary, *probe_args)
    app = subprocess.Popen(command)
    try:
        yield probe
    finally:
        app.kill()
        app.wait()
        probe.stop()


def find_window(title):
    found = subprocess.run(
        ["xdotool", "search", "--sync", "--onlyvisible", "--name", title],
        capture_output=True,
        text=True,
        check=True,
        timeout=WINDOW_SECONDS,
    )
    window = found.stdout.split()[0]
    geometry = subprocess.run(
        ["xdotool", "getwindowgeometry", "--shell", window],
        capture_output=True,
        text=True,
        check=True,
    )
    values = dict(line.split("=") for line in geometry.stdout.split())
    xdotool("windowfocus", window)
    return {key: int(values[key.upper()]) for key in ("x", "y", "width", "height")}


def inside(window, snapshot):
    placement = snapshot["placement"]
    if placement is None or placement["kind"] != "inline" or placement["height"] <= 0:
        return False
    return (
        window["x"] - WINDOW_MARGIN <= placement["x"] <= window["x"] + window["width"] + WINDOW_MARGIN
        and window["y"] - WINDOW_MARGIN <= placement["y"] <= window["y"] + window["height"] + WINDOW_MARGIN
    )


def short(snapshot):
    if snapshot is None:
        return "no snapshot"
    placement = snapshot["placement"]
    where = "no caret" if placement is None else f"caret {placement['x']},{placement['y']}"
    return (
        f"app={snapshot['app']!r} before={snapshot['before']!r} "
        f"after={snapshot['after']!r} {where} "
        f"selection={snapshot['has_selection']} password={snapshot['is_password']}"
    )


class Table:
    def __init__(self, app, probe):
        self.app = app
        self.probe = probe
        self.rows = []

    def step(self, name, action, predicate):
        action()
        snapshot = self.probe.wait(predicate)
        detail = short(snapshot if snapshot is not None else self.probe.latest())
        self.rows.append((self.app, name, snapshot is not None, detail))
        return snapshot

    def reset(self):
        press("ctrl+a")
        press("BackSpace")
        self.probe.wait(lambda s: s["before"] == "" and s["after"] == "")


def text_field(app, probe, window, multiline):
    table = Table(app, probe)
    table.reset()
    typed = table.step(
        "typed text",
        lambda: type_text("Hello world"),
        lambda s: s["before"] == "Hello world" and s["after"] == "",
    )
    if typed is not None:
        table.rows.append((app, "caret rect near the window", inside(window, typed), short(typed)))
        start = typed["placement"]["x"] if typed["placement"] else 0
        table.step(
            "caret follows typing",
            lambda: type_text(" again"),
            lambda s: s["before"] == "Hello world again"
            and s["placement"] is not None
            and s["placement"]["x"] > start,
        )
    table.reset()
    type_text("abc def")
    table.step(
        "caret in the middle",
        lambda: press("Left", "Left", "Left"),
        lambda s: s["before"] == "abc " and s["after"] == "def",
    )
    table.step(
        "selection",
        lambda: press("shift+Right", "shift+Right"),
        lambda s: s["has_selection"] and s["before"] == "abc ",
    )
    table.reset()
    type_text("héllo wörld 日本語")
    table.step(
        "unicode before the caret",
        lambda: None,
        lambda s: s["before"] == "héllo wörld 日本語",
    )
    table.step(
        "unicode around the caret",
        lambda: press("Left", "Left"),
        lambda s: s["before"] == "héllo wörld 日" and s["after"] == "本語",
    )
    table.reset()
    type_text("a😀b")
    table.step(
        "astral character before the caret",
        lambda: None,
        lambda s: s["before"] == "a😀b",
    )
    table.step(
        "astral character around the caret",
        lambda: press("Left"),
        lambda s: s["before"] == "a😀" and s["after"] == "b",
    )
    if multiline:
        table.reset()
        type_text("line one")
        first = probe.wait(lambda s: s["before"] == "line one")
        first_y = first["placement"]["y"] if first and first["placement"] else 0
        press("Return")
        type_text("line two")
        table.step(
            "second line",
            lambda: None,
            lambda s: s["before"] == "line one\nline two"
            and s["placement"] is not None
            and s["placement"]["y"] > first_y,
        )
    return table.rows


def password_field(app, probe):
    table = Table(app, probe)
    table.step(
        "password is flagged and hidden",
        lambda: type_text("secret"),
        lambda s: s["is_password"] and s["before"] == "" and s["after"] == "",
    )
    return table.rows


def blocked_field(app, probe):
    table = Table(app, probe)
    table.step(
        "blocked app text is hidden",
        lambda: type_text("private"),
        lambda s: s["app"] == app and s["before"] == "" and s["after"] == "",
    )
    return table.rows


def gtk4(probe_binary):
    rows = []
    for kind in ("entry", "textview"):
        with running(
            [sys.executable, str(FIXTURES / "gtk4_field.py"), kind], probe_binary
        ) as probe:
            window = find_window(f"pith-gtk4-{kind}")
            rows += text_field(f"gtk4 {kind}", probe, window, kind == "textview")
    with running(
        [sys.executable, str(FIXTURES / "gtk4_field.py"), "password"], probe_binary
    ) as probe:
        find_window("pith-gtk4-password")
        rows += password_field("gtk4 password", probe)
    return rows


def zenity(probe_binary):
    rows = []
    entry = ["zenity", "--entry", "--title=pith-zenity-entry"]
    text = ["zenity", "--text-info", "--editable", "--filename=/dev/null", "--title=pith-zenity-text"]
    with running(entry, probe_binary) as probe:
        window = find_window("pith-zenity-entry")
        rows += text_field("gtk3 entry", probe, window, False)
    with running(text, probe_binary) as probe:
        window = find_window("pith-zenity-text")
        rows += text_field("gtk3 textview", probe, window, True)
    with running(entry, probe_binary, "--block", "zenity") as probe:
        find_window("pith-zenity-entry")
        rows += blocked_field("zenity", probe)
    return rows


def firefox(probe_binary):
    profile = tempfile.mkdtemp(prefix="pith-firefox-")
    lines = [
        f"user_pref({json.dumps(key)}, {json.dumps(value)});"
        for key, value in FIREFOX_PREFERENCES.items()
    ]
    pathlib.Path(profile, "user.js").write_text("\n".join(lines))
    command = [
        shutil.which("firefox"),
        "--no-remote",
        "--profile",
        profile,
        "--new-window",
        (FIXTURES / "textarea.html").as_uri(),
    ]
    with running(command, probe_binary) as probe:
        window = find_window("pith-firefox-textarea")
        return text_field("firefox textarea", probe, window, True)


TARGETS = {"gtk4": gtk4, "zenity": zenity, "firefox": firefox}


def report(rows):
    lines = ["| App | Scenario | Result | Observed |", "|---|---|---|---|"]
    for app, name, ok, detail in rows:
        lines.append(f"| {app} | {name} | {'pass' if ok else 'FAIL'} | `{detail}` |")
    table = "\n".join(lines)
    print(table)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(table + "\n\n")


def main():
    refuse_outside_ci()
    parser = argparse.ArgumentParser()
    parser.add_argument(CI_FLAG, action="store_true", required=True)
    parser.add_argument("--probe", required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    options = parser.parse_args()
    rows = TARGETS[options.target](options.probe)
    report(rows)
    sys.exit(0 if all(ok for _, _, ok, _ in rows) else 1)


if __name__ == "__main__":
    main()
