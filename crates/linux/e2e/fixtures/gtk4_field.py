import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk


def build(kind, seed):
    if kind == "textview":
        field = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD)
        buffer = field.get_buffer()
        buffer.set_text(seed)
        buffer.place_cursor(buffer.get_end_iter())
        return field
    field = Gtk.PasswordEntry() if kind == "password" else Gtk.Entry()
    field.set_text(seed)
    field.set_position(-1)
    return field


def activate(application, kind, seed):
    window = Gtk.ApplicationWindow(
        application=application,
        title=f"pith-gtk4-{kind}",
        default_width=480,
        default_height=160,
    )
    field = build(kind, seed)
    window.set_child(field)
    window.present()
    field.grab_focus()


application = Gtk.Application()
application.connect("activate", activate, sys.argv[1], "".join(sys.argv[2:]))
application.run(None)
