"""Launch a real bundled Tauri app and check ordered UI flows."""

import importlib.util
from concurrent.futures import ThreadPoolExecutor
import contextlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time


native_path = Path(__file__).with_name("desktop_smoke_native.py")
native_spec = importlib.util.spec_from_file_location(
    "desktop_smoke_native", native_path
)
assert native_spec is not None and native_spec.loader is not None, (
    "Python source loader is unavailable"
)
native = importlib.util.module_from_spec(native_spec)
native_spec.loader.exec_module(native)
display_diagnostics = native.display_diagnostics
unlock_keyring = native.unlock_keyring
run = native.run
read_varint = native.read_varint
signed_profile_payload = native.signed_profile_payload
check_profile_readback = native.check_profile_readback
forge_search_signature = native.forge_search_signature
expire_sticker_media = native.expire_sticker_media
video_smoke = native.video_smoke
start_smoke_node = native.start_smoke_node


class DesktopSmoke:
    """Own smoke results and the shared application launcher."""

    def __init__(self, executable) -> None:
        self.executable = executable
        report_root = os.environ.get("RUNNER_TEMP")
        if not report_root:
            report_root = f"/reports/{os.environ['HOSTNAME']}"
        self.reports = Path(report_root)
        self.reports.mkdir(parents=True, exist_ok=True)
        self.results = []

    def case(
        self,
        name,
        extra,
        profile,
        broken: bool = False,
        peer: bool = True,
        timeout: int = 120,
        launcher=(),
        extra_env=None,
    ):
        arguments = [*launcher, self.executable, "--ui-test", *extra]
        if not launcher:
            unlock_keyring()
        if os.environ.get("DYAPP_SMOKE_RU") == "1":
            arguments.append("--ui-test-ru")
        env = dict(os.environ, DYAPP_TEST_CONFIG_DIR=str(profile))
        env.update(extra_env or {})
        process = run(arguments, timeout=timeout, env=env)
        match = re.search(r"UI_SMOKE_PASS:([0-9a-f]{64})?", process.stdout)
        found = match is not None and (match[1] is not None or not peer)
        marker_found = "UI_SMOKE_FAIL" in process.stdout if broken else found
        passed = process.returncode == int(broken) and marker_found
        if not passed:
            raise RuntimeError(
                f"UI suite failed: {arguments}, exit={process.returncode}\n"
                f"{process.stdout}\n{process.stderr}"
            )
        print(process.stdout, end="")
        print(process.stderr, end="", file=sys.stderr)
        result = {"case": name, "exit": process.returncode, "passed": True}
        if name.startswith("video-"):
            metrics = re.search(
                r"^UI_SMOKE_METRICS:video:(?:360p|720p):(\{[^\n]{1,500}\})$",
                process.stdout,
                re.M,
            )
            if not metrics:
                raise RuntimeError(f"{name} did not report bounded metrics")
            result["metrics"] = json.loads(metrics[1])
        self.results.append(result)
        return match.group(1) if match else None

    def remove(self, name, profile, slot, first) -> None:
        """Remove the opened identity; only its slot folder may go."""
        slots = Path(profile) / "profiles"
        kept = {s.name for s in slots.iterdir()} - {slot}
        if self.case(name, ["--ui-test-step=remove"], profile) != first:
            raise RuntimeError("the remaining identity did not open")
        if {s.name for s in slots.iterdir()} != kept:
            raise RuntimeError("removal touched the wrong slots")
        reopened = self.case(
            f"{name}-restart", ["--ui-test-step=settings-restart"], profile
        )
        if reopened != first:
            raise RuntimeError("removed identity reopened after restart")

    def window(self, profile: "str") -> None:
        """Ctrl+Q saves the moved window, a relaunch puts it back, and one
        saved on a display that went away comes back centred."""
        state = Path(profile) / "window.json"
        file = ["--ui-test-file"]
        self.case(
            "window-save",
            [*file, "--ui-test-step=window:save"],
            profile,
            peer=False,
        )
        saved = json.loads(state.read_text())
        if saved != {
            "normal": {"x": 120, "y": 80, "width": 900, "height": 640},
            "maximized": False,
        }:
            raise RuntimeError(f"window state not saved: {saved}")
        self.case(
            "window-restore",
            [*file, "--ui-test-step=window:restore"],
            profile,
            peer=False,
        )
        saved["normal"]["x"] = -5000
        state.write_text(json.dumps(saved))
        self.case(
            "window-off-screen",
            [*file, "--ui-test-step=window:centre"],
            profile,
            peer=False,
        )

    def linked(self, owner, owner_id, seed, file) -> None:
        """A new client joins the identity of `owner` through the node, both
        list two devices, and a disconnect wipes the new client. It waits on
        a bus of its own: on a shared one the single-instance check would
        hand its launch to the running owner."""
        with tempfile.TemporaryDirectory(dir=self.reports) as fresh:
            fresh = Path(fresh)
            written = fresh / "link-code"
            with ThreadPoolExecutor(1) as pool:
                joined = pool.submit(
                    self.case,
                    "link-wait",
                    [*file, f"--ui-test-step=link-wait:{seed}"],
                    fresh,
                    timeout=180,
                    launcher=["dbus-run-session", "--"],
                )
                code = ""
                while not code and not joined.done():
                    code = written.read_text() if written.is_file() else ""
                    time.sleep(0.5)
                if code:
                    self.case(
                        "link-send",
                        [*file, f"--ui-test-step=link-send:{code}"],
                        owner,
                    )
                if joined.result() != owner_id:
                    raise RuntimeError("the client joined another identity")
            slots = fresh / "profiles"
            if len(list(slots.iterdir())) != 1:
                raise RuntimeError("the wizard's slot outlived the link")
            self.case(
                "link-disconnect",
                [*file, "--ui-test-step=link-disconnect"],
                owner,
            )
            self.case(
                "link-wiped",
                [*file, "--ui-test-step=link-wiped"],
                fresh,
                peer=False,
            )
            if any(slots.iterdir()):
                raise RuntimeError("the wipe left the identity on disk")

    def single(self, profile: "str") -> None:
        """A second launch shows the hidden window of the running one and
        exits."""
        hidden = Path(profile) / "hidden"
        with ThreadPoolExecutor(1) as pool:
            first = pool.submit(
                self.case,
                "single-instance",
                ["--ui-test-file", "--ui-test-step=single"],
                profile,
                peer=False,
            )
            for _ in range(120):
                if hidden.exists() or first.done():
                    break
                time.sleep(0.5)
            if hidden.exists():
                env = dict(os.environ, DYAPP_TEST_CONFIG_DIR=str(profile))
                second = run([self.executable], timeout=30, env=env)
                if second.returncode != 0:
                    raise RuntimeError(f"second launch failed: {second}")
            first.result()

    def sticker_packs(self, profile, mode, flags, peer_id) -> None:
        for step in ("import", "restart"):
            reopened = self.case(
                f"{mode}-sticker-{step}",
                [*flags, f"--ui-test-step=sticker-{step}"],
                profile,
            )
            if reopened != peer_id:
                raise RuntimeError("sticker smoke changed the active identity")

    def mixed(self, profile, first) -> None:
        """Add a file identity to a system one, then crash, bad pointer."""
        root = Path(profile)
        file = ["--ui-test-file"]
        second = self.case(
            "mixed-add", [*file, "--ui-test-step=add"], profile, timeout=300
        )
        if second == first:
            raise RuntimeError("added identity reused the Peer ID")
        added = (root / "active-profile").read_text().strip()
        # A crash after reservation leaves a pending slot without a key.
        pending = root / "profiles" / "00000000-0000-4000-8000-000000000000"
        pending.mkdir()
        (pending / "storage-mode").write_bytes(b"file\n")
        (pending / "state").write_bytes(b"pending\n")
        if (
            self.case("crash-restart", [*file, "--ui-test-existing"], profile)
            != second
        ):
            raise RuntimeError("pending slot replaced the active identity")
        (root / "active-profile").write_bytes(b"../escape\n")
        if (
            self.case(
                "corrupt-pointer-pick", [*file, "--ui-test-step=pick"], profile
            )
            != second
        ):
            raise RuntimeError("picker did not reopen the file identity")
        if (
            self.case("pick-restart", [*file, "--ui-test-existing"], profile)
            != second
        ):
            raise RuntimeError("picked identity was not reopened")
        self.remove("remove-ready", profile, added, first)
        self.case("readd", [*file, "--ui-test-step=add"], profile, timeout=300)
        added = (root / "active-profile").read_text().strip()
        key = root / "profiles" / added / "identity.key"
        key.write_bytes(b"not a key\n")
        self.case(
            "broken-key",
            [*file, "--ui-test-step=recovery:unavailable"],
            profile,
            peer=False,
        )
        key.unlink()
        self.case(
            "missing-key",
            [*file, "--ui-test-step=recovery:missing"],
            profile,
            peer=False,
        )
        self.remove("remove-missing", profile, added, first)

    def system_files_only(self, profile: "str") -> None:
        """An app-file copy cannot recover a credential in a fresh provider."""
        root = Path(profile)
        slot = (root / "active-profile").read_text().strip()
        with tempfile.TemporaryDirectory(dir=self.reports) as restored:
            restored = Path(restored)
            (restored / "profiles").mkdir()
            shutil.copytree(
                root / "profiles" / slot, restored / "profiles" / slot
            )
            home = restored / "home"
            home.mkdir()
            runtime = restored / "runtime"
            runtime.mkdir(mode=0o700)
            launcher = [
                "dbus-run-session",
                "--",
                "bash",
                str(Path(__file__).with_name("fresh-keyring.sh")),
            ]
            self.case(
                "system-files-only",
                ["--ui-test-step=recovery:missing"],
                restored,
                peer=False,
                launcher=launcher,
                extra_env={
                    "HOME": str(home),
                    "XDG_DATA_HOME": str(home / "data"),
                    "XDG_CONFIG_HOME": str(home / "config"),
                    "XDG_RUNTIME_DIR": str(runtime),
                },
            )

    def legacy(self, profile, peer_id) -> None:
        """Turn the file slot into a pre-slot root key and reopen it."""
        root = Path(profile)
        slot = (root / "active-profile").read_text().strip()
        (root / "profiles" / slot / "identity.key").rename(
            root / "identity.key"
        )
        (root / "identity-storage").write_bytes(b"file\n")
        shutil.rmtree(root / "profiles")
        (root / "active-profile").unlink()
        if (
            self.case(
                "legacy-upgrade",
                ["--ui-test-file", "--ui-test-existing"],
                profile,
            )
            != peer_id
        ):
            raise RuntimeError("legacy key changed Peer ID")

    def network(self, profile: "str") -> None:
        """Failed checks keep the draft but never activate it."""
        file = ["--ui-test-file"]
        peer_id = self.case("network-create", file, profile)
        self.case("network-check", [*file, "--ui-test-step=network"], profile)
        slot = (Path(profile) / "active-profile").read_text().strip()
        folder = Path(profile) / "profiles" / slot
        if not (folder / "network-draft").is_file():
            raise RuntimeError("the checked draft was not saved")
        if (folder / "network").exists():
            raise RuntimeError("a failed check activated the draft")
        if (
            self.case(
                "network-restart",
                [*file, "--ui-test-step=network-restart"],
                profile,
            )
            != peer_id
        ):
            raise RuntimeError("the network step replaced the identity")
        # Leaving the Network area keeps the draft and the failed rows.
        if (
            self.case(
                "network-settings-draft",
                [*file, "--ui-test-step=network-settings-failure"],
                profile,
            )
            != peer_id
        ):
            raise RuntimeError("the Settings check replaced the identity")
        # The kernel completes the handshake of a listener that never
        # accepts, so the dial waits until Cancel.
        with socket.socket() as silent:
            silent.bind(("127.0.0.1", 0))
            silent.listen()
            seed = "127.0.0.1:%d" % silent.getsockname()[1]
            self.case(
                "network-cancel",
                [*file, f"--ui-test-step=cancel:{seed}"],
                profile,
            )
        if (folder / "network").exists():
            raise RuntimeError("a failed check activated the draft")

    def profile_draft(self, profile: "str") -> None:
        """A nonempty public draft and its preview survive an app restart."""
        file = ["--ui-test-file"]
        peer_id = self.case("profile-create", file, profile)
        saved = self.case(
            "profile-save", [*file, "--ui-test-step=profile-save"], profile
        )
        if saved != peer_id:
            raise RuntimeError("saving a profile replaced the identity")
        # The app has exited; copy the whole slot into a fresh config.
        with tempfile.TemporaryDirectory(dir=self.reports) as restored:
            slot = (Path(profile) / "active-profile").read_text().strip()
            target = Path(restored) / "profiles"
            target.mkdir()
            shutil.copytree(Path(profile) / "profiles" / slot, target / slot)
            restored_peer = self.case(
                "profile-backup-restore",
                [*file, "--ui-test-step=profile-restart"],
                restored,
            )
            if restored_peer != peer_id:
                raise RuntimeError("restored file slot changed Peer ID")
        reopened = self.case(
            "profile-restart",
            [*file, "--ui-test-step=profile-restart"],
            profile,
        )
        if reopened != peer_id:
            raise RuntimeError("reopening a profile replaced the identity")

    @contextlib.contextmanager
    def fake_tray(self, mode, profile, script: str = "fake-tray.py"):
        """A session bus host (`script`), or a click driver for the Windows
        notification area (windows-tray.py), for one case."""
        (Path(profile) / "hidden").unlink(missing_ok=True)
        driver = (
            [sys.executable, "windows-tray.py"]
            if os.name == "nt"
            else ["/usr/bin/python3", script]
        )
        host = subprocess.Popen(  # pylint: disable=consider-using-with
            [
                driver[0],
                str(Path(__file__).with_name(driver[1])),
                mode,
                str(profile),
            ],
            stdout=subprocess.PIPE,
            text=True,
        )
        try:
            if host.stdout.readline().strip() != "ready":
                raise RuntimeError(f"fake tray host did not start: {mode}")
            yield
        finally:
            host.terminate()
            host.wait(timeout=10)

    def connect_identity(self, profile, file, seed, storage):
        peer_id = self.case("connect-create", file, profile)
        failed = self.case(
            "profile-publish-no-network",
            [*file, "--ui-test-step=profile-publish-fail"],
            profile,
            timeout=180,
        )
        if failed != peer_id:
            raise RuntimeError("failed publish replaced the identity")
        self.case(
            "search-no-network",
            [*file, "--ui-test-step=search-no-network"],
            profile,
        )
        self.case(
            "network-wrong", [*file, f"--ui-test-step=wrong:{seed}"], profile
        )
        if (
            storage
            / "profiles"
            / ((storage / "active-profile").read_text().strip())
            / "network"
        ).exists():
            raise RuntimeError("a wrong network was activated")
        # The relaunch reopens the failed draft and the retry connects.
        connected = self.case(
            "network-connect",
            [*file, f"--ui-test-step=connect:{seed}"],
            profile,
        )
        if connected != peer_id:
            raise RuntimeError("connecting replaced the identity")
        return peer_id

    def active_network(self, profile, peer_id, file, storage):
        # The next launch dials the active network on its own.
        self.case(
            "network-launch",
            [*file, "--ui-test-step=launch:connected"],
            profile,
        )
        slot = (storage / "active-profile").read_text().strip()
        if (
            self.case(
                "node", [*file, "--ui-test-step=node"], profile, timeout=180
            )
            != peer_id
        ):
            raise RuntimeError("the node step replaced the identity")
        folder = storage / "profiles" / slot
        saved = json.loads((folder / "node.json").read_text())
        if saved["enabled"] or not (folder / "node").is_dir():
            raise RuntimeError(f"the node did not run, then stop: {saved}")
        active = storage / "profiles" / slot / "network"
        active_before = active.read_bytes()
        settings_step = "--ui-test-step=network-settings-failure"
        settings_checked = self.case(
            "network-settings-failure", [*file, settings_step], profile
        )
        if settings_checked != peer_id:
            raise RuntimeError("failed Settings check changed identity")
        if active.read_bytes() != active_before:
            raise RuntimeError("Settings check changed active network")
        # A cancelled check keeps the previous network and its status.
        with socket.socket() as silent:
            silent.bind(("127.0.0.1", 0))
            silent.listen()
            quiet = "127.0.0.1:%d" % silent.getsockname()[1]
            self.case(
                "network-cancel-active",
                [*file, f"--ui-test-step=cancel:{quiet}"],
                profile,
            )
        if active.read_bytes() != active_before:
            raise RuntimeError("a cancelled check changed active network")
        return folder

    def public_profile(
        self, profile, peer_id, file, node, seed, env, storage
    ) -> None:
        published = self.case(
            "profile-publish-retry",
            [*file, "--ui-test-step=profile-publish-retry"],
            profile,
            timeout=180,
        )
        if published != peer_id:
            raise RuntimeError("publishing replaced the identity")
        visible_payload = check_profile_readback(
            node, seed, peer_id, env, storage
        )
        self.case(
            "search", [*file, "--ui-test-step=search"], profile, timeout=180
        )
        forge_search_signature(storage / "node")
        self.case(
            "search-forged",
            [*file, "--ui-test-step=search-forged"],
            profile,
            timeout=180,
        )
        self.case(
            "profile-hide",
            [*file, "--ui-test-step=profile-hide"],
            profile,
            timeout=120,
        )
        if (
            check_profile_readback(
                node, seed, peer_id, env, storage, hidden=True
            )
            != visible_payload
        ):
            raise RuntimeError("Hide changed the direct profile record")
        self.case(
            "profile-unhide",
            [*file, "--ui-test-step=profile-unhide"],
            profile,
            timeout=180,
        )
        check_profile_readback(node, seed, peer_id, env, storage)

    def peer_flows(self, profile, peer_id, file, node, seed, env, storage):
        # Keep the first identity's removal flow intact while both
        # published Peer IDs coexist on the loopback node.
        peer_root = Path(
            tempfile.mkdtemp(prefix="like-peer-", dir=self.reports)
        )
        other_peer = self.case("peer-create", file, peer_root)
        if other_peer == peer_id:
            raise RuntimeError("two client roots generated the same Peer ID")
        if (
            self.case(
                "peer-connect",
                [*file, f"--ui-test-step=connect:{seed}"],
                peer_root,
            )
            != other_peer
        ):
            raise RuntimeError(
                "connecting the second client changed its Peer ID"
            )
        # With a photo: this first record reaches search at once, while
        # a later edit waits out the search window.
        if (
            self.case(
                "peer-publish",
                [*file, "--ui-test-step=profile-publish-photo"],
                peer_root,
                timeout=180,
            )
            != other_peer
        ):
            raise RuntimeError(
                "publishing the second client changed its Peer ID"
            )
        check_profile_readback(node, seed, other_peer, env, peer_root)
        check_profile_readback(node, seed, peer_id, env, storage)
        # Two identities on one node. Every step relaunches its client,
        # so state it reads back survived a restart.
        alice, bob = (profile, other_peer), (peer_root, peer_id)
        for (root, peer), step in (
            (alice, "photo-seen"),
            (alice, "like"),
            (alice, "like-restart"),
            (bob, "liked-you"),
            (alice, "unlike"),
            (bob, "liked-gone"),
            (alice, "chat-first"),
            (bob, "chat-reply"),
            (alice, "chat-open"),
            (bob, "catch-up"),
            (bob, "chat-clear"),
            (alice, "sticker-send"),
            (bob, "sticker-request"),
            (alice, "sticker-answer"),
            (bob, "sticker-save-reply"),
            (alice, "sticker-reply-seen"),
            (alice, "share-profile-send"),
            (bob, "share-profile-open"),
            (alice, "presence-like"),
            (bob, "presence-pass"),
            (bob, "presence-match"),
            (alice, "presence-send"),
            (bob, "presence-shown"),
            (alice, "match-send"),
            (bob, "match-received"),
            *[(alice, "flood-send")] * 4,
            (bob, "flood-received"),
            (alice, "notice-send"),
            (bob, "notice-click"),
            (alice, "chat-block"),
            (bob, "chat-after-block"),
            (alice, "chat-blocked"),
        ):
            # The step fails unless the click brings the window back.
            with (
                self.fake_tray("click", root, "fake-notify.py")
                if step == "notice-click"
                else contextlib.nullcontext()
            ):
                self.case(
                    f"peers-{step}",
                    [*file, f"--ui-test-step=peer:{step}:{peer}"],
                    root,
                    timeout=180,
                )
            if step == "sticker-send":
                expire_sticker_media(storage / "node", peer_id)
        kept = Path(peer_root) / "notices.json"
        notice = json.loads(kept.read_text())[-1]
        body = "&lt;b&gt;notice&lt;/b&gt; text"
        if not notice["summary"] or (notice["body"], notice["actions"][0]) != (
            body,
            "default",
        ):
            raise RuntimeError(f"unexpected notice: {notice}")
        return peer_root, other_peer

    def connect(self, profile: "str") -> None:
        """A loopback dyappd answers the check, which activates the list."""
        node, storage = (
            Path(self.executable).with_name("dyappd"),
            Path(profile),
        )
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            seed = "127.0.0.1:%d" % probe.getsockname()[1]
        env = dict(
            os.environ,
            DYAPPD_LISTEN=f'["{seed}"]',
            DYAPPD_EXTERNAL=f'["{seed}"]',
            DYAPPD_STORAGE_DIR=str(storage / "node"),
            DYAPPD_ROLES='["store", "search", "media"]',
            DYAPPD_NETWORK_ID_POW_BITS="8",
            DYAPPD_SEARCH_MIN_PROFILE_POW_BITS="4",
        )
        subprocess.run(
            [node, "keygen"],
            env=env,
            check=True,
            stdout=subprocess.DEVNULL,
            timeout=600,
        )
        log = storage / "node.log"

        log.write_text("")
        server = start_smoke_node(node, env, seed, log)
        try:
            file = ["--ui-test-file"]
            peer_id = self.connect_identity(profile, file, seed, storage)
            folder = self.active_network(profile, peer_id, file, storage)
            self.public_profile(
                profile, peer_id, file, node, seed, env, storage
            )
            peer_root, other_peer = self.peer_flows(
                profile, peer_id, file, node, seed, env, storage
            )
            # A first run that connects on the wizard's network step.
            with tempfile.TemporaryDirectory(dir=self.reports) as fresh:
                self.case(
                    "wizard-connect",
                    [*file, f"--ui-test-step=wizard-connect:{seed}"],
                    fresh,
                )
                fresh = Path(fresh)
                picked = (fresh / "active-profile").read_text().strip()
                if not (fresh / "profiles" / picked / "network").is_file():
                    raise RuntimeError("the wizard did not activate a network")
            for language in ([], ["--pseudo-locale"]):
                self.linked(peer_root, other_peer, seed, [*file, *language])
            # Removal sends the tombstone first: with no node it keeps the
            # identity, with the node back it removes it.
            server.terminate()
            server.wait(timeout=30)
            if (
                self.case(
                    "remove-offline",
                    [*file, "--ui-test-step=remove-offline"],
                    profile,
                    timeout=120,
                )
                != peer_id
            ):
                raise RuntimeError("a failed withdrawal changed the identity")
            if not folder.is_dir():
                raise RuntimeError("a failed withdrawal removed the identity")
            server = start_smoke_node(node, env, seed, log)
            if (
                self.case(
                    "remove-tombstone",
                    [*file, "--ui-test-step=remove-tombstone"],
                    profile,
                    timeout=120,
                )
                == peer_id
                or folder.exists()
            ):
                raise RuntimeError("the removed identity is still there")
        except RuntimeError as error:
            raise RuntimeError(
                f"{error}\ndyappd log:\n{log.read_text()[-8192:]}"
            ) from error
        finally:
            server.terminate()
            server.wait(timeout=30)

    def tray(self, profile: "str") -> None:
        """Without a host the tray settings are off; with one, Quit exits,
        close-to-tray hides the window until Open, the setting survives a
        relaunch, and a host that goes away brings the window back. Windows
        always has a notification area, so it has no none or vanish case."""
        file = ["--ui-test-file"]
        if os.name == "posix":
            self.case(
                "tray-none",
                [*file, "--ui-test-step=tray:none"],
                profile,
                peer=False,
            )
        with self.fake_tray("quit", profile):
            self.case(
                "tray-quit",
                [*file, "--ui-test-step=tray:quit"],
                profile,
                peer=False,
            )
        with self.fake_tray("open", profile):
            self.case(
                "tray-close",
                [*file, "--ui-test-step=tray:close"],
                profile,
                peer=False,
            )
        saved = json.loads((Path(profile) / "tray.json").read_text())
        if saved != {"show": True, "closeToTray": True}:
            raise RuntimeError(f"tray settings not saved: {saved}")
        with self.fake_tray("open", profile):
            self.case(
                "tray-relaunch",
                [*file, "--ui-test-step=tray:persisted"],
                profile,
                peer=False,
            )
        if os.name == "posix":
            with self.fake_tray("vanish", profile):
                self.case(
                    "tray-vanish",
                    [*file, "--ui-test-step=tray:vanish"],
                    profile,
                    peer=False,
                )

    def storage_modes(self, flows) -> None:
        for mode in ("system", "file"):
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                flags = ["--ui-test-file"] if mode == "file" else []
                peer_id = self.case(f"{mode}-create", flags, profile)
                reopened = self.case(
                    f"{mode}-restart", [*flags, "--ui-test-existing"], profile
                )
                if reopened != peer_id:
                    raise RuntimeError(
                        f"{mode} storage changed Peer ID after restart"
                    )
                if flows:
                    self.sticker_packs(profile, mode, flags, peer_id)
                    if mode == "system" and sys.platform.startswith("linux"):
                        self.system_files_only(profile)
                    (self.legacy if mode == "file" else self.mixed)(
                        profile, peer_id
                    )

    def network_flows(self, flows) -> None:
        if flows:
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                self.network(profile)
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                self.profile_draft(profile)
        # linux-test.sh builds dyappd beside the app; Windows and the installed
        # client packages have none.
        if flows and Path(self.executable).with_name("dyappd").exists():
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                self.connect(profile)
        elif flows:
            # The no-network start of connect(), which needs no node.
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                peer_id = self.case(
                    "offline-create", ["--ui-test-file"], profile
                )
                if (
                    self.case(
                        "profile-publish-no-network",
                        [
                            "--ui-test-file",
                            "--ui-test-step=profile-publish-fail",
                        ],
                        profile,
                        timeout=180,
                    )
                    != peer_id
                ):
                    raise RuntimeError("failed publish replaced the identity")
                self.case(
                    "search-no-network",
                    ["--ui-test-file", "--ui-test-step=search-no-network"],
                    profile,
                )

    def exercise(self, flows: "bool") -> None:
        video_enabled = os.environ.get("DYAPP_VIDEO_SMOKE") == "1"
        if flows and sys.platform.startswith("linux") and video_enabled:
            video_smoke(self.case, self.reports)
        self.storage_modes(flows)
        self.network_flows(flows)
        language_step = "--ui-test-step=settings-language"
        with tempfile.TemporaryDirectory(dir=self.reports) as profile:
            self.case(
                "settings-english", ["--ui-test-file", language_step], profile
            )
            self.case(
                "settings-privacy-restart",
                ["--ui-test-file", "--ui-test-step=settings-privacy-restart"],
                profile,
            )

        with tempfile.TemporaryDirectory(dir=self.reports) as profile:
            russian = ["--ui-test-file", "--ui-test-ru", language_step]
            self.case("russian-locale", russian, profile)
        with tempfile.TemporaryDirectory(dir=self.reports) as profile:
            pseudo = ["--ui-test-file", "--pseudo-locale", language_step]
            self.case("pseudo-locale", pseudo, profile)
        with tempfile.TemporaryDirectory(dir=self.reports) as profile:
            self.case(
                "hidden-ready-rejected",
                ["--ui-test-file", "--break-ui"],
                profile,
                broken=True,
            )
        if flows:
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                self.window(profile)
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                self.single(profile)
        if flows:
            with tempfile.TemporaryDirectory(dir=self.reports) as profile:
                self.tray(profile)
        (self.reports / "desktop-ui-results.json").write_text(
            json.dumps(self.results, indent=2)
        )
        print(f"{len(self.results)} desktop UI cases passed")


def smoke(executable: "str", flows: bool = True) -> None:
    """Only fixture apps skip stateful identity/network flows."""
    DesktopSmoke(executable).exercise(flows)


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("desktop-smoke")
    try:
        smoke(sys.argv[1], os.environ.get("DYAPP_SMOKE_FIXTURE") != "1")
    except (RuntimeError, OSError) as error:
        if os.environ.get("DYAPP_XVFB_DIAGNOSTICS") == "1":
            raise RuntimeError(f"{error}\n{display_diagnostics()}") from None
        raise
