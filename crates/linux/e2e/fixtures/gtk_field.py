import sys

import gi

VERSION, KIND, SEED = sys.argv[1], sys.argv[2], "".join(sys.argv[3:])
gi.require_version("Gtk", VERSION)
from gi.repository import Gtk

GTK4 = VERSION == "4.0"
WIDTH, HEIGHT = 480, 160


def password_entry():
    return Gtk.PasswordEntry() if GTK4 else Gtk.Entry(visibility=False)


def text_view():
    field = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD)
    buffer = field.get_buffer()
    buffer.set_text(SEED)
    buffer.place_cursor(buffer.get_end_iter())
    return field


def single_line(field):
    field.set_text(SEED)
    field.set_position(-1)
    return field


def add(box, child, grows=False):
    if GTK4:
        child.set_vexpand(grows)
        box.append(child)
    else:
        box.pack_start(child, grows, grows, 0)


def form():
    box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    secret = password_entry()
    secret.set_text("hunter2")
    field = text_view()
    add(box, Gtk.Label(label="Subject"))
    add(box, Gtk.Entry(text="Quarterly numbers"))
    add(box, secret)
    add(box, field, grows=True)
    return box, field


def build():
    if KIND == "form":
        return form()
    if KIND == "textview":
        field = text_view()
    else:
        field = single_line(password_entry() if KIND == "password" else Gtk.Entry())
    return field, field


def activate(application):
    window = Gtk.ApplicationWindow(
        application=application,
        title=f"pith-gtk{VERSION[0]}-{KIND}",
        default_width=WIDTH,
        default_height=HEIGHT,
    )
    root, field = build()
    if GTK4:
        window.set_child(root)
        window.present()
    else:
        window.add(root)
        window.show_all()
    getattr(field, "grab_focus_without_selecting", field.grab_focus)()


application = Gtk.Application()
application.connect("activate", activate)
application.run(None)
