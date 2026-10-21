"""Writes rust/bootstrap/data/places.bin from GeoNames cities1000 (CC BY 4.0).

One block per country, sorted: the ISO country code (2 bytes), the body
length (LEB128) and the body, the country's sorted GeoNames IDs as LEB128
deltas from the previous ID (the first from 0).
Usage: python3 -I tools/scripts/geonames-places.py cities1000.zip [out]
Download cities1000.zip from https://download.geonames.org/export/dump/
into an empty directory first.
"""
import io
import itertools
import sys
import zipfile

OUT = "rust/bootstrap/data/places.bin"


def leb128(n):
    out = bytearray()
    while n > 0x7F:
        out.append(n & 0x7F | 0x80)
        n >>= 7
    return bytes(out + bytes([n]))


def main():
    rows = set()
    with zipfile.ZipFile(sys.argv[1]) as z, z.open("cities1000.txt") as f:
        for line in io.TextIOWrapper(f, encoding="utf-8"):
            cols = line.rstrip("\n").split("\t")
            country = cols[8]
            if len(country) == 2 and country.isascii() and country.isupper():
                rows.add((country.encode(), int(cols[0])))
    data = bytearray()
    for country, group in itertools.groupby(sorted(rows), key=lambda r: r[0]):
        ids = [gid for _, gid in group]
        body = b"".join(leb128(b - a) for a, b in zip([0] + ids, ids))
        data += country + leb128(len(body)) + body
    out = sys.argv[2] if len(sys.argv) > 2 else OUT
    with open(out, "wb") as f:
        f.write(data)
    print(f"{len(rows)} places, {len(data)} bytes -> {out}")


if __name__ == "__main__":
    main()
