"""A StatusNotifierItem host for the desktop smoke: owns the watcher name on
the session bus, takes the app's tray item and clicks its menu (Open, Quit).

quit: clicks Quit once the item is there; open: clicks Open once the app
writes hidden to the config dir; vanish: exits once the app writes it."""
from pathlib import Path
import sys

from gi.repository import Gio, GLib

WATCHER = "org.kde.StatusNotifierWatcher"
NODE = Gio.DBusNodeInfo.new_for_xml(f"""<node><interface name="{WATCHER}">
<method name="RegisterStatusNotifierItem"><arg type="s"/></method>
<method name="RegisterStatusNotifierHost"><arg type="s"/></method>
<property name="RegisteredStatusNotifierItems" type="as" access="read"/>
<property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
<property name="ProtocolVersion" type="i" access="read"/>
</interface></node>""")

mode, trigger = sys.argv[1], Path(sys.argv[2]) / "hidden"
bus = Gio.bus_get_sync(Gio.BusType.SESSION)
loop = GLib.MainLoop()
items = []


def call(name, path, interface, method, args, reply=None):
    return bus.call_sync(name, path, interface, method, args,
                         reply and GLib.VariantType(reply),
                         Gio.DBusCallFlags.NONE, 5000, None)


def click(position):
    """Sends "clicked" to the menu entry at `position` of the newest item."""
    name, path = items[-1]
    menu = call(name, path, "org.freedesktop.DBus.Properties", "Get",
                GLib.Variant("(ss)", ("org.kde.StatusNotifierItem", "Menu")),
                "(v)").unpack()[0]
    _, (_, _, children) = call(
        name, menu, "com.canonical.dbusmenu", "GetLayout",
        GLib.Variant("(iias)", (0, -1, [])), "(u(ia{sv}av))").unpack()
    entries = [c[0] for c in children if c[1].get("type") != "separator"]
    call(name, menu, "com.canonical.dbusmenu", "Event",
         GLib.Variant("(isvu)", (entries[position], "clicked",
                                 GLib.Variant("s", ""), 0)))


def tick():
    if not items or (mode != "quit" and not trigger.exists()):
        return True
    if mode == "vanish":
        loop.quit()
        return False
    # Clicks again until stopped: a click before the menu is wired up is lost.
    try:
        click(0 if mode == "open" else 1)
    except (GLib.Error, IndexError) as error:
        print(f"fake tray: {error}", file=sys.stderr, flush=True)
    return True


def method(_bus, sender, _path, _interface, name, params, invocation):
    if name == "RegisterStatusNotifierItem":
        service = params.unpack()[0]
        # appindicator registers an object path on its own connection.
        items.append((sender, service) if service.startswith("/")
                     else (service, "/StatusNotifierItem"))
    invocation.return_value(None)


def get(_bus, _sender, _path, _interface, name):
    return {
        "RegisteredStatusNotifierItems":
            GLib.Variant("as", [name + path for name, path in items]),
        "IsStatusNotifierHostRegistered": GLib.Variant("b", True),
        "ProtocolVersion": GLib.Variant("i", 0),
    }[name]


bus.register_object("/StatusNotifierWatcher", NODE.interfaces[0], method, get,
                    None)
Gio.bus_own_name_on_connection(
    bus, WATCHER, Gio.BusNameOwnerFlags.NONE,
    lambda *_: print("ready", flush=True), lambda *_: loop.quit())
GLib.timeout_add(1000, tick)
loop.run()
