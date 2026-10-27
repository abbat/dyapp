"""Writes rust/client/data/places.z from GeoNames cities15000 (CC BY 4.0).

The data is zlib-compressed UTF-8 lines of tab-separated fields: the regions
(country.admin1 code, name, Russian name), an empty line, then the places by
country and GeoNames ID (country, ID minus the previous ID of the country,
bit length of the population, admin1 code, name, Russian name). A Russian
field is empty when GeoNames has no Russian name or it equals the English
one. Places missing from the node's rust/bootstrap/data/places.bin are
skipped: the node would index them under the country only.
Usage: python3 -I tools/scripts/geonames-names.py DIR [out]
DIR holds cities15000.zip, admin1CodesASCII.txt and alternateNamesV2.zip from
https://download.geonames.org/export/dump/; download them into an empty
directory first.
"""
import io
import sys
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "rust/client/data/places.z"
SERVER = ROOT / "rust/bootstrap/data/places.bin"


def lines(path, member):
    with zipfile.ZipFile(path) as z, z.open(member) as f:
        for line in io.TextIOWrapper(f, encoding="utf-8"):
            yield line.rstrip("\n").split("\t")


def server_ids():
    """The (country, ID) pairs of places.bin; see its writer
    geonames-places.py."""
    data, known = SERVER.read_bytes(), set()

    def leb128(at):
        n = shift = 0
        while True:
            n |= (data[at] & 0x7F) << shift
            shift += 7
            at += 1
            if data[at - 1] < 0x80:
                return n, at

    at = 0
    while at < len(data):
        country = data[at:at + 2].decode()
        length, at = leb128(at + 2)
        end, gid = at + length, 0
        while at < end:
            delta, at = leb128(at)
            gid += delta
            known.add((country, gid))
    return known


def russian(path, wanted):
    """The Russian name of each wanted ID: a preferred one first, never a
    colloquial or historic one."""
    best = {}
    for c in lines(path, "alternateNamesV2.txt"):
        gid = int(c[1])
        if c[2] != "ru" or gid not in wanted or "1" in (c[6], c[7]):
            continue
        if gid not in best or c[4] == "1" and not best[gid][0]:
            best[gid] = (c[4] == "1", c[3])
    return {gid: name for gid, (_, name) in best.items()}


def main():
    src = Path(sys.argv[1])
    regions = {}
    with open(src / "admin1CodesASCII.txt", encoding="utf-8") as f:
        for line in f:
            code, name, _, gid = line.rstrip("\n").split("\t")
            regions[code] = (name, int(gid))
    known = server_ids()
    places, unknown = [], 0
    for c in lines(src / "cities15000.zip", "cities15000.txt"):
        key = (c[8], int(c[0]))
        if key not in known:
            unknown += 1
            continue
        region = c[10] if f"{c[8]}.{c[10]}" in regions else ""
        size = int(c[14] or 0).bit_length()
        places.append((c[8], key[1], size, c[1], region))
    used = {f"{p[0]}.{p[4]}" for p in places if p[4]}
    ru = russian(src / "alternateNamesV2.zip",
                 {p[1] for p in places} | {regions[r][1] for r in used})

    def other(name, gid):
        return "" if ru.get(gid, name) == name else ru[gid]

    text = "".join(
        f"{code}\t{name}\t{other(name, gid)}\n"
        for code, (name, gid) in sorted(regions.items()) if code in used)
    text += "\n"
    last = (None, 0)
    for country, gid, size, name, region in sorted(places):
        delta = gid - last[1] if country == last[0] else gid
        last = (country, gid)
        text += (f"{country}\t{delta}\t{size}\t{region}\t{name}"
                 f"\t{other(name, gid)}\n")
    if any(ch in field for p in places for field in (p[3], p[4])
           for ch in "\t\n"):
        raise SystemExit("a name holds a tab or newline")
    data = zlib.compress(text.encode(), 9)
    out = sys.argv[2] if len(sys.argv) > 2 else OUT
    with open(out, "wb") as f:
        f.write(data)
    print(f"{len(places)} places ({unknown} not in places.bin skipped), "
          f"{len(ru)} Russian names, "
          f"{len(data)} bytes -> {out}")


if __name__ == "__main__":
    main()
