import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk

FIELDS = {
    "entry": Gtk.Entry,
    "textview": lambda: Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD),
    "password": Gtk.PasswordEntry,
}


def activate(application, kind):
    window = Gtk.ApplicationWindow(
        application=application,
        title=f"pith-gtk4-{kind}",
        default_width=480,
        default_height=160,
    )
    field = FIELDS[kind]()
    window.set_child(field)
    window.present()
    field.grab_focus()


application = Gtk.Application()
application.connect("activate", activate, sys.argv[1])
application.run(None)
