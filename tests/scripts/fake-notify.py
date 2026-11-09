"""A notification server for the desktop smoke: owns the freedesktop name on
the session bus, keeps each notice in notices.json of the config dir, and
clicks the newest one once the app writes hidden there."""

import json
from pathlib import Path
import sys

from gi.repository import Gio, GLib

NAME = "org.freedesktop.Notifications"
PATH = "/org/freedesktop/Notifications"
NODE = Gio.DBusNodeInfo.new_for_xml(f"""<node><interface name="{NAME}">
<method name="Notify">
<arg direction="in" type="s"/><arg direction="in" type="u"/>
<arg direction="in" type="s"/><arg direction="in" type="s"/>
<arg direction="in" type="s"/><arg direction="in" type="as"/>
<arg direction="in" type="a{{sv}}"/><arg direction="in" type="i"/>
<arg direction="out" type="u"/>
</method>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>""")

folder = Path(sys.argv[2])
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
loop = GLib.MainLoop()
notices = []
owner = ""


def method(_bus, sender, _path, _interface, _name, params, invocation):
    global owner  # pylint: disable=global-statement
    owner = sender
    _, replaces, _, summary, body, actions, _, _ = params.unpack()
    notices.append(
        {
            "replaces": replaces,
            "summary": summary,
            "body": body,
            "actions": actions,
        }
    )
    (folder / "notices.json").write_text(json.dumps(notices))
    invocation.return_value(GLib.Variant("(u)", (len(notices),)))


def tick():
    # Clicks again until stopped: a click before the app listens is lost. Only
    # the asking connection hears it, as from a real server.
    if notices and (folder / "hidden").exists():
        bus.emit_signal(
            owner,
            PATH,
            NAME,
            "ActionInvoked",
            GLib.Variant("(us)", (len(notices), "default")),
        )
    return True


bus.register_object(PATH, NODE.interfaces[0], method, None, None)
Gio.bus_own_name_on_connection(
    bus,
    NAME,
    Gio.BusNameOwnerFlags.NONE,
    lambda *_: print("ready", flush=True),
    lambda *_: loop.quit(),
)
GLib.timeout_add(500, tick)
loop.run()
