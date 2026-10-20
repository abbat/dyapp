"""B4 inventory: every network request handler has a fuzz target."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_fuzz", ROOT / "scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class Fuzz(unittest.TestCase):
    def test_handlers_without_a_target_fail(self):
        texts = {
            CHECK.HANDLERS[0]: (
                "pub fn node(&self, peer: &Peer, request: NodeRequest)\n"
                "pub fn gossip(&self, data: &[u8]) -> Verdict\n"
                "pub fn media(\n    &self,\n    peer: &Peer,\n"
                "    request: MediaRequest,\n) -> Result\n"
                "pub fn held(&self, mailbox: &[u8])\n"),
            "fuzz/fuzz_targets/node.rs": "SERVICE.node(&PEER, request);\n",
        }
        self.assertEqual(CHECK.fuzz_errors(texts), [
            f"{CHECK.HANDLERS[0]}: Service::gossip has no fuzz target",
            f"{CHECK.HANDLERS[0]}: Service::media has no fuzz target",
        ])

    def test_repository_handlers_all_have_targets(self):
        targets = (ROOT / "fuzz/fuzz_targets").glob("*.rs")
        names = [*CHECK.HANDLERS,
                 *(str(path.relative_to(ROOT)) for path in targets)]
        texts = {name: (ROOT / name).read_text() for name in names}
        self.assertEqual(CHECK.fuzz_errors(texts), [])
        self.assertEqual(sum(len(CHECK.HANDLER.findall(texts[name]))
                             for name in CHECK.HANDLERS), 8)


if __name__ == "__main__":
    unittest.main()
