import sys

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk


def build(kind, seed):
    if kind == "textview":
        field = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD)
        buffer = field.get_buffer()
        buffer.set_text(seed)
        buffer.place_cursor(buffer.get_end_iter())
        return field
    field = Gtk.Entry(visibility=kind != "password")
    field.set_text(seed)
    field.set_position(-1)
    return field


def activate(application, kind, seed):
    window = Gtk.ApplicationWindow(
        application=application,
        title=f"pith-gtk3-{kind}",
        default_width=480,
        default_height=160,
    )
    field = build(kind, seed)
    window.add(field)
    window.show_all()
    getattr(field, "grab_focus_without_selecting", field.grab_focus)()


application = Gtk.Application()
application.connect("activate", activate, sys.argv[1], "".join(sys.argv[2:]))
application.run(None)
