"""Checks that a Windows executable carries the app icon: its first icon group
holds the 16, 32 and 256 px images of the ICO byte for byte. Explorer, the
taskbar and the window take their icon from that group.

Usage: check-windows-icon.py EXE ICO. Windows only."""
import ctypes
import ctypes.wintypes as wintypes
from pathlib import Path
import struct
import sys

SIZES = (16, 32, 256)
RT_ICON, RT_GROUP_ICON = 3, 14
LOAD_AS_DATA = 0x2 | 0x20  # LOAD_LIBRARY_AS_DATAFILE | AS_IMAGE_RESOURCE


def ico_images(data):
    """{size: image bytes} of an ICO file; 0 in the directory means 256."""
    count = struct.unpack_from("<HHH", data)[2]
    images = {}
    for i in range(count):
        width, _, _, _, _, _, size, offset = struct.unpack_from(
            "<BBBBHHII", data, 6 + 16 * i)
        images[width or 256] = data[offset:offset + size]
    return images


def exe_images(path):
    """{size: image bytes} of the first RT_GROUP_ICON in the executable."""
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.LoadLibraryExW.restype = wintypes.HMODULE
    kernel32.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE,
                                        wintypes.DWORD]
    kernel32.FindResourceW.restype = wintypes.HRSRC
    kernel32.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p,
                                       ctypes.c_void_p]
    kernel32.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
    kernel32.LoadResource.restype = wintypes.HGLOBAL
    kernel32.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HRSRC]
    kernel32.LockResource.restype = ctypes.c_void_p
    kernel32.LockResource.argtypes = [wintypes.HGLOBAL]
    callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HMODULE,
                                  ctypes.c_void_p, ctypes.c_void_p,
                                  ctypes.c_void_p)
    kernel32.EnumResourceNamesW.argtypes = [wintypes.HMODULE, ctypes.c_void_p,
                                            callback, ctypes.c_void_p]

    module = kernel32.LoadLibraryExW(str(path), None, LOAD_AS_DATA)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())

    def resource(name, kind):
        found = kernel32.FindResourceW(module, name, kind)
        if not found:
            raise RuntimeError(f"resource {name} of type {kind} missing")
        size = kernel32.SizeofResource(module, found)
        pointer = kernel32.LockResource(kernel32.LoadResource(module, found))
        return ctypes.string_at(pointer, size)

    groups = []

    def first(_module, _kind, name, _param):
        # The pointer stays valid while the module is loaded; False stops.
        groups.append(name)
        return False

    kernel32.EnumResourceNamesW(module, RT_GROUP_ICON, callback(first), None)
    if not groups:
        raise RuntimeError(f"{path} has no icon group")
    group = resource(groups[0], RT_GROUP_ICON)
    images = {}
    for i in range(struct.unpack_from("<HHH", group)[2]):
        width, *_, icon_id = struct.unpack_from("<BBBBHHIH", group, 6 + 14 * i)
        images[width or 256] = resource(icon_id, RT_ICON)
    return images


def main():
    exe, ico = Path(sys.argv[1]), Path(sys.argv[2])
    expected, found = ico_images(ico.read_bytes()), exe_images(exe)
    for size in SIZES:
        if size not in expected:
            raise RuntimeError(f"{ico} has no {size} px image")
        if found.get(size) != expected[size]:
            raise RuntimeError(f"{exe} {size} px icon differs from {ico}")
    print(f"OK {exe.name} icon {', '.join(map(str, SIZES))} px")


if __name__ == "__main__":
    main()
