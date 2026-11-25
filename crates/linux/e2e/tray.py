import argparse
import contextlib
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import threading

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

import harness
import inject
from app import wait_for

GROUP = "Tray"
WATCHER = "org.kde.StatusNotifierWatcher"
WATCHER_PATH = "/StatusNotifierWatcher"
NOTIFICATIONS = "org.freedesktop.Notifications"
NOTIFICATIONS_PATH = "/org/freedesktop/Notifications"
ITEM_PATH = "/StatusNotifierItem"
ITEM_INTERFACE = "org.kde.StatusNotifierItem"
MENU_PATH = "/MenuBar"
MENU_INTERFACE = "com.canonical.dbusmenu"
STARTED_SECONDS = 60.0
STATE_SECONDS = 20.0
NO_TRAY_SECONDS = 60.0
EXIT_SECONDS = 20.0
POLL_TIMEOUT_MS = 5000
CONFIG_FILE = "config.toml"
AUTOSTART_FILE = "autostart/pith.desktop"
PAUSE_LABEL = "Pause for 15 min"
SETTINGS_FLAG = "--settings"
BUS_VARIABLE = "DBUS_SESSION_BUS_ADDRESS"
NO_BUS = "unix:path=/nonexistent/bus"
WRAPPED_ENTRY = b"""[Desktop Entry]
Type=Application
Name=Pith
Exec=env PITH_E2E=1 "/gone/pith" --my-flag
X-GNOME-Autostart-enabled=true

[Desktop Action Extra]
Exec="/gone/pith" --extra
"""
STALE_ENTRY = b"""[Desktop Entry]
Type=Application
Name=Pith
Exec="/gone/with space/pith" "--allow-accessibility" "--keep-me"
Comment=keep me

[Desktop Action Extra]
Exec="/gone/pith" --extra
"""
UNREADABLE_ENTRY = b"[Desktop Entry]\nExec=\xff\xfe\n"

WATCHER_XML = """
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem">
      <arg direction="in" name="service" type="s"/>
    </method>
    <method name="RegisterStatusNotifierHost">
      <arg direction="in" name="service" type="s"/>
    </method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
    <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
  </interface>
</node>
"""

NOTIFICATIONS_XML = """
<node>
  <interface name="org.freedesktop.Notifications">
    <method name="GetCapabilities"><arg direction="out" type="as"/></method>
    <method name="Notify">
      <arg direction="in" type="s"/>
      <arg direction="in" type="u"/>
      <arg direction="in" type="s"/>
      <arg direction="in" type="s"/>
      <arg direction="in" type="s"/>
      <arg direction="in" type="as"/>
      <arg direction="in" type="a{sv}"/>
      <arg direction="in" type="i"/>
      <arg direction="out" type="u"/>
    </method>
    <method name="CloseNotification"><arg direction="in" type="u"/></method>
    <method name="GetServerInformation">
      <arg direction="out" type="s"/>
      <arg direction="out" type="s"/>
      <arg direction="out" type="s"/>
      <arg direction="out" type="s"/>
    </method>
  </interface>
</node>
"""


class Service:
    def __init__(self, connection, name, path, xml, handler, properties):
        interface = Gio.DBusNodeInfo.new_for_xml(xml).interfaces[0]

        def call(_connection, _sender, _path, _interface, method, parameters, invocation):
            invocation.return_value(handler(method, parameters.unpack()))

        def get(_connection, _sender, _path, _interface, key):
            return properties[key]

        self.connection = connection
        self.registration = connection.register_object(path, interface, call, get, None)
        self.owner = Gio.bus_own_name_on_connection(
            connection, name, Gio.BusNameOwnerFlags.NONE, None, None
        )
        if not wait_for(lambda: owns(connection, name), STATE_SECONDS):
            raise RuntimeError(f"{name} was not acquired")

    def stop(self):
        Gio.bus_unown_name(self.owner)
        self.connection.unregister_object(self.registration)


def owns(connection, name):
    return connection.call_sync(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        GLib.Variant("(s)", (name,)),
        GLib.VariantType("(b)"),
        Gio.DBusCallFlags.NONE,
        POLL_TIMEOUT_MS,
        None,
    ).unpack()[0]


class Desktop:
    def __init__(self):
        self.connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.items = []
        self.notices = []
        self.loop = GLib.MainLoop()
        threading.Thread(target=self.loop.run, daemon=True).start()
        self.notifications = Service(
            self.connection,
            NOTIFICATIONS,
            NOTIFICATIONS_PATH,
            NOTIFICATIONS_XML,
            self.notification,
            {},
        )

    def watch(self):
        self.items = []
        return Service(
            self.connection,
            WATCHER,
            WATCHER_PATH,
            WATCHER_XML,
            self.watcher,
            {
                "RegisteredStatusNotifierItems": GLib.Variant("as", []),
                "IsStatusNotifierHostRegistered": GLib.Variant("b", True),
                "ProtocolVersion": GLib.Variant("i", 0),
            },
        )

    def watcher(self, method, arguments):
        if method == "RegisterStatusNotifierItem":
            self.items.append(arguments[0])
        return None

    def notification(self, method, arguments):
        if method == "GetCapabilities":
            return GLib.Variant("(as)", ([],))
        if method == "Notify":
            self.notices.append((arguments[3], arguments[4]))
            return GLib.Variant("(u)", (len(self.notices),))
        if method == "GetServerInformation":
            return GLib.Variant("(ssss)", ("pith-e2e", "pith", "1", "1.2"))
        return None

    def call(self, name, path, interface, method, parameters, reply):
        return self.connection.call_sync(
            name,
            path,
            interface,
            method,
            parameters,
            GLib.VariantType(reply) if reply else None,
            Gio.DBusCallFlags.NONE,
            POLL_TIMEOUT_MS,
            None,
        )

    def property(self, name, key):
        return self.call(
            name,
            ITEM_PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            GLib.Variant("(ss)", (ITEM_INTERFACE, key)),
            "(v)",
        ).unpack()[0]

    def menu(self, name):
        _, (_, _, children) = self.call(
            name,
            MENU_PATH,
            MENU_INTERFACE,
            "GetLayout",
            GLib.Variant("(iias)", (0, -1, [])),
            "(u(ia{sv}av))",
        ).unpack()
        return [
            {
                "id": identifier,
                "label": properties.get("label", ""),
                "enabled": properties.get("enabled", True),
                "separator": properties.get("type") == "separator",
                "checked": properties.get("toggle-state") == 1,
                "checkable": "toggle-type" in properties,
            }
            for identifier, properties, _ in children
        ]

    def click(self, name, label, exits=False):
        item = next(entry for entry in self.menu(name) if entry["label"] == label)
        try:
            self.call(
                name,
                MENU_PATH,
                MENU_INTERFACE,
                "Event",
                GLib.Variant("(isvu)", (item["id"], "clicked", GLib.Variant("i", 0), 0)),
                None,
            )
        except GLib.Error:
            if not exits:
                raise

    def stop(self):
        self.notifications.stop()
        self.loop.quit()


class Pith:
    def __init__(self, binary, workdir, files, environment):
        self.binary = binary
        self.configs = workdir / "config"
        self.data = workdir / "data"
        self.model = workdir / "model.gguf"
        self.model.write_text("not a model")
        for name, content in files.items():
            path = self.configs / name
            path.parent.mkdir(parents=True)
            path.write_bytes(content)
        self.environment = dict(
            os.environ,
            XDG_CONFIG_HOME=str(self.configs),
            XDG_DATA_HOME=str(self.data),
            **environment,
        )
        self.output = workdir / "pith.out"
        self.stderr = self.output.open("w")
        self.process = subprocess.Popen(
            [binary, "--allow-accessibility", "--model", str(self.model)],
            env=self.environment,
            stderr=self.stderr,
        )

    @property
    def log(self):
        path = self.data / "pith" / "pith.log"
        return path.read_text() if path.exists() else ""

    def launch_again(self, *arguments):
        with tempfile.TemporaryFile("w+") as errors:
            process = subprocess.Popen(
                [self.binary, "--allow-accessibility", *arguments],
                env=self.environment,
                stderr=errors,
            )
            try:
                code = process.wait(EXIT_SECONDS)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
                code = None
            errors.seek(0)
            return code, errors.read().strip()

    def stop(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
        try:
            self.process.wait(EXIT_SECONDS)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.stderr.close()
        return self.process.returncode


@contextlib.contextmanager
def session(binary, desktop, tray, files=None, environment=None):
    desktop.notices = []
    watcher = desktop.watch() if tray else None
    with tempfile.TemporaryDirectory(prefix="pith-tray-") as directory:
        pith = Pith(binary, pathlib.Path(directory), files or {}, environment or {})
        try:
            if not wait_for(lambda: "pith starting" in pith.log, STARTED_SECONDS):
                raise RuntimeError(f"pith did not start: {pith.log}")
            yield pith
        finally:
            pith.stop()
            if watcher:
                watcher.stop()


def status(desktop, name):
    return next(
        entry
        for entry in desktop.menu(name)
        if not entry["enabled"] and not entry["separator"]
    )


def labelled(desktop, name, label):
    return next((entry for entry in desktop.menu(name) if entry["label"] == label), None)


def wait_status(desktop, name, prefix):
    return wait_for(lambda: status(desktop, name)["label"].startswith(prefix), STATE_SECONDS)


def with_tray(binary, desktop):
    rows = []

    def record(scenario, ok, detail):
        rows.append((GROUP, scenario, ok, str(detail)))

    with session(binary, desktop, True) as pith:
        name = wait_for(lambda: desktop.items[0] if desktop.items else None, STATE_SECONDS)
        record("the item registers with the watcher", name is not None, desktop.items)
        if name is None:
            record("pith exits cleanly on SIGTERM", pith.stop() == 0, pith.log)
            return rows
        icons = desktop.property(name, "IconPixmap")
        record(
            "the item has a title and an icon",
            desktop.property(name, "Title") == "Pith" and len(icons) > 0 and all(len(data) == w * h * 4 for w, h, data in icons),
            f"{len(icons)} icons",
        )
        labels = [entry["label"] for entry in desktop.menu(name)]
        wanted = ["Open settings", PAUSE_LABEL, "Pause for 1 hour", "Pause until tomorrow", "Pause until I resume", "Resume", "Turn off in this app", "Start at login", "Quit"]
        record("the menu offers the pause choices, the app switch, autostart and quit", all(label in labels for label in wanted), labels)
        record(
            "the status line follows the model",
            wait_status(desktop, name, "Error") is not None,
            status(desktop, name)["label"],
        )
        record(
            "the app switch is off while no app has been read",
            labelled(desktop, name, "Turn off in this app")["enabled"] is False,
            labelled(desktop, name, "Turn off in this app"),
        )
        record("Resume is off while nothing is paused", labelled(desktop, name, "Resume")["enabled"] is False, labelled(desktop, name, "Resume"))
        desktop.click(name, PAUSE_LABEL)
        record("the pause menu pauses", wait_status(desktop, name, "Paused until") is not None, status(desktop, name)["label"])
        record("Resume turns on while paused", labelled(desktop, name, "Resume")["enabled"] is True, labelled(desktop, name, "Resume"))
        desktop.click(name, "Resume")
        record("Resume lifts the pause", wait_status(desktop, name, "Error") is not None, status(desktop, name)["label"])
        autostart = pith.configs / AUTOSTART_FILE
        desktop.click(name, "Start at login")
        written = wait_for(autostart.exists, STATE_SECONDS)
        entry = autostart.read_text() if written else ""
        record(
            "Start at login writes the autostart file",
            written and f'Exec="{os.path.realpath(pith.binary)}" "--allow-accessibility"' in entry,
            entry,
        )
        checked = wait_for(lambda: labelled(desktop, name, "Start at login")["checked"], STATE_SECONDS)
        record("the autostart item shows as checked", checked is True, labelled(desktop, name, "Start at login"))
        desktop.click(name, "Start at login")
        record("Start at login off removes the autostart file", wait_for(lambda: not autostart.exists(), STATE_SECONDS) is True, autostart.exists())
        record(
            "a notice says Pith is running in the tray",
            wait_for(lambda: any("system tray" in body for _, body in desktop.notices), STATE_SECONDS) is True,
            desktop.notices,
        )
        record("a notice reports the model failure", wait_for(lambda: any("could not load its model" in body for _, body in desktop.notices), STATE_SECONDS) is True, desktop.notices)
        code, errors = pith.launch_again(SETTINGS_FLAG)
        settings = pith.configs / "pith" / CONFIG_FILE
        record(
            "a second launch signals the running instance and exits",
            code == 0 and wait_for(lambda: "settings requested" in pith.log, STATE_SECONDS) is True,
            f"exit code {code}, {errors!r}, {pith.log}",
        )
        record("the settings file exists for the editor to open", wait_for(settings.exists, STATE_SECONDS) is True, settings)
        desktop.click(name, "Quit", exits=True)
        try:
            pith.process.wait(EXIT_SECONDS)
            code = pith.process.returncode
        except subprocess.TimeoutExpired:
            code = None
        record("Quit from the menu exits pith cleanly", code == 0, f"exit code {code}")
    return rows


def without_tray(binary, desktop):
    rows = []

    def record(scenario, ok, detail):
        rows.append((GROUP, scenario, ok, str(detail)))

    with session(binary, desktop, False) as pith:
        record("pith runs without a watcher", wait_for(lambda: "no tray icon" in pith.log, STATE_SECONDS) is True, pith.log)
        record(
            "a notice explains the missing tray",
            wait_for(lambda: any("without a tray icon" in body for _, body in desktop.notices), NO_TRAY_SECONDS) is True,
            desktop.notices,
        )
        before = len(desktop.notices)
        launched, errors = pith.launch_again(SETTINGS_FLAG)
        record(
            "a second launch reaches the running instance",
            launched == 0 and wait_for(lambda: "settings requested" in pith.log, STATE_SECONDS) is True,
            f"exit code {launched}, {errors!r}, {pith.log}",
        )
        record(
            "the status is shown as a notice instead",
            wait_for(lambda: len(desktop.notices) > before, STATE_SECONDS) is True,
            desktop.notices[before:],
        )
        code = pith.stop()
        record("pith exits cleanly on SIGTERM", code == 0, f"exit code {code}")
    return rows


def late_tray(binary, desktop):
    rows = []

    def record(scenario, ok, detail):
        rows.append((GROUP, scenario, ok, str(detail)))

    with session(binary, desktop, False) as pith:
        wait_for(lambda: "no tray icon" in pith.log, STATE_SECONDS)
        watcher = desktop.watch()
        try:
            ready = wait_for(lambda: "tray icon ready" in pith.log, STATE_SECONDS)
            welcomed = wait_for(lambda: any("system tray" in body for _, body in desktop.notices), STATE_SECONDS)
        finally:
            watcher.stop()
        record("the item appears when the tray host starts late", ready is True, pith.log)
        record(
            "the first notice waits for the tray and does not say it is missing",
            welcomed is True and not any("without a tray icon" in body for _, body in desktop.notices),
            desktop.notices,
        )
    return rows


def autostart_entries(binary, desktop):
    rows = []

    def record(scenario, ok, detail):
        rows.append((GROUP, scenario, ok, str(detail)))

    with session(binary, desktop, False, files={AUTOSTART_FILE: WRAPPED_ENTRY}) as pith:
        record("an Exec line the user edited is left alone", (pith.configs / AUTOSTART_FILE).read_bytes() == WRAPPED_ENTRY, pith.log)
    with session(binary, desktop, False, files={AUTOSTART_FILE: STALE_ENTRY}) as pith:
        lines = (pith.configs / AUTOSTART_FILE).read_text().splitlines()
        record(
            "a stale program path is repaired and nothing else changes",
            lines == [
                "[Desktop Entry]",
                "Type=Application",
                "Name=Pith",
                f'Exec="{os.path.realpath(pith.binary)}" "--allow-accessibility" "--keep-me"',
                "Comment=keep me",
                "",
                "[Desktop Action Extra]",
                'Exec="/gone/pith" --extra',
            ],
            lines,
        )
    with session(binary, desktop, True, files={AUTOSTART_FILE: UNREADABLE_ENTRY}) as pith:
        name = wait_for(lambda: desktop.items[0] if desktop.items else None, STATE_SECONDS)
        if name is None:
            record("the item registers with the watcher", False, desktop.items)
            return rows
        desktop.click(name, "Start at login")
        refused = wait_for(lambda: "start at login not changed" in pith.log, STATE_SECONDS)
        record(
            "an autostart file that cannot be read is not overwritten",
            refused is True and (pith.configs / AUTOSTART_FILE).read_bytes() == UNREADABLE_ENTRY,
            pith.log,
        )
    return rows


def without_bus(binary, desktop):
    rows = []

    def record(scenario, ok, detail):
        rows.append((GROUP, scenario, ok, str(detail)))

    with session(binary, desktop, False, environment={BUS_VARIABLE: NO_BUS}) as pith:
        try:
            code = pith.process.wait(EXIT_SECONDS)
        except subprocess.TimeoutExpired:
            code = None
        output = pith.output.read_text()
        record(
            "without a session bus the single instance check is skipped, not fatal",
            "single instance not checked" in pith.log and "single instance:" not in output,
            f"{pith.log} {output!r}",
        )
        record("without a session bus pith stops on the missing accessibility bus", code not in (None, 0), f"exit code {code}, {output!r}")
    return rows


def main():
    inject.require_ci()
    parser = argparse.ArgumentParser()
    parser.add_argument(inject.CI_FLAG, action="store_true", required=True)
    parser.add_argument("--pith", required=True)
    options = parser.parse_args()
    desktop = Desktop()
    try:
        rows = (
            without_tray(options.pith, desktop)
            + late_tray(options.pith, desktop)
            + with_tray(options.pith, desktop)
            + autostart_entries(options.pith, desktop)
            + without_bus(options.pith, desktop)
        )
    finally:
        desktop.stop()
    harness.report(rows)
    sys.exit(0 if all(ok is not False for _, _, ok, _ in rows) else 1)


if __name__ == "__main__":
    main()
