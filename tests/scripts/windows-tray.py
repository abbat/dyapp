"""Clicks the app's notification-area menu (Open, Quit) for the desktop smoke
on Windows, as fake-tray.py does through a StatusNotifierItem host on Linux.

quit: clicks Quit once the icon is there and the window has its taskbar
icons; open: clicks Open once the app writes hidden to the config dir."""
import ctypes
from ctypes import wintypes
from pathlib import Path
import sys
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.FindWindowW.restype = wintypes.HWND
user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM,
                                wintypes.LPARAM]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.SendMessageW.restype = wintypes.LPARAM
user32.SendMessageW.argtypes = user32.PostMessageW.argtypes

# tray-icon's hidden window and its callback message; #32768 is a popup menu.
TRAY_CLASS, WM_USER_TRAYICON, MENU_CLASS = "tray_icon_app", 6002, "#32768"
WM_KEYDOWN, WM_RBUTTONUP, VK_RETURN, VK_DOWN = 0x100, 0x205, 0x0D, 0x28
WM_GETICON, ICON_SMALL, ICON_BIG = 0x7F, 0, 1

mode, trigger = sys.argv[1], Path(sys.argv[2]) / "hidden"
print("ready", flush=True)
while True:
    time.sleep(1)
    tray = user32.FindWindowW(TRAY_CLASS, None)
    if not tray or (mode != "quit" and not trigger.exists()):
        continue
    if mode == "quit":
        # The taskbar shows the window's icons; without them, no Quit.
        window = user32.FindWindowW(None, "DYApp")
        if not all(user32.SendMessageW(window, WM_GETICON, kind, 0)
                   for kind in (ICON_SMALL, ICON_BIG)):
            sys.exit("windows-tray: the DYApp window has no taskbar icon")
    menu = user32.FindWindowW(MENU_CLASS, None)
    if not menu or not user32.IsWindowVisible(menu):
        # Clicks until stopped: a click before the menu is wired is lost.
        user32.PostMessageW(tray, WM_USER_TRAYICON, 0, WM_RBUTTONUP)
        continue
    for _ in range(1 if mode == "open" else 2):
        user32.PostMessageW(menu, WM_KEYDOWN, VK_DOWN, 0)
    user32.PostMessageW(menu, WM_KEYDOWN, VK_RETURN, 0)
