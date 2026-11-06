#!/usr/bin/env bash
# Build ffmpeg and ffprobe from source as LGPL-only tools, for build.sh --lgpl.
#   packaging/ffmpeg/build-lgpl.sh PREFIX
# The source is ffmpeg $VERSION from ffmpeg.org, checked against SHA256 before it is unpacked. The configuration has no
# --enable-gpl and no --enable-nonfree, and no external codec libraries (x264, x265, ...): mau copies streams, and makes
# its subtitle conversions and audio clips with the encoders built into ffmpeg (srt, mov_text, pcm_s16le). The libraries
# are shared (libavcodec and the rest, as .dylib or .so), so a user can replace them, as the LGPL requires.
# PREFIX gets bin/ (ffmpeg, ffprobe), lib/ and licences/ (the LGPL text, and SOURCE.txt: where the source is, its checksum
# and how it was configured). The build needs make and a C compiler (on a Mac, Xcode's command line tools).
set -euo pipefail

VERSION=9.0.2
SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
URL="https://ffmpeg.org/releases/ffmpeg-$VERSION.tar.xz"
CONFIGURE=(--disable-gpl --disable-nonfree --enable-shared --disable-static --disable-doc --disable-debug
           --disable-ffplay --disable-sdl2 --disable-xlib --disable-libxcb --enable-pic)

[ $# -eq 1 ] || { echo "usage: $0 PREFIX" >&2; exit 2; }
prefix=$1
case "$prefix" in /*) ;; *) echo "PREFIX must be an absolute path" >&2; exit 2 ;; esac
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

echo "== fetching ffmpeg $VERSION =="
curl -fsSL -o "$work/src.tar.xz" "$URL"
echo "$SHA256  $work/src.tar.xz" | shasum -a 256 -c - >/dev/null ||
  { echo "the ffmpeg $VERSION download does not match $SHA256" >&2; exit 1; }
tar -C "$work" -xf "$work/src.tar.xz"
src="$work/ffmpeg-$VERSION"

echo "== configuring (LGPL only) =="
(cd "$src" && ./configure --prefix="$prefix" "${CONFIGURE[@]}" > "$work/configure.log" 2>&1) ||
  { tail -40 "$work/configure.log" >&2; exit 1; }
licence=$(grep -m1 "^License:" "$work/configure.log" || true)
case "$licence" in
  *"LGPL version 2.1"*) ;;
  *) echo "configure did not give an LGPL licence: ${licence:-no licence line}" >&2; exit 1 ;;
esac

echo "== building (this takes a few minutes) =="
jobs=$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 2)
(cd "$src" && make -j"$jobs" > "$work/make.log" 2>&1) || { tail -40 "$work/make.log" >&2; exit 1; }
(cd "$src" && make install >> "$work/make.log" 2>&1) || { tail -40 "$work/make.log" >&2; exit 1; }

echo "== checking the build =="
ffmpeg="$prefix/bin/ffmpeg"
config=$("$ffmpeg" -buildconf 2>&1)
case "$config" in
  *--enable-gpl*|*--enable-nonfree*|*--enable-version3*) echo "the build has a GPL or nonfree option" >&2; exit 1 ;;
esac
licence=$("$ffmpeg" -L 2>&1 || true)
case "$licence" in
  *"Lesser General Public"*) ;;
  *) echo "ffmpeg does not say it is under the Lesser General Public License" >&2; exit 1 ;;
esac
encoders=$("$ffmpeg" -hide_banner -encoders 2>/dev/null)
muxers=$("$ffmpeg" -hide_banner -muxers 2>/dev/null)
for name in srt mov_text pcm_s16le; do
  case "$encoders" in *" $name "*) ;; *) echo "the build has no $name encoder" >&2; exit 1 ;; esac
done
for name in matroska mp4 sup; do
  case "$muxers" in *" $name "*) ;; *) echo "the build has no $name muxer" >&2; exit 1 ;; esac
done

echo "== writing the licence notice =="
mkdir -p "$prefix/licences"
cp "$src/COPYING.LGPLv2.1" "$prefix/licences/"
{
  echo "ffmpeg $VERSION, built by mau from the source below under the GNU Lesser General Public License, version 2.1 or later"
  echo "(the text is in COPYING.LGPLv2.1 beside this file)."
  echo
  echo "Source:   $URL"
  echo "SHA-256:  $SHA256"
  echo "Configure: ./configure ${CONFIGURE[*]}"
  echo
  echo "The shared libraries (libav*, libsw*) are in the app's lib folder. To replace them, build the source above with"
  echo "the same configure options, and copy the new libraries over the old ones."
} > "$prefix/licences/SOURCE.txt"
echo "built $("$ffmpeg" -version | head -1) in $prefix"
