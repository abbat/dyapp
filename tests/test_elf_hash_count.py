"""Loader hash counts expose truncated or contradictory symbol inventories."""

from pathlib import Path
import runpy
import struct
import unittest

ROOT = Path(__file__).resolve().parents[1]
CHECKER = runpy.run_path(str(ROOT / "tools/scripts/check-hardening.py"))


def gnu(heads: tuple[int, ...], chains: tuple[int, ...]) -> bytes:
    return (
        struct.pack("<IIIIQ", len(heads), 3, 1, 6, 0)
        + struct.pack(f"<{len(heads)}I", *heads)
        + struct.pack(f"<{len(chains)}I", *chains)
    )


class ElfHashCount(unittest.TestCase):
    def test_gnu_chains_cover_every_counted_symbol(self) -> None:
        self.assertEqual(CHECKER["elf_gnu_count"](gnu((3, 5), (2, 3, 5))), 6)
        self.assertEqual(CHECKER["elf_gnu_count"](gnu((5, 3), (2, 3, 5))), 6)
        self.assertEqual(CHECKER["elf_gnu_count"](gnu((0,), ())), 3)

    def test_gnu_gaps_overlap_missing_terminators_and_tails_fail(self) -> None:
        for heads, chains in (
            ((4,), (1, 1)),
            ((3, 3), (1,)),
            ((3,), (2,)),
            ((3,), (1, 1)),
            ((0,), (1,)),
            ((2,), (1,)),
        ):
            with self.subTest(heads=heads, chains=chains):
                with self.assertRaises(ValueError):
                    CHECKER["elf_gnu_count"](gnu(heads, chains))

    def test_malformed_gnu_dimensions_fail(self) -> None:
        valid = gnu((3,), (1,))
        for data in (valid[:8], valid[:-1], bytes(24)):
            with self.subTest(data=data):
                with self.assertRaises(ValueError):
                    CHECKER["elf_gnu_count"](data)

    def test_sysv_count_and_indices_are_bounded(self) -> None:
        self.assertEqual(
            CHECKER["elf_sysv_count"](struct.pack("<6I", 1, 3, 1, 0, 2, 0)), 3
        )
        for values in ((1, 3, 3, 0, 0, 0), (1, 4, 0, 0, 0, 0), (0, 0)):
            with self.subTest(values=values):
                with self.assertRaises(ValueError):
                    CHECKER["elf_sysv_count"](
                        struct.pack(f"<{len(values)}I", *values)
                    )


if __name__ == "__main__":
    unittest.main()
