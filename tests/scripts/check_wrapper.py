"""Make the shared check runner available to standalone test scripts."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/scripts"))
from capture import Capture  # noqa: E402,F401
from check_dispatch import wrap  # noqa: E402,F401
