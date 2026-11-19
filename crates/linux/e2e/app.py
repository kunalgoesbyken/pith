import argparse
import collections
import contextlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

import harness
import inject

PROMPT = (
    "Hi Sarah, thanks for sending the revised contract. "
    "I reviewed the payment terms and they look"
)
FOLLOW_UP = " Please let me know if you have any"
TAB = "\t"
LETTER = "x"
GHOST_NAME = "PithGhostText"
READY_MARKER = "ready ("
SHOW = "show"
INSERT = "insert"
MODEL_SECONDS = 600.0
GHOST_SECONDS = 40.0
GONE_SECONDS = 4.0
RACE_SETTLE_SECONDS = 0.6
QUIET_SECONDS = 2.0
POLL_SECONDS = 0.1
GHOST_REACH = 40
PILL_REACH = 60
BASE_CONFIG = 'model = "small"\n'
RACE_CONFIG = BASE_CONFIG + "accept_race_ms = 1000\n"
SHOTS_ENV = "PITH_E2E_SHOTS"
DATA_ENV = "PITH_E2E_DATA"

Fixture = collections.namedtuple("Fixture", "title command tab_reaches_app")
Case = collections.namedtuple("Case", "label fixture insert config pill race")

FIXTURES = {
    "textview": Fixture(
        "pith-gtk3-textview",
        lambda: [sys.executable, harness.GTK_FIXTURE, "3.0", "textview"],
        True,
    ),
    "entry": Fixture(
        "pith-gtk3-entry",
        lambda: [sys.executable, harness.GTK_FIXTURE, "3.0", "entry"],
        False,
    ),
    "firefox": Fixture(
        "pith-firefox-textarea",
        lambda: harness.firefox_command(harness.firefox_profile(), "textarea.html"),
        False,
    ),
}

WITH_COMPOSITOR = [
    Case("GTK 3 textview, accessibility insert", "textview", "accessibility", BASE_CONFIG, False, False),
    Case("GTK 3 textview, clipboard insert", "textview", "clipboard", BASE_CONFIG, False, False),
    Case("GTK 3 entry, accessibility insert", "entry", "accessibility", BASE_CONFIG, False, False),
    Case("GTK 3 textview, accept race", "textview", "accessibility", RACE_CONFIG, False, True),
    Case("Firefox textarea, automatic insert", "firefox", "auto", BASE_CONFIG, False, False),
    Case("Firefox textarea, clipboard insert", "firefox", "clipboard", BASE_CONFIG, False, False),
]

WITHOUT_COMPOSITOR = [
    Case("GTK 3 textview, no compositor", "textview", "accessibility", BASE_CONFIG, True, False),
]


def wait_for(produce, seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = produce()
        if value:
            return value
        time.sleep(POLL_SECONDS)
    return None


def ghost():
    found = subprocess.run(
        ["xdotool", "search", "--onlyvisible", "--name", GHOST_NAME],
        capture_output=True,
        text=True,
    ).stdout.split()
    if not found:
        return None
    geometry = subprocess.run(
        ["xdotool", "getwindowgeometry", "--shell", found[0]],
        capture_output=True,
        text=True,
    ).stdout.split()
    values = dict(line.split("=") for line in geometry)
    return {key: int(values[key.upper()]) for key in ("x", "y", "width", "height")}


def hidden():
    return ghost() is None


def screenshot(name):
    directory = os.environ.get(SHOTS_ENV)
    if directory:
        pathlib.Path(directory).mkdir(parents=True, exist_ok=True)
        subprocess.run(["import", "-window", "root", f"{directory}/{name}.png"])


class Pith:
    def __init__(self, binary, insert, config, workdir):
        data = os.environ.get(DATA_ENV) or str(workdir / "data")
        configs = workdir / "config"
        (configs / "pith").mkdir(parents=True)
        (configs / "pith" / "config.toml").write_text(config)
        self.record = workdir / "record.jsonl"
        self.output = workdir / "pith.log"
        self.log = self.output.open("w")
        self.process = subprocess.Popen(
            [
                binary,
                "--allow-accessibility",
                "--insert",
                insert,
                "--record",
                str(self.record),
            ],
            env=dict(os.environ, XDG_CONFIG_HOME=str(configs), XDG_DATA_HOME=data),
            stderr=self.log,
        )

    def ready(self):
        return READY_MARKER in self.output.read_text()

    def events(self, kind):
        if not self.record.exists():
            return []
        lines = self.record.read_text().splitlines()
        return [event for event in map(json.loads, lines) if event["event"] == kind]

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait()
            self.log.close()
        return self.process.returncode


@contextlib.contextmanager
def pith(binary, insert, config):
    with tempfile.TemporaryDirectory(prefix="pith-e2e-") as workdir:
        session = Pith(binary, insert, config, pathlib.Path(workdir))
        try:
            if not wait_for(session.ready, MODEL_SECONDS):
                sys.stderr.write(session.output.read_text())
                raise RuntimeError("pith did not become ready")
            yield session
        finally:
            session.stop()


def near(window, caret, pill):
    dx, dy = window["x"] - caret["x"], window["y"] - caret["y"]
    if pill:
        return 0 <= dy - caret["height"] + 2 <= PILL_REACH and abs(dx) <= GHOST_REACH
    return -2 <= dx <= GHOST_REACH and abs(dy) <= caret["height"]


def suggests(table, probe, name, pill):
    table.reset()
    harness.type_text(PROMPT)
    probe.wait(lambda snapshot: snapshot["before"] == PROMPT)
    window = wait_for(ghost, GHOST_SECONDS)
    caret = harness.caret(probe.latest()) if probe.latest() else None
    table.record("ghost appears", window is not None, f"ghost {window} caret {caret}")
    if window is None:
        screenshot(f"{name}-no-ghost")
        return False
    table.record(
        "pill below the caret" if pill else "ghost at the caret",
        caret is not None and near(window, caret, pill),
        f"ghost {window} caret {caret}",
    )
    screenshot(name)
    return True


def tab_with_letter():
    inject.xdotool("keydown", "Tab")
    inject.xdotool("key", LETTER)
    inject.xdotool("keyup", "Tab")


def accepts(table, probe, session, letter):
    shown = session.events(SHOW)[-1]["text"][0]
    inserts = len(session.events(INSERT))
    time.sleep(RACE_SETTLE_SECONDS)
    if letter:
        tab_with_letter()
    else:
        inject.press("Tab")
    inserted = wait_for(lambda: session.events(INSERT)[inserts:], GONE_SECONDS)
    text = inserted[0]["text"] if inserted else None
    expected = PROMPT + (text or "") + (LETTER if letter else "")
    field = probe.wait(lambda snapshot: snapshot["before"] == expected)
    table.record(
        "Tab inserts the shown suggestion",
        text == shown and field is not None,
        f"shown {shown!r} inserted {text!r} field {harness.short(probe.latest())}",
    )
    return field is not None


def dismisses(table, probe, session):
    shows = len(session.events(SHOW))
    harness.type_text(FOLLOW_UP)
    visible = wait_for(
        lambda: len(session.events(SHOW)) > shows and ghost(), GHOST_SECONDS
    )
    before = probe.latest()["before"]
    inserts = len(session.events(INSERT))
    inject.press("Escape")
    gone = wait_for(hidden, GONE_SECONDS)
    time.sleep(QUIET_SECONDS)
    outcome = {
        "ghost before Esc": bool(visible),
        "hidden after Esc": bool(gone),
        "still hidden": hidden(),
        "nothing inserted": len(session.events(INSERT)) == inserts,
        "text unchanged": probe.latest()["before"] == before,
    }
    table.record(
        "Esc hides the ghost and keeps the text",
        all(outcome.values()),
        f"{outcome}",
    )


def passes_through(table, probe):
    start = probe.latest()["before"]
    inject.press("Tab")
    moved = probe.wait(lambda snapshot: snapshot["before"] == start + TAB)
    table.record(
        "Tab reaches the app when no ghost is shown",
        moved is not None,
        harness.short(probe.latest()),
    )


def races(table, probe, session):
    inserts = len(session.events(INSERT))
    tab_with_letter()
    moved = probe.wait(lambda snapshot: snapshot["before"].endswith(TAB + LETTER))
    table.record(
        "Tab inside the accept race goes to the app with the letter typed meanwhile",
        moved is not None and len(session.events(INSERT)) == inserts,
        harness.short(probe.latest()),
    )


def run_case(binary, probe_binary, case):
    fixture = FIXTURES[case.fixture]
    with pith(binary, case.insert, case.config) as session:
        with harness.running(fixture.command(), probe_binary) as probe:
            harness.find_window(fixture.title)
            table = harness.Table(case.label, probe)
            name = case.label.replace(",", "").replace(" ", "-")
            if suggests(table, probe, name, case.pill):
                if case.race:
                    races(table, probe, session)
                elif accepts(table, probe, session, case.insert == "accessibility"):
                    dismisses(table, probe, session)
                    if fixture.tab_reaches_app:
                        passes_through(table, probe)
            exit_code = session.stop()
            table.record("pith exits cleanly on SIGTERM", exit_code == 0, f"exit code {exit_code}")
            if any(ok is False for _, _, ok, _ in table.rows):
                print(f"--- {case.label}", file=sys.stderr)
                print(session.output.read_text(), file=sys.stderr)
            return table.rows


def refuses_wayland(binary):
    done = subprocess.run(
        [binary],
        env=dict(os.environ, WAYLAND_DISPLAY="wayland-0"),
        capture_output=True,
        text=True,
        timeout=GHOST_SECONDS,
    )
    return (
        "Pith",
        "refuses a Wayland session",
        done.returncode != 0 and "README" in done.stderr,
        f"exit code {done.returncode}, {done.stderr.strip()!r}",
    )


def main():
    inject.require_ci()
    parser = argparse.ArgumentParser()
    parser.add_argument(inject.CI_FLAG, action="store_true", required=True)
    parser.add_argument("--pith", required=True)
    parser.add_argument("--probe", required=True)
    parser.add_argument("--compositor", choices=("on", "off"), required=True)
    options = parser.parse_args()
    cases = WITH_COMPOSITOR if options.compositor == "on" else WITHOUT_COMPOSITOR
    rows = [refuses_wayland(options.pith)] if options.compositor == "on" else []
    for case in cases:
        rows += run_case(options.pith, options.probe, case)
    harness.report(rows)
    sys.exit(0 if all(ok is not False for _, _, ok, _ in rows) else 1)


if __name__ == "__main__":
    main()
