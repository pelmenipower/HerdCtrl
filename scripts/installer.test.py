#!/usr/bin/env python3
"""Offline tests for install.sh and update.sh.

Every external program the installer calls is a stub on PATH; nothing touches the
network, the real HOME, or the real desktop. Run: python3 scripts/installer.test.py
"""
import fcntl
import hashlib
import io
import os
import pwd
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INSTALL = ROOT / "install.sh"
UPDATE = ROOT / "update.sh"
DESKTOP = ROOT / "assets/HerdCtrl-Iconset/linux/herdctrl.desktop"
EXT_ZIP = "herdctrl@herdctrl.local.shell-extension.zip"
TAG_URL = "https://github.com/pelmenipower/HerdCtrl/releases/tag/v{}"

CURL = r"""#!/usr/bin/env bash
echo "curl $*" >> "$STUB_LOG/curl.log"
out='' fmt='' url=''
while (($#)); do
  case $1 in
    -o) out=$2; shift ;;
    -w) fmt=$2; shift ;;
    --max-time|--connect-timeout|--proto|--proto-redir) shift ;;
    https://*) url=$1 ;;
  esac
  shift
done
if [[ $fmt == *redirect_url* ]]; then printf '%s' "${STUB_REDIRECT:-}"; exit 0; fi
if [[ $url =~ /releases/download/(v[0-9.]+)/(.+)$ ]]; then
  src="$STUB_FIXTURES/${BASH_REMATCH[1]}/${BASH_REMATCH[2]}"
  if [[ -f $src ]]; then cp -- "$src" "$out"; exit 0; fi
fi
echo "curl: (22) not found: $url" >&2
exit 22
"""

STUBS = {
    "curl": CURL,
    "cosign": '#!/usr/bin/env bash\necho "cosign $*" >> "$STUB_LOG/cosign.log"\nexit "${STUB_COSIGN_EXIT:-0}"\n',
    "sudo": '#!/usr/bin/env bash\necho "sudo $*" >> "$STUB_LOG/sudo.log"\nexec "$@"\n',
    "apt-get": '#!/usr/bin/env bash\necho "apt-get $*" >> "$STUB_LOG/apt.log"\n',
    "dpkg-query": '#!/usr/bin/env bash\nfor a; do p=$a; done\nif [[ $p == libvulkan1 ]]; then exit 1; fi\nprintf "install ok installed"\n',
    "gnome-shell": "#!/usr/bin/env bash\necho 'GNOME Shell 50.0'\n",
    "update-desktop-database": '#!/usr/bin/env bash\necho "$0 $*" >> "$STUB_LOG/caches.log"\n',
    "gtk-update-icon-cache": '#!/usr/bin/env bash\necho "$0 $*" >> "$STUB_LOG/caches.log"\n',
    "gnome-extensions": r"""#!/usr/bin/env bash
echo "gnome-extensions $*" >> "$STUB_LOG/gext.log"
dir="$XDG_DATA_HOME/gnome-shell/extensions/herdctrl@herdctrl.local"
case $1 in
  install) mkdir -p "$dir"; sha256sum < "${@: -1}" > "$dir/metadata.json" ;;
  uninstall) rm -rf "$dir" ;;
esac
""",
    # Records the call and starts a stand-in tray so the installer sees it running.
    "setsid": r"""#!/usr/bin/env bash
echo "setsid $*" >> "$STUB_LOG/setsid.log"
echo "handoff=${HERDCTRL_INSTALL_HANDOFF-unset}" >> "$STUB_LOG/setsid.env"
if [[ -n ${STUB_IDLE:-} ]]; then
  ( exec -a herdctrl "$STUB_IDLE" --tray </dev/null >/dev/null 2>&1 & echo $! >> "$STUB_LOG/trays.pids" )
fi
""",
}


_IDLE_DIR = None


def idle_program():
    """Path of a tiny program that sleeps and ignores its arguments (sleep(1) rejects --tray), or None."""
    global _IDLE_DIR
    if _IDLE_DIR is None:
        _IDLE_DIR = Path(tempfile.mkdtemp(prefix="herdctrl-idle-"))
        source = _IDLE_DIR / "idle.c"
        source.write_text("#include <unistd.h>\nint main(void) { for (;;) pause(); }\n")
        cc = shutil.which("cc") or shutil.which("gcc")
        if cc is None or subprocess.run([cc, "-o", str(_IDLE_DIR / "idle"), str(source)]).returncode != 0:
            return None
    path = _IDLE_DIR / "idle"
    return str(path) if path.exists() else None


def tearDownModule():
    if _IDLE_DIR is not None:
        shutil.rmtree(_IDLE_DIR, ignore_errors=True)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fixture_herdctrl(version: str, output: str = None) -> bytes:
    out = output if output is not None else f"herdctrl {version}"
    return f'#!/bin/sh\nif [ "$1" = --version ]; then echo "{out}"; fi\nexit 0\n'.encode()


def make_tarball(version, *, install_sh=None, herdctrl=None, dirname=None) -> bytes:
    name = dirname or f"herdctrl-v{version}-linux-x86_64"
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        def add(path, data, mode=0o644):
            info = tarfile.TarInfo(f"{name}/{path}")
            info.size = len(data)
            info.mode = mode
            tar.addfile(info, io.BytesIO(data))

        add("herdctrl", herdctrl or fixture_herdctrl(version), 0o755)
        add("desktop/herdctrl.desktop", DESKTOP.read_bytes())
        add("desktop/hicolor/16x16/apps/herdctrl.png", b"png16")
        add("desktop/hicolor/48x48/apps/herdctrl.png", b"png48")
        if install_sh is not None:
            add("install.sh", install_sh, 0o755)
    return buf.getvalue()


class InstallerCase(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="herdctrl-installer-")).resolve()
        real_home = Path(pwd.getpwuid(os.getuid()).pw_dir).resolve()
        if real_home in (self.tmp, *self.tmp.parents):
            self.fail("test directories must not live under the real HOME")
        self.home = self.tmp / "home dir"
        self.config = self.tmp / "config"
        self.data = self.tmp / "data"
        self.bin = self.tmp / "bin dir"
        self.runtime = self.tmp / "run"
        self.stubs = self.tmp / "stubs"
        self.fixtures = self.tmp / "fixtures"
        self.log = self.tmp / "log"
        for d in (self.home, self.config, self.data, self.runtime, self.stubs, self.fixtures, self.log):
            d.mkdir()
        os.chmod(self.runtime, 0o700)
        for name, body in STUBS.items():
            if name == "cosign":
                continue  # only present in the cosign cases
            self.write_stub(name, body)
        self.procs = []
        self.idle = idle_program()
        self.install_sh = INSTALL.read_bytes()

    def tearDown(self):
        for proc in self.procs:
            if proc.poll() is None:
                proc.kill()
            proc.wait()
        pids = self.log / "trays.pids"
        if pids.exists():
            for line in pids.read_text().split():
                try:
                    os.kill(int(line), signal.SIGKILL)
                except ProcessLookupError:
                    pass
        shutil.rmtree(self.tmp, ignore_errors=True)

    def write_stub(self, name, body):
        path = self.stubs / name
        path.write_text(body)
        path.chmod(0o755)

    def env(self, **extra):
        env = {
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.config),
            "XDG_DATA_HOME": str(self.data),
            "XDG_BIN_HOME": str(self.bin),
            "XDG_RUNTIME_DIR": str(self.runtime),
            "PATH": f"{self.stubs}:/usr/bin:/bin",
            "DISPLAY": ":0",
            "STUB_LOG": str(self.log),
            "STUB_FIXTURES": str(self.fixtures),
            "TMPDIR": str(self.tmp),
        }
        if self.idle:
            env["STUB_IDLE"] = self.idle
        env.update(extra)
        return env

    def release(self, version, *, tarball=None, install_sh=None, bad_sum=False, **tar_args):
        """Publish fixtures for a fake release and return the tarball bytes."""
        d = self.fixtures / f"v{version}"
        d.mkdir(exist_ok=True)
        tar_name = f"herdctrl-v{version}-linux-x86_64.tar.gz"
        install_sh = self.install_sh if install_sh is None else install_sh
        data = tarball or make_tarball(version, install_sh=self.install_sh, **tar_args)
        zip_data = b"zip " + version.encode()
        files = {tar_name: data, EXT_ZIP: zip_data, "install.sh": install_sh,
                 "update.sh": UPDATE.read_bytes(), "SHA256SUMS.sigstore.json": b"{}"}
        sums = ""
        for name, content in files.items():
            (d / name).write_bytes(content)
            digest = "0" * 64 if bad_sum and name == tar_name else sha256(content)
            sums += f"{digest}  {name}\n"
        (d / "SHA256SUMS").write_text(sums)
        return data

    def run_script(self, script, *args, redirect=None, env=None, timeout=60):
        environ = self.env(**(env or {}))
        if redirect:
            environ["STUB_REDIRECT"] = TAG_URL.format(redirect)
        return subprocess.run(["bash", str(script), *args], env=environ, cwd=self.tmp,
                              stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=timeout)

    def run_install(self, *args, **kw):
        return self.run_script(INSTALL, *args, **kw)

    def read_log(self, name):
        path = self.log / name
        return path.read_text() if path.exists() else ""

    def stamp(self):
        path = self.data / "herdctrl/installed-version"
        return path.read_text() if path.exists() else None

    def write_stamp(self, version):
        path = self.data / "herdctrl"
        path.mkdir(parents=True, exist_ok=True)
        (path / "installed-version").write_text(f"{version}\n")

    def spawn_herdctrl(self, *args, config=None):
        """Start `exec -a herdctrl <idle> <args>`; returns once the rename took effect."""
        if not self.idle:
            self.skipTest("no C compiler for the stand-in process")
        env = self.env(XDG_CONFIG_HOME=str(config or self.config))
        proc = subprocess.Popen(["bash", "-c", 'exec -a herdctrl "$@"', "x", self.idle, *args], env=env,
                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.procs.append(proc)
        for _ in range(100):
            try:
                if Path(f"/proc/{proc.pid}/cmdline").read_bytes().startswith(b"herdctrl\0"):
                    return proc
            except OSError:
                pass
            time.sleep(0.05)
        self.fail("stand-in process did not start")

    def assert_nothing_installed(self):
        self.assertFalse((self.bin / "herdctrl").exists())
        self.assertIsNone(self.stamp())


class TestInstall(InstallerCase):
    def test_fresh_install(self):
        self.release("0.2.0")
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        exe = self.bin / "herdctrl"
        self.assertTrue(os.access(exe, os.X_OK))
        self.assertTrue(os.access(self.bin / "herdctrl-update", os.X_OK))
        entry = (self.data / "applications/herdctrl.desktop").read_text()
        self.assertIn(f'Exec="{exe}"\n', entry)
        self.assertIn(f"TryExec={exe}\n", entry)
        self.assertEqual(entry.count("Exec="), 2)  # Exec and TryExec only
        self.assertIn("Name=HerdCtrl", entry)
        self.assertTrue((self.data / "icons/hicolor/16x16/apps/herdctrl.png").exists())
        self.assertEqual(self.stamp(), "0.2.0\n")
        apt = self.read_log("apt.log")
        self.assertIn("-y -- libvulkan1", apt)
        self.assertNotIn("libgl1", apt)
        self.assertIn("update-desktop-database", self.read_log("caches.log"))
        self.assertIn("Start HerdCtrl from the app menu.", r.stdout)
        self.assertIn(f"{self.bin} is not in your PATH yet", r.stdout)
        if not shutil.which("cosign", path="/usr/bin:/bin"):
            self.assertIn("cosign not found: checksum only (see docs/manual.md#verify-a-download)", r.stdout)
        curl = self.read_log("curl.log").splitlines()
        self.assertNotIn("--location", curl[0])
        self.assertIn("--location", curl[1])

    def test_no_packages(self):
        self.release("0.2.0")
        r = self.run_install("--no-packages", redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.read_log("apt.log"), "")

    def test_failed_sudo_continues(self):
        self.release("0.2.0")
        self.write_stub("sudo", "#!/usr/bin/env bash\nexit 1\n")
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Could not install packages: libvulkan1.", r.stderr)

    def test_same_version_does_nothing(self):
        self.release("0.2.0")
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        (self.log / "curl.log").unlink()
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("HerdCtrl 0.2.0 is already installed. Nothing to do.", r.stdout)
        self.assertEqual(len(self.read_log("curl.log").splitlines()), 1)  # only the version lookup

    def test_reinstall_runs_again(self):
        self.release("0.2.0")
        self.run_install(redirect="0.2.0")
        r = self.run_install("--reinstall", redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("Nothing to do", r.stdout)

    def test_update_repoints_autostart(self):
        self.release("0.2.0")
        self.release("0.3.0")
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        autostart = self.config / "autostart"
        autostart.mkdir()
        entry = autostart / "herdctrl.desktop"
        entry.write_text('[Desktop Entry]\nType=Application\nName=HerdCtrl\nExec="/src/target/release/herdctrl" --tray\nNoDisplay=true\n')
        r = self.run_install(redirect="0.3.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.stamp(), "0.3.0\n")
        self.assertEqual(entry.read_text(), f'[Desktop Entry]\nType=Application\nName=HerdCtrl\nExec="{self.bin}/herdctrl" --tray\nNoDisplay=true\n')
        self.assertIn(f"Start at login now uses {self.bin}/herdctrl.", r.stdout)

    def test_autostart_of_other_program_is_left_alone(self):
        self.release("0.2.0")
        autostart = self.config / "autostart"
        autostart.mkdir()
        entry = autostart / "herdctrl.desktop"
        text = "[Desktop Entry]\nExec=other-tool --tray\n"
        entry.write_text(text)
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        self.assertEqual(entry.read_text(), text)

    def test_autostart_with_wrapper_command_is_left_alone(self):
        self.release("0.2.0")
        autostart = self.config / "autostart"
        autostart.mkdir()
        entry = autostart / "herdctrl.desktop"
        text = "[Desktop Entry]\nExec=env FOO=1 /opt/x/herdctrl --tray\n"
        entry.write_text(text)
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        self.assertEqual(entry.read_text(), text)

    def test_autostart_is_never_created(self):
        self.release("0.2.0")
        self.run_install(redirect="0.2.0")
        self.assertFalse((self.config / "autostart").exists())

    def test_explicit_version(self):
        self.release("0.2.5")
        r = self.run_install("--version", "v0.2.5")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.stamp(), "0.2.5\n")

    def test_archive_with_installer(self):
        data = make_tarball("0.1.0", install_sh=self.install_sh)
        archive = self.tmp / "herdctrl-v0.1.0-linux-x86_64.tar.gz"
        archive.write_bytes(data)
        r = self.run_install("--archive", str(archive))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.stamp(), "0.1.0\n")
        self.assertEqual(self.read_log("curl.log"), "")

    def test_archive_without_installer_is_rejected(self):
        archive = self.tmp / "old.tar.gz"
        archive.write_bytes(make_tarball("0.1.0"))
        r = self.run_install("--archive", str(archive))
        self.assertEqual(r.returncode, 1)
        self.assertIn("this archive predates the installer (no install.sh inside).", r.stderr)
        self.assert_nothing_installed()

    def test_archive_skips_extension(self):
        archive = self.tmp / "a.tar.gz"
        archive.write_bytes(make_tarball("0.1.0", install_sh=self.install_sh))
        r = self.run_install("--archive", str(archive), "--extension")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("The GNOME extension is not part of the archive", r.stdout)
        self.assertEqual(self.read_log("gext.log"), "")

    def test_wrong_directory_name_in_download(self):
        self.release("0.2.0", dirname="herdctrl-v0.2.1-linux-x86_64")
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 1)
        self.assert_nothing_installed()

    def test_checksum_mismatch(self):
        self.release("0.2.0", bad_sum=True)
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 1)
        self.assertIn("HerdCtrl: checksum mismatch for herdctrl-v0.2.0-linux-x86_64.tar.gz. Nothing was installed.", r.stderr)
        self.assert_nothing_installed()

    def test_cosign_verifies(self):
        self.release("0.2.0")
        self.write_stub("cosign", STUBS["cosign"])
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Signature verified (Sigstore)", r.stdout)
        log = self.read_log("cosign.log")
        self.assertIn("verify-blob", log)
        self.assertIn("release.yml@refs/tags/v0.2.0", log)

    def test_cosign_failure_installs_nothing(self):
        self.release("0.2.0")
        self.write_stub("cosign", STUBS["cosign"])
        r = self.run_install(redirect="0.2.0", env={"STUB_COSIGN_EXIT": "1"})
        self.assertEqual(r.returncode, 1)
        self.assertIn("the signature of SHA256SUMS does not verify. Nothing was installed.", r.stderr)
        self.assert_nothing_installed()

    def test_smoke_test_failure_keeps_old_install(self):
        self.release("0.2.0")
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        old = (self.bin / "herdctrl").read_bytes()
        self.release("0.3.0", herdctrl=fixture_herdctrl("0.3.0", output="oops"))
        r = self.run_install(redirect="0.3.0")
        self.assertEqual(r.returncode, 1)
        self.assertIn('the new program does not start (expected "herdctrl 0.3.0")', r.stderr)
        self.assertEqual((self.bin / "herdctrl").read_bytes(), old)
        self.assertEqual(self.stamp(), "0.2.0\n")

    def test_bin_with_dollar_is_rejected(self):
        self.release("0.2.0")
        r = self.run_install(redirect="0.2.0", env={"XDG_BIN_HOME": str(self.tmp / "b$in")})
        self.assertEqual(r.returncode, 1)
        self.assertIn("contains characters a desktop entry cannot hold", r.stderr)

    def test_relative_xdg_value_is_ignored(self):
        self.release("0.2.0")
        r = self.run_install(redirect="0.2.0", env={"XDG_BIN_HOME": "relative/bin"})
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue((self.home / ".local/bin/herdctrl").exists())

    def test_second_installer_is_refused(self):
        self.release("0.2.0")
        lock = self.runtime / f"herdctrl-install-{os.getuid()}.lock"
        with open(lock, "w") as handle:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 1)
        self.assertIn("Another HerdCtrl installer is running.", r.stderr)
        self.assert_nothing_installed()

    def test_handoff_to_newer_installer(self):
        recorder = b'#!/usr/bin/env bash\necho "handoff $HERDCTRL_INSTALL_HANDOFF $*" > "$STUB_LOG/handoff.log"\nexit 7\n'
        self.release("0.3.0", install_sh=recorder)
        r = self.run_install("--no-packages", redirect="0.3.0")
        self.assertEqual(r.returncode, 7, r.stderr)
        self.assertEqual(self.read_log("handoff.log"), "handoff 1 --no-packages\n")
        self.assertIn("Handing over to the installer of HerdCtrl 0.3.0", r.stdout)
        self.assertNotIn("tar.gz", self.read_log("curl.log"))
        self.assert_nothing_installed()

    def test_restarted_tray_does_not_inherit_handoff(self):
        self.release("0.3.0", install_sh=self.install_sh + b"\n# newer\n")
        self.spawn_herdctrl("--tray")
        r = self.run_install("--no-packages", redirect="0.3.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Handing over to the installer of HerdCtrl 0.3.0", r.stdout)
        self.assertEqual(self.stamp(), "0.3.0\n")
        self.assertEqual(self.read_log("setsid.env"), "handoff=unset\n")

    def test_empty_option_values_are_usage_errors(self):
        for args in (["--version", ""], ["--archive", ""]):
            r = self.run_install(*args)
            self.assertEqual(r.returncode, 2, args)
            self.assertIn("needs a value", r.stderr)

    def test_failure_after_tray_stop_restarts_tray(self):
        self.release("0.2.0")
        self.release("0.3.0")
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        self.spawn_herdctrl("--tray")
        (self.log / "setsid.log").unlink(missing_ok=True)
        self.bin.chmod(0o555)  # replacing the program now fails
        try:
            r = self.run_install(redirect="0.3.0")
        finally:
            self.bin.chmod(0o755)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("restarting the tray", r.stderr)
        self.assertIn(f"setsid -f {self.bin}/herdctrl --tray", self.read_log("setsid.log"))
        self.assertEqual(self.stamp(), "0.2.0\n")

    def test_piped_script_does_not_hand_over(self):
        self.release("0.3.0", install_sh=b"#!/usr/bin/env bash\nexit 7\n")
        r = subprocess.run(["bash"], input=INSTALL.read_text(), env=dict(self.env(), STUB_REDIRECT=TAG_URL.format("0.3.0")),
                           cwd=self.tmp, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.stamp(), "0.3.0\n")


class TestTray(InstallerCase):
    def test_tray_is_stopped_and_restarted(self):
        self.release("0.2.0")
        tray = self.spawn_herdctrl("--tray")
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNotNone(tray.poll(), "the tray with the same configuration must be stopped")
        self.assertIn(f"setsid -f {self.bin}/herdctrl --tray", self.read_log("setsid.log"))
        self.assertNotIn("did not start", r.stderr)

    def test_tray_with_other_configuration_survives(self):
        self.release("0.2.0")
        other = self.tmp / "other-config"
        other.mkdir()
        tray = self.spawn_herdctrl("--tray", config=other)
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNone(tray.poll())
        self.assertEqual(self.read_log("setsid.log"), "")
        self.assertIn("Start HerdCtrl from the app menu.", r.stdout)

    def test_same_effective_configuration_counts(self):
        self.release("0.2.0")
        # An unset XDG_CONFIG_HOME resolves to $HOME/.config, which is what the installer sees here.
        default = self.home / ".config"
        default.mkdir()
        tray = self.spawn_herdctrl("--tray", config=default)
        r = self.run_install(redirect="0.2.0", env={"XDG_CONFIG_HOME": "relative"})
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNotNone(tray.poll())

    def test_editor_is_left_running(self):
        self.release("0.2.0")
        editor = self.spawn_herdctrl()
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNone(editor.poll())
        self.assertIn("The editor is open; close and reopen it to use 0.2.0.", r.stdout)

    def test_other_arguments_are_ignored(self):
        self.release("0.2.0")
        proc = self.spawn_herdctrl("--identify-monitors")
        r = self.run_install(redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNone(proc.poll())

    def test_no_session_leaves_tray_running(self):
        self.release("0.2.0")
        tray = self.spawn_herdctrl("--tray")
        environ = self.env()
        del environ["DISPLAY"]
        r = subprocess.run(["bash", str(INSTALL)], env=dict(environ, STUB_REDIRECT=TAG_URL.format("0.2.0")), cwd=self.tmp,
                           stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNone(tray.poll())
        self.assertIn("The tray keeps running the previous version until you quit and restart it.", r.stdout)
        self.assertNotIn("Start HerdCtrl from the app menu.", r.stdout)


class TestCheck(InstallerCase):
    def test_check_variants(self):
        r = self.run_install("--check", redirect="0.2.0")
        self.assertEqual(r.returncode, 10)
        self.assertIn("Installed: unknown (installed by hand or before 0.2.0) · Latest: 0.2.0 → Run the installer to update.", r.stdout)
        self.write_stamp("0.1.9")
        r = self.run_install("--check", redirect="0.2.0")
        self.assertEqual(r.returncode, 10)
        self.assertIn("Installed: 0.1.9 · Latest: 0.2.0 → Update available. Run: herdctrl-update", r.stdout)
        self.write_stamp("0.2.0")
        r = self.run_install("--check", redirect="0.2.0")
        self.assertEqual(r.returncode, 0)
        self.assertIn("HerdCtrl 0.2.0 is up to date", r.stdout)
        self.write_stamp("0.10.0")
        self.assertEqual(self.run_install("--check", redirect="0.9.0").returncode, 0)
        self.assertEqual(len(self.read_log("curl.log").splitlines()), 4)  # lookups only

    def test_bad_redirect(self):
        r = self.run_install("--check")
        self.assertEqual(r.returncode, 1)
        self.assertIn("could not determine the latest release (GitHub answered: no redirect).", r.stderr)

    def test_check_does_not_lock_or_install(self):
        r = self.run_install("--check", redirect="0.2.0")
        self.assertFalse(list(self.runtime.iterdir()))
        self.assertEqual(r.returncode, 10)


class TestUsage(InstallerCase):
    def assert_usage_error(self, *args):
        r = self.run_install(*args)
        self.assertEqual(r.returncode, 2, (args, r.stderr))
        self.assertIn("Run with --help for usage.", r.stderr)

    def test_usage_errors(self):
        for args in (["--bogus"], ["--version"], ["--archive"], ["--version", "1.2"], ["--version", "1.2.3", "--archive", "x"],
                     ["--check", "--archive", "x"], ["--check", "--reinstall"], ["--check", "--uninstall"],
                     ["--purge"], ["--uninstall", "--reinstall"], ["--uninstall", "--version", "0.2.0"],
                     ["--extension", "--no-extension"]):
            with self.subTest(args=args):
                self.assert_usage_error(*args)

    def test_old_version_is_unsupported(self):
        r = self.run_install("--version", "0.1.0")
        self.assertEqual(r.returncode, 1)
        self.assertIn("HerdCtrl 0.1.0 predates the installer; install 0.2.0 or newer.", r.stderr + r.stdout)

    def test_help(self):
        r = self.run_install("--help")
        self.assertEqual(r.returncode, 0)
        self.assertIn("--uninstall", r.stdout)


class TestUninstall(InstallerCase):
    def setUp(self):
        super().setUp()
        self.release("0.2.0")
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        layouts = self.config / "herdctrl"
        layouts.mkdir(parents=True)
        (layouts / "layouts.json").write_text("{}")

    def test_uninstall_keeps_configuration(self):
        autostart = self.config / "autostart"
        autostart.mkdir()
        (autostart / "herdctrl.desktop").write_text(f'[Desktop Entry]\nExec="{self.bin}/herdctrl" --tray\n')
        r = self.run_install("--uninstall")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse((self.bin / "herdctrl").exists())
        self.assertFalse((self.bin / "herdctrl-update").exists())
        self.assertFalse((self.data / "applications/herdctrl.desktop").exists())
        self.assertFalse((self.data / "icons/hicolor/16x16/apps/herdctrl.png").exists())
        self.assertIsNone(self.stamp())
        self.assertFalse((autostart / "herdctrl.desktop").exists())
        self.assertTrue((self.config / "herdctrl/layouts.json").exists())

    def test_purge_removes_configuration(self):
        r = self.run_install("--uninstall", "--purge")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse((self.config / "herdctrl").exists())

    def test_foreign_autostart_is_kept(self):
        autostart = self.config / "autostart"
        autostart.mkdir()
        entry = autostart / "herdctrl.desktop"
        entry.write_text('[Desktop Entry]\nExec="/src/target/release/herdctrl" --tray\n')
        r = self.run_install("--uninstall")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(entry.exists())
        self.assertIn("alone: it starts another HerdCtrl.", r.stdout)

    def test_uninstall_stops_tray(self):
        tray = self.spawn_herdctrl("--tray")
        r = self.run_install("--uninstall")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIsNotNone(tray.poll())


class TestExtension(InstallerCase):
    GNOME = {"XDG_CURRENT_DESKTOP": "ubuntu:GNOME", "XDG_SESSION_TYPE": "wayland", "WAYLAND_DISPLAY": "wayland-0"}

    def test_extension_installed_then_up_to_date(self):
        self.release("0.2.0")
        r = self.run_install(redirect="0.2.0", env=self.GNOME)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("gnome-extensions install --force", self.read_log("gext.log"))
        self.assertIn("Log out and back in to load the new extension.", r.stdout)
        r = self.run_install("--reinstall", redirect="0.2.0", env=self.GNOME)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Extension is up to date.", r.stdout)

    def test_no_extension_outside_gnome(self):
        self.release("0.2.0")
        self.assertEqual(self.run_install(redirect="0.2.0").returncode, 0)
        self.assertEqual(self.read_log("gext.log"), "")
        self.assertNotIn(EXT_ZIP, self.read_log("curl.log"))

    def test_flag_forces_extension(self):
        self.release("0.2.0")
        self.assertEqual(self.run_install("--extension", redirect="0.2.0").returncode, 0)
        self.assertIn("install --force", self.read_log("gext.log"))

    def test_missing_extension_hint_for_current_version(self):
        self.release("0.2.0")
        self.write_stamp("0.2.0")
        r = self.run_install(redirect="0.2.0", env=self.GNOME)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("The GNOME extension is missing. Run: herdctrl-update --extension", r.stdout)
        self.assertIn("Nothing to do.", r.stdout)

    def test_extension_flag_on_current_version_installs_it(self):
        self.release("0.2.0")
        self.write_stamp("0.2.0")
        tray = self.spawn_herdctrl("--tray")
        r = self.run_install("--extension", redirect="0.2.0")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("gnome-extensions install --force", self.read_log("gext.log"))
        self.assertIn("Log out and back in to load the new extension.", r.stdout)
        self.assertNotIn("Nothing to do", r.stdout)
        self.assertIsNone(tray.poll(), "the tray must not be stopped")
        self.assertEqual(self.read_log("setsid.log"), "")
        self.assertFalse((self.bin / "herdctrl").exists())
        self.assertNotIn("tar.gz", self.read_log("curl.log"))

    def test_uninstall_removes_extension(self):
        self.release("0.2.0")
        self.run_install(redirect="0.2.0", env=self.GNOME)
        r = self.run_install("--uninstall", env=self.GNOME)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("gnome-extensions uninstall herdctrl@herdctrl.local", self.read_log("gext.log"))


class TestUpdateWrapper(InstallerCase):
    def test_delegates_to_installed_updater_and_keeps_exit_code(self):
        self.bin.mkdir()
        updater = self.bin / "herdctrl-update"
        updater.write_text('#!/usr/bin/env bash\necho "updater $*"\nexit 10\n')
        updater.chmod(0o755)
        r = self.run_script(UPDATE, "--check")
        self.assertEqual(r.returncode, 10)
        self.assertIn("updater --check", r.stdout)
        self.assertEqual(self.read_log("curl.log"), "")

    def test_downloads_verified_installer_when_not_installed(self):
        self.release("0.2.0", install_sh=b'#!/usr/bin/env bash\necho "installer $*"\nexit 10\n')
        r = self.run_script(UPDATE, "--check", redirect="0.2.0")
        self.assertEqual(r.returncode, 10, r.stderr)
        self.assertIn("installer --check", r.stdout)
        self.assertFalse(list(self.tmp.glob("herdctrl-update.*")), "temp directory must be cleaned up")

    def test_rejects_installer_with_wrong_checksum(self):
        self.release("0.2.0")
        d = self.fixtures / "v0.2.0"
        (d / "install.sh").write_text("#!/usr/bin/env bash\necho tampered\n")
        r = self.run_script(UPDATE, redirect="0.2.0")
        self.assertEqual(r.returncode, 1)
        self.assertIn("checksum mismatch for install.sh", r.stderr)
        self.assertNotIn("tampered", r.stdout)


class TestScripts(unittest.TestCase):
    def test_syntax_and_layout(self):
        for script in (INSTALL, UPDATE):
            subprocess.run(["bash", "-n", str(script)], check=True)
            self.assertTrue(script.stat().st_mode & stat.S_IXUSR)
            self.assertTrue(script.read_text().rstrip().endswith('main "$@"; exit'))


if __name__ == "__main__":
    unittest.main(verbosity=1)
