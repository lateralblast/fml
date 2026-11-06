"""The ./mau wrapper: it brings the Rust build up to date before it runs mau-cli (or --gui's app).

A stand-in `cargo` is put first on PATH. It records each call, writes a stand-in binary when
there is none (or when the test says the sources changed), and fails when told to. The binaries
it writes are shell scripts that print their name, version and arguments, so the tests can see
which build ran. Nothing is built and no real media is touched.
"""

import os
import pathlib
import shutil
import stat
import subprocess

import pytest

ROOT = pathlib.Path(__file__).resolve().parent.parent
WRAPPER = ROOT / "mau"

# The stand-in cargo. `build` only: it writes target/<profile>/<name> when that is missing or
# when STUB_CHANGE=1, so an up-to-date build leaves the file alone, as cargo does.
STUB_CARGO = """#!/bin/sh
echo "$*" >> "$STUB_LOG"
if [ "$STUB_FAIL" = 1 ]; then
  echo "error: stub build failure" >&2
  exit 101
fi
case " $* " in *" -p mau-gui "*) name=mau ;; *) name=mau-cli ;; esac
profile=debug
case " $* " in *" --release "*) profile=release ;; esac
out="target/$profile/$name"
if [ ! -x "$out" ] || [ "$STUB_CHANGE" = 1 ]; then
  mkdir -p "target/$profile"
  printf '#!/bin/sh\\necho "%s %s ran: $*"\\n' "$name" "$STUB_VERSION" > "$out"
  chmod +x "$out"
fi
"""


@pytest.fixture
def checkout(tmp_path):
    """A copy of the wrapper in an empty checkout, with the stand-in cargo first on PATH."""
    repo = tmp_path / "checkout"
    (repo / "rust").mkdir(parents=True)
    shutil.copy(WRAPPER, repo / "mau")
    (repo / "mau").chmod(0o755)
    bindir = tmp_path / "bin"
    bindir.mkdir()
    cargo = bindir / "cargo"
    cargo.write_text(STUB_CARGO)
    cargo.chmod(cargo.stat().st_mode | stat.S_IXUSR)
    return repo, bindir


def run_mau(repo, bindir, *args, **env):
    log = repo.parent / "cargo.log"
    full = dict(os.environ)
    full.pop("MAU_NO_BUILD", None)
    full.update(
        PATH=f"{bindir}{os.pathsep}{os.environ['PATH']}", STUB_LOG=str(log), STUB_VERSION="v1"
    )
    full.update(env)
    result = subprocess.run(
        [str(repo / "mau"), *args], capture_output=True, text=True, env=full, cwd=repo.parent
    )
    calls = log.read_text().splitlines() if log.exists() else []
    return result, calls


def test_the_first_run_builds_the_cli_and_runs_it(checkout):
    repo, bindir = checkout
    result, calls = run_mau(repo, bindir, "--version")
    assert result.returncode == 0, result.stderr
    assert "mau-cli v1 ran: --version" in result.stdout
    assert calls == ["build --release -p mau-cli"]
    assert "building mau-cli for the first time" in result.stderr


def test_an_up_to_date_build_is_checked_quietly_and_not_rebuilt(checkout):
    repo, bindir = checkout
    run_mau(repo, bindir, "--version")
    result, calls = run_mau(repo, bindir, "--version")
    assert result.returncode == 0, result.stderr
    assert "mau-cli v1 ran" in result.stdout
    assert "rebuilt" not in result.stderr and "building" not in result.stderr
    # the check is cargo's own, run quietly, every time
    assert calls == ["build --release -p mau-cli", "build -q --release -p mau-cli"]


def test_changed_sources_are_rebuilt_before_the_run(checkout):
    repo, bindir = checkout
    run_mau(repo, bindir, "--version")
    result, _ = run_mau(repo, bindir, "--version", STUB_CHANGE="1", STUB_VERSION="v2")
    assert result.returncode == 0, result.stderr
    assert "rebuilt mau-cli, its sources had changed" in result.stderr
    assert "mau-cli v2 ran: --version" in result.stdout


def test_gui_also_brings_the_app_up_to_date(checkout):
    repo, bindir = checkout
    result, calls = run_mau(repo, bindir, "--gui")
    assert result.returncode == 0, result.stderr
    assert calls == ["build --release -p mau-cli", "build --release -p mau-gui"]
    # the stand-in CLI got --gui, as the real one hands over to the app
    assert "mau-cli v1 ran: --gui" in result.stdout
    assert (repo / "rust" / "target" / "release" / "mau").exists()


def test_a_failed_build_stops_the_run_and_runs_nothing(checkout):
    repo, bindir = checkout
    run_mau(repo, bindir, "--version")
    result, _ = run_mau(
        repo, bindir, "--version", STUB_FAIL="1", STUB_CHANGE="1", STUB_VERSION="v2"
    )
    assert result.returncode == 1
    assert "the build of mau-cli failed" in result.stderr
    assert "MAU_NO_BUILD=1" in result.stderr
    assert "ran" not in result.stdout


def test_mau_no_build_skips_cargo_and_runs_the_existing_build(checkout):
    repo, bindir = checkout
    run_mau(repo, bindir, "--version")
    result, calls = run_mau(
        repo, bindir, "--version", MAU_NO_BUILD="1", STUB_CHANGE="1", STUB_VERSION="v2"
    )
    assert result.returncode == 0, result.stderr
    assert "mau-cli v1 ran: --version" in result.stdout
    assert len(calls) == 1  # only the first run called cargo


def test_the_old_fml_no_build_name_still_skips_cargo(checkout):
    repo, bindir = checkout
    run_mau(repo, bindir, "--version")
    result, calls = run_mau(
        repo, bindir, "--version", FML_NO_BUILD="1", STUB_CHANGE="1", STUB_VERSION="v2"
    )
    assert result.returncode == 0, result.stderr
    assert "mau-cli v1 ran: --version" in result.stdout
    assert len(calls) == 1


def test_mau_no_build_with_nothing_built_says_how_to_build(checkout):
    repo, bindir = checkout
    result, calls = run_mau(repo, bindir, "--version", MAU_NO_BUILD="1")
    assert result.returncode == 1
    assert "not built yet" in result.stderr
    assert calls == []


def test_a_debug_only_checkout_is_kept_on_the_debug_build(checkout):
    repo, bindir = checkout
    debug = repo / "rust" / "target" / "debug"
    debug.mkdir(parents=True)
    (debug / "mau-cli").write_text('#!/bin/sh\necho "mau-cli debug ran: $*"\n')
    (debug / "mau-cli").chmod(0o755)
    result, calls = run_mau(repo, bindir, "--version")
    assert result.returncode == 0, result.stderr
    assert "mau-cli debug ran: --version" in result.stdout
    assert calls == ["build -q -p mau-cli"]
