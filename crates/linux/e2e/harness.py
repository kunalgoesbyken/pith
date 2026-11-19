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
import urllib.parse

import inject

READY_LINE = "ready"
READY_SECONDS = 30.0
WAIT_SECONDS = 10.0
POLL_SECONDS = 0.1
WINDOW_SECONDS = 40
TYPE_DELAY_MS = 40
WINDOW_MARGIN = 150
INLINE = "Inline"
FIXTURES = pathlib.Path(__file__).parent / "fixtures"
GTK_FIXTURE = str(FIXTURES / "gtk_field.py")
HOSTED_APP = "gtk_field"
HOSTED_TERMINAL = "xterm"
SEED = "a😀 héllo wörld 日本語"
SEED_ASTRAL_HEAD = SEED[:2]
SEED_ASTRAL_REST = SEED[2:]
SEED_CJK_TAIL = SEED[-2:]
SEED_BEFORE_CJK_TAIL = SEED[:-2]
GTK_VERSIONS = {"gtk3": "3.0", "gtk4": "4.0"}
GTK_CARET_RECTS = {"gtk3": True, "gtk4": False}
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


class Probe:
    def __init__(self, binary, *args):
        self.snapshots = []
        self.lock = threading.Lock()
        self.ready = threading.Event()
        self.process = subprocess.Popen(
            [binary, "--allow-accessibility", *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        threading.Thread(target=self.collect, daemon=True).start()
        threading.Thread(target=self.watch_errors, daemon=True).start()
        deadline = time.monotonic() + READY_SECONDS
        while not self.ready.wait(POLL_SECONDS):
            if self.process.poll() is not None or time.monotonic() > deadline:
                self.stop()
                raise RuntimeError("probe did not become ready")

    def collect(self):
        for line in self.process.stdout:
            with self.lock:
                self.snapshots.append(json.loads(line))

    def watch_errors(self):
        for line in self.process.stderr:
            if line.strip() == READY_LINE:
                self.ready.set()
            else:
                sys.stderr.write(line)

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


def type_text(text):
    delay = TYPE_DELAY_MS if text.isascii() else TYPE_DELAY_MS * 6
    inject.type_text(text, delay)


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
    inject.focus_window(window)
    return {key: int(values[key.upper()]) for key in ("x", "y", "width", "height")}


def caret(snapshot):
    placement = snapshot["placement"]
    if placement is None:
        return None
    ((kind, rect),) = placement.items()
    return rect if kind == INLINE else None


def inside(window, snapshot):
    rect = caret(snapshot)
    if rect is None or rect["height"] <= 0:
        return False
    return (
        window["x"] - WINDOW_MARGIN
        <= rect["x"]
        <= window["x"] + window["width"] + WINDOW_MARGIN
        and window["y"] - WINDOW_MARGIN
        <= rect["y"]
        <= window["y"] + window["height"] + WINDOW_MARGIN
    )


def short(snapshot):
    if snapshot is None:
        return "no snapshot"
    rect = caret(snapshot)
    where = "no caret" if rect is None else f"caret {rect['x']},{rect['y']}"
    return (
        f"app={snapshot['app']!r} before={snapshot['before']!r} "
        f"after={snapshot['after']!r} {where} "
        f"selection={snapshot['has_selection']} masked={snapshot['is_password']}"
    )


class Table:
    def __init__(self, app, probe):
        self.app = app
        self.probe = probe
        self.rows = []

    def record(self, name, ok, detail):
        self.rows.append((self.app, name, ok, detail))

    def step(self, name, action, predicate):
        action()
        snapshot = self.probe.wait(predicate)
        observed = snapshot if snapshot is not None else self.probe.latest()
        self.record(name, snapshot is not None, short(observed))
        return snapshot

    def reset(self):
        inject.press("ctrl+a")
        inject.press("BackSpace")
        self.probe.wait(lambda s: s["before"] == "" and s["after"] == "")


def moved_right(start):
    def check(snapshot):
        rect = caret(snapshot)
        return rect is not None and rect["x"] > start

    return check


def moved_down(start):
    def check(snapshot):
        rect = caret(snapshot)
        return rect is not None and rect["y"] > start

    return check


def typing_rows(app, probe, window, multiline, caret_rect):
    table = Table(app, probe)
    table.reset()
    typed = table.step(
        "typed text",
        lambda: type_text("Hello world"),
        lambda s: s["before"] == "Hello world" and s["after"] == "",
    )
    if caret_rect:
        if typed is not None:
            table.record(
                "caret rect near the window", inside(window, typed), short(typed)
            )
            start = caret(typed)["x"] if caret(typed) else 0
            table.step(
                "caret follows typing",
                lambda: type_text(" again"),
                lambda s: s["before"] == "Hello world again" and moved_right(start)(s),
            )
    else:
        table.record(
            "caret rect",
            None,
            "known gap: toolkit reports caret extents as unsupported",
        )
    table.reset()
    type_text("abc def")
    table.step(
        "caret in the middle",
        lambda: inject.press("Left", "Left", "Left"),
        lambda s: s["before"] == "abc " and s["after"] == "def",
    )
    table.step(
        "selection",
        lambda: inject.press("shift+Right", "shift+Right"),
        lambda s: s["has_selection"] and s["before"] == "abc de",
    )
    if multiline:
        table.reset()
        type_text("line one")
        first = probe.wait(lambda s: s["before"] == "line one")
        first_rect = caret(first) if first else None
        first_y = first_rect["y"] if first_rect else 0
        inject.press("Return")
        type_text("line two")
        table.step(
            "second line" if caret_rect else "second line text",
            lambda: None,
            lambda s: s["before"] == "line one\nline two"
            and (not caret_rect or moved_down(first_y)(s)),
        )
    return table.rows


def unicode_rows(app, probe, caret_rect):
    table = Table(app, probe)

    def placed(snapshot):
        return not caret_rect or caret(snapshot) is not None

    table.step(
        "unicode text before the caret",
        lambda: inject.press("End"),
        lambda s: s["before"] == SEED and s["after"] == "" and placed(s),
    )
    table.step(
        "unicode around the caret",
        lambda: inject.press("Left", "Left"),
        lambda s: s["before"] == SEED_BEFORE_CJK_TAIL
        and s["after"] == SEED_CJK_TAIL
        and placed(s),
    )
    table.step(
        "astral character boundary",
        lambda: inject.press("Home", "Right", "Right"),
        lambda s: s["before"] == SEED_ASTRAL_HEAD
        and s["after"] == SEED_ASTRAL_REST
        and placed(s),
    )
    return table.rows


def password_rows(app, probe):
    table = Table(app, probe)
    table.step(
        "password is flagged and hidden",
        lambda: type_text("secret"),
        lambda s: s["is_password"] and s["before"] == "" and s["after"] == "",
    )
    return table.rows


def nearby_rows(app, probe):
    table = Table(app, probe)
    table.step(
        "the text beside the box is read",
        lambda: type_text("hello"),
        lambda s: s["before"] == "hello"
        and any("Quarterly numbers" in line for line in s["nearby"]),
    )
    table.step(
        "the password box never reaches the cues",
        lambda: None,
        lambda s: not any("hunter2" in line for line in s["nearby"]),
    )
    return table.rows


def blocked_rows(app, executable, probe):
    table = Table(app, probe)
    table.step(
        "blocked app text is hidden",
        lambda: type_text("private"),
        lambda s: s["app"] == executable and s["before"] == "" and s["after"] == "",
    )
    return table.rows


def hosted_terminal_rows(app, probe):
    table = Table(app, probe)
    table.step(
        "interpreter-hosted terminal is hidden",
        lambda: type_text("private"),
        lambda s: s["app"] == HOSTED_TERMINAL and s["before"] == "" and s["after"] == "",
    )
    return table.rows


def gtk(target, probe_binary):
    version = GTK_VERSIONS[target]
    caret_rect = GTK_CARET_RECTS[target]
    label = f"GTK {version[0]}"
    rows = []

    def fixture(kind, *seed):
        return [sys.executable, GTK_FIXTURE, version, kind, *seed]

    def window_title(kind):
        return f"pith-gtk{version[0]}-{kind}"

    for kind in ("entry", "textview"):
        with running(fixture(kind), probe_binary) as probe:
            window = find_window(window_title(kind))
            rows += typing_rows(
                f"{label} {kind}", probe, window, kind == "textview", caret_rect
            )
        with running(fixture(kind, SEED), probe_binary) as probe:
            find_window(window_title(kind))
            rows += unicode_rows(f"{label} {kind}", probe, caret_rect)
    with running(fixture("password"), probe_binary) as probe:
        find_window(window_title("password"))
        rows += password_rows(f"{label} password", probe)
    with running(fixture("form"), probe_binary, "--nearby") as probe:
        find_window(window_title("form"))
        rows += nearby_rows(f"{label} form", probe)
    with running(
        fixture("entry"), probe_binary, "--block", HOSTED_APP
    ) as probe:
        find_window(window_title("entry"))
        rows += blocked_rows(f"{label} entry", HOSTED_APP, probe)
    with tempfile.TemporaryDirectory(prefix="pith-terminal-") as directory:
        script = pathlib.Path(directory, HOSTED_TERMINAL)
        shutil.copy(GTK_FIXTURE, script)
        command = [sys.executable, str(script), version, "entry"]
        with running(command, probe_binary) as probe:
            find_window(window_title("entry"))
            rows += hosted_terminal_rows(f"{label} entry", probe)
    return rows


def firefox_command(profile, page, seed=""):
    url = (FIXTURES / page).as_uri()
    if seed:
        url += "#" + urllib.parse.quote(seed)
    return [
        shutil.which("firefox"),
        "--no-remote",
        "--profile",
        profile,
        "--new-window",
        url,
    ]


def firefox_profile():
    profile = tempfile.mkdtemp(prefix="pith-firefox-")
    lines = [
        f"user_pref({json.dumps(key)}, {json.dumps(value)});"
        for key, value in FIREFOX_PREFERENCES.items()
    ]
    pathlib.Path(profile, "user.js").write_text("\n".join(lines))
    return profile


def firefox(probe_binary):
    rows = []
    with running(
        firefox_command(firefox_profile(), "textarea.html"), probe_binary
    ) as probe:
        window = find_window("pith-firefox-textarea")
        rows += typing_rows("Firefox textarea", probe, window, True, True)
    with running(
        firefox_command(firefox_profile(), "textarea.html", SEED), probe_binary
    ) as probe:
        find_window("pith-firefox-textarea")
        rows += unicode_rows("Firefox textarea", probe, True)
    with running(
        firefox_command(firefox_profile(), "password.html"), probe_binary
    ) as probe:
        find_window("pith-firefox-password")
        rows += password_rows("Firefox password", probe)
    return rows


TARGETS = {
    "gtk3": lambda probe: gtk("gtk3", probe),
    "gtk4": lambda probe: gtk("gtk4", probe),
    "firefox": firefox,
}


def verdict(ok):
    return {True: "pass", False: "FAIL", None: "known gap"}[ok]


def report(rows):
    lines = ["| App | Scenario | Result | Observed |", "|---|---|---|---|"]
    for app, name, ok, detail in rows:
        lines.append(f"| {app} | {name} | {verdict(ok)} | `{detail}` |")
    table = "\n".join(lines)
    print(table)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(table + "\n\n")


def main():
    inject.require_ci()
    parser = argparse.ArgumentParser()
    parser.add_argument(inject.CI_FLAG, action="store_true", required=True)
    parser.add_argument("--probe", required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    options = parser.parse_args()
    rows = TARGETS[options.target](options.probe)
    report(rows)
    sys.exit(0 if all(ok is not False for _, _, ok, _ in rows) else 1)


if __name__ == "__main__":
    main()
