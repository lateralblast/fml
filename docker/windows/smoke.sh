#!/usr/bin/env bash
# Runs the Windows fml-cli.exe under Wine, in a container, against stand-in tools (.bat files), and checks
# what it does: it starts, finds the tools through PATHEXT, makes a checked edit with a backup and a log, and
# refuses a change the tool did not make, without leaving a temporary file. It is a smoke test of the
# Windows-only code (paths, the disk-space call, spawning .bat/.exe, renaming over a file); the real tools,
# the GUI and Ctrl-C need a real Windows machine.
#   docker/windows/smoke.sh [path to fml-cli-<version>-windows.zip]   (default: the newest in dist/)
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
zip=${1:-$(ls -t "$root"/dist/fml-cli-*-windows.zip 2>/dev/null | head -1)}
[ -f "${zip:-}" ] || { echo "no fml-cli-*-windows.zip: build it first (./build.sh --target windows)" >&2; exit 1; }
command -v docker >/dev/null || { echo "docker not found" >&2; exit 1; }

work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
mkdir -p "$work/tools" "$work/media"
python3 - "$zip" "$work" <<'PY'
import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
name = next(n for n in z.namelist() if n.endswith("fml-cli.exe"))
open(sys.argv[2] + "/fml-cli.exe", "wb").write(z.read(name))
before = ('{"tracks":[{"id":0,"type":"video","codec":"V","properties":{}},'
          '{"id":1,"type":"audio","codec":"AAC","properties":{"language":"ita","default_track":true}},'
          '{"id":2,"type":"audio","codec":"AAC","properties":{"language":"eng","default_track":false}}]}')
after = before.replace('"ita","default_track":true', '"ita","default_track":false') \
              .replace('"eng","default_track":false', '"eng","default_track":true')
# the stand-in mkvmerge shows the changed layout for the temporary copy (.fml-tmp), as if mkvpropedit worked
open(sys.argv[2] + "/tools/mkvmerge.bat", "w", newline="").write(
    '@echo off\r\nif "%~x4"==".fml-tmp" goto changed\r\necho ' + before + '\r\ngoto :eof\r\n:changed\r\necho ' + after + '\r\n')
for tool in ("mkvpropedit", "mediainfo", "ffmpeg"):
    open(f"{sys.argv[2]}/tools/{tool}.bat", "w", newline="").write("@echo off\r\nexit /b 0\r\n")
open(sys.argv[2] + "/media/fake.mkv", "wb").write(bytes.fromhex("1a45dfa39f428288") + b"matroska" + bytes(1000))
PY

cat > "$work/steps.sh" <<'STEPS'
export WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=" WINEPREFIX=/root/.wine WINEPATH='Z:\work\tools'
cd /work
F='Z:\work\media\fake.mkv'
run() { wine fml-cli.exe "$@" 2>&1 | grep -v '^wine: '; return "${PIPESTATUS[0]}"; }
echo "## version";  run --version
echo "## check";    run --check
echo "## dryrun";   run --set lang --default eng --file "$F" --dryrun
echo "## edit";     run --set lang --default eng --file "$F" --backup --log 'Z:\work\media\log.jsonl'
echo "## files";    ls media
echo "## refused";  run --delete lang --lang eng --file "$F"; echo "rc=$?"
echo "## files";    ls media
STEPS

if ! docker image inspect fml-wine-test >/dev/null 2>&1; then
  printf 'FROM ubuntu:24.04\nENV DEBIAN_FRONTEND=noninteractive\nRUN apt-get update -qq && apt-get install -y -qq --no-install-recommends wine64 wine && rm -rf /var/lib/apt/lists/*\n' \
    | docker build -q -t fml-wine-test - >/dev/null
fi
out=$(docker run --rm -v "$work":/work fml-wine-test bash /work/steps.sh)
echo "$out"

fail=0
expect() { if echo "$out" | grep -qF -- "$1"; then echo "ok:   $1"; else echo "FAIL: expected \"$1\""; fail=1; fi; }
expect "fml-cli 0."
expect "Found:		mkvmerge (Z:\\work\\tools\\mkvmerge.bat)"
expect "[dry-run] Z:\\work\\media\\fake.mkv: audio track 2 (eng): default no -> yes"
expect "the original is kept as Z:\\work\\media\\fake.mkv.fml-bak"
expect "mkvmerge did not make the change to the file"
expect "rc=1"
# after the edit and after the refusal the folder holds the file, its backup and the log, and no temporary file
if echo "$out" | grep -q "fml-tmp"; then echo "FAIL: a temporary file was left"; fail=1; else echo "ok:   no temporary file left"; fi
[ "$(echo "$out" | grep -c '^fake.mkv.fml-bak$')" = 2 ] && echo "ok:   backup kept" || { echo "FAIL: backup missing"; fail=1; }
exit $fail
