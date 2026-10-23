#!/usr/bin/env bash
# Build fml installers (the desktop app) and the command-line binary.
#   ./build.sh --target mac [--arch arm64|x64|universal]                     (on a Mac: builds a .dmg; untested)
#   ./build.sh --target linux [--package deb|rpm|appimage|all]               (comma list allowed, e.g. deb,rpm; default: the
#                                                                 distribution's own package: deb on Debian/Ubuntu, rpm on
#                                                                 RHEL/Fedora/Alma/Rocky/SUSE, else appimage;
#                                                                 --target deb / --target rpm = linux with that package only)
#   ./build.sh --target windows      (on Linux or a Mac: cross-compiles the NSIS installer and fml-cli.exe in a Docker
#                                                                 container, docker/windows/Dockerfile (MinGW, x86_64-pc-windows-gnu);
#                                                                 on Windows, e.g. in Git Bash: builds the installer there; untested)
# The app's installer and the command line are built apart: the installer is a .deb/.rpm/AppImage, .exe or .dmg, the
# command line an archive, fml-cli-<version>-<system>.tar.gz or .zip, holding just the `fml-cli` binary. Add --no-cli to skip it.
# Results are copied to ./dist/ (gitignored).
# A target can only be built on its own platform (the app links the system web view).
# Needs the Rust toolchain and tauri-cli (cargo install tauri-cli --version '^2'). The interface in rust/fml-gui/ui is plain
# HTML, CSS and JavaScript, so there is no Node step. On Linux the app also needs libwebkit2gtk-4.1-dev and libgtk-3-dev.
#   ./build.sh [--target <t>] --release [<version>] [--commit <ref>] [--draft] [--yes] [--dry-run]
#   ./build.sh --delete-release [<version>] [--keep-tag] [--yes] [--dry-run]
# --release tags the commit v<version> (version from rust/Cargo.toml), pushes the tag and creates the GitHub release with the
# files in dist/ for that version (needs `gh`, logged in). The release notes are that version's section of CHANGELOG.md.
# If the release already exists the files are added to it, so each platform can be built and added on its own machine.
# It never commits: the working tree must already be committed and pushed. Without --target it only releases what is
# already in dist/. --draft creates a draft release, --yes skips the question, --dry-run prints what it would do and
# changes nothing. --release 0.7.1 releases that version instead of the current one: it uses the files in dist/ for it
# and tags its existing tag, else the newest commit whose rust/Cargo.toml says that version (which must be pushed);
# --commit <ref> (a commit, branch or tag) says which commit to tag instead: for a version no commit's rust/Cargo.toml says
# (say it was only ever built from the working tree), with --release <version>. The commit must be pushed.
# a version other than the current one cannot be combined with --target (build it from a checkout of that commit).
# --delete-release deletes the GitHub release v<version> (default: the current version) with its uploaded files, and the
# tag v<version> on GitHub and locally unless --keep-tag is given. It asks first (--yes skips the question). It never
# touches commits or dist/.
# Apart from --release and --delete-release it never commits, tags, pushes or publishes anything.
set -euo pipefail

usage() {
  echo "usage: $0 --target mac|linux|deb|rpm|windows [--arch arm64|x64|universal] [--package deb|rpm|appimage|all] [--no-cli]"
  echo "       $0 [--target <t>] --release [<version>] [--commit <ref>] [--draft] [--yes] [--dry-run]"
  echo "       $0 --delete-release [<version>] [--keep-tag] [--yes] [--dry-run]"
  exit "${1:-1}"
}

target="" arch="" package="" cli=1 delete=0 del_version="" rel_version="" rel_commit="" keep_tag=0 release=0 draft=0 yes=0 dry=0
while [ $# -gt 0 ]; do
  case "$1" in
    --target) target="${2:-}"; shift 2 ;;
    --arch) arch="${2:-}"; shift 2 ;;
    --package|--format) package="${2:-}"; shift 2 ;;
    --no-cli) cli=0; shift ;;
    --release)
      release=1; shift
      case "${1:-}" in -*|"") ;; *) rel_version="${1#v}"; shift ;;
      esac ;;
    --delete-release)
      delete=1; shift
      case "${1:-}" in -*|"") ;; *) del_version="${1#v}"; shift ;;
      esac ;;
    --commit) rel_commit="${2:-}"; [ -n "$rel_commit" ] || { echo "--commit needs a commit or tag" >&2; exit 1; }; shift 2 ;;
    --keep-tag) keep_tag=1; shift ;;
    --draft) draft=1; shift ;;
    --yes) yes=1; shift ;;
    --dry-run) dry=1; shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown option: $1" >&2; usage ;;
  esac
done
[ -n "$target" ] || [ "$release" = 1 ] || [ "$delete" = 1 ] || usage
[ -z "$rel_version" ] || [ -z "$target" ] || { echo "--release <version> cannot be combined with --target: build that version first" >&2; exit 1; }
[ "$delete" = 0 ] || { [ "$release" = 0 ] && [ -z "$target" ]; } || { echo "--delete-release cannot be combined with --release or --target" >&2; exit 1; }

root=$(cd "$(dirname "$0")" && pwd)
dist="$root/dist"
# the version of the workspace (the first `version =` line of rust/Cargo.toml, under [workspace.package])
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/rust/Cargo.toml" | head -1)
current=$version

# The package format of the Linux distribution this runs on (/etc/os-release ID and ID_LIKE).
native_package() {
  local ids=""
  [ -r /etc/os-release ] && ids=$(. /etc/os-release; echo "${ID:-} ${ID_LIKE:-}")
  case " $ids " in
    *" debian "*|*" ubuntu "*) echo deb ;;
    *" rhel "*|*" fedora "*|*" centos "*|*" almalinux "*|*" rocky "*|*" suse "*|*" opensuse "*|*" opensuse-leap "*|*" sles "*|*" amzn "*|*" ol "*) echo rpm ;;
    *) echo appimage ;;
  esac
}

# The command line is shipped as an archive, apart from the app's installer: fml-cli-<version>-<label>.tar.gz
# (Linux, macOS) or .zip (Windows), holding a folder of the same name with just the `fml-cli` binary in it.
# usage: package_cli <path to the built binary> <label, e.g. linux or windows> [.exe]
package_cli() {
  local bin="$1" label="$2" exe="${3:-}" name work
  name="fml-cli-$version-$label"
  work=$(mktemp -d)
  mkdir "$work/$name"
  cp "$bin" "$work/$name/fml-cli$exe"
  chmod +x "$work/$name/fml-cli$exe"
  mkdir -p "$dist"
  if [ -n "$exe" ]; then
    rm -f "$dist/$name.zip"
    if command -v zip >/dev/null; then
      (cd "$work" && zip -q -r "$dist/$name.zip" "$name")
    elif command -v python3 >/dev/null; then
      python3 -I -c 'import shutil, sys; shutil.make_archive(sys.argv[1], "zip", sys.argv[2], sys.argv[3])' "$dist/$name" "$work" "$name"
    else
      echo "neither zip nor python3 found: cannot make $name.zip" >&2; exit 1
    fi
    echo "  $dist/$name.zip"
  else
    (cd "$work" && tar -czf "$dist/$name.tar.gz" "$name")
    echo "  $dist/$name.tar.gz"
  fi
  rm -rf "$work"
}

# Windows from Linux or a Mac: cross-compile in a container (docker/windows), no Windows machine needed. The repository
# is mounted read-only; the target directory and the cargo registry are Docker volumes, so only the first build compiles
# everything. The installer and fml-cli.exe are copied to dist/, and fml-cli.exe is packaged like the other platforms.
windows_docker() {
  command -v docker >/dev/null || { echo "docker not found: install it, or build on Windows" >&2; exit 1; }
  docker info >/dev/null 2>&1 || { echo "docker is not running, or you may not use it (is your user in the docker group?)" >&2; exit 1; }
  tauri_version=$(sed -n 's/^ *"version": *"\(.*\)".*/\1/p' "$root/rust/fml-gui/tauri.conf.json" | head -1)
  [ "$tauri_version" = "$version" ] || { echo "rust/fml-gui/tauri.conf.json says $tauri_version but rust/Cargo.toml says $version: make them the same" >&2; exit 1; }
  echo "== Windows cross build in Docker (x86_64-pc-windows-gnu) =="
  docker build -t fml-windows-cross "$root/docker/windows"
  out=$(mktemp -d)
  inner=(); [ "$cli" = 1 ] || inner=(--no-cli)
  docker run --rm \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    -e CARGO_HOME=/cargo-home -e CARGO_TARGET_DIR=/target \
    -v "$root":/src:ro -v "$out":/out \
    -v fml-windows-target:/target -v fml-windows-cargo:/cargo-home \
    fml-windows-cross \
    sh -c '/src/docker/windows/build.sh "$@"; rc=$?; chown -R "$HOST_UID:$HOST_GID" /out 2>/dev/null; exit $rc' sh ${inner[@]+"${inner[@]}"} \
    || { rm -rf "$out"; echo "the Windows build failed" >&2; exit 1; }
  mkdir -p "$dist"
  found=0
  for f in "$out"/*-setup.exe; do
    [ -f "$f" ] || continue
    cp "$f" "$dist/"; echo "  $dist/$(basename "$f")"; found=1
  done
  [ "$found" = 1 ] || { rm -rf "$out"; echo "no installer was built" >&2; exit 1; }
  if [ "$cli" = 1 ]; then
    [ -f "$out/fml-cli.exe" ] || { rm -rf "$out"; echo "fml-cli.exe was not built" >&2; exit 1; }
    package_cli "$out/fml-cli.exe" windows .exe
  fi
  rm -rf "$out"
  echo "done: $dist"
}

build_target() {
  host=$(uname -s)
  case "$target" in deb|rpm) package="$target"; target=linux ;; esac
  case "$target" in
    mac|macos)   want=Darwin; bundles=dmg ;;
    linux)       want=Linux;  bundles=$(native_package) ;;
    windows|win) want=MINGW;  bundles=nsis ;;
    *) echo "unknown target: $target (mac, linux, deb, rpm, windows)" >&2; exit 1 ;;
  esac
  if [ -n "$package" ] && [ "$target" != linux ]; then
    echo "--package only applies to --target linux" >&2; exit 1
  fi
  if [ "$target" = linux ] && [ -n "$package" ]; then
    bundles=""
    for p in ${package//,/ }; do
      case "$p" in
        all) bundles="deb,rpm,appimage"; break ;;
        deb|rpm|appimage) bundles="${bundles:+$bundles,}$p" ;;
        *) echo "unknown --package: $p (deb, rpm, appimage, all)" >&2; exit 1 ;;
      esac
    done
  fi
  if [ "$want" = MINGW ]; then
    case "$host" in
      MINGW*|MSYS*|CYGWIN*) ;;
      *) windows_docker; return ;;
    esac
  fi
  case "$host" in
    $want*) ;;
    *) echo "the $target installer must be built on $target (this is $host)" >&2; exit 1 ;;
  esac
  if [ "$target" = linux ]; then
    case ",$bundles," in
      *,rpm,*) command -v rpmbuild >/dev/null || { echo "rpmbuild not found: install it (Debian/Ubuntu: apt install rpm; Fedora: dnf install rpm-build)" >&2; exit 1; } ;;
    esac
    if command -v pkg-config >/dev/null && ! pkg-config --exists webkit2gtk-4.1 gtk+-3.0; then
      echo "the system web view is missing: install libwebkit2gtk-4.1-dev and libgtk-3-dev (Debian/Ubuntu)" >&2; exit 1
    fi
  fi

  command -v cargo >/dev/null || { echo "cargo not found: install Rust (https://rustup.rs)" >&2; exit 1; }
  cargo tauri --version >/dev/null 2>&1 || { echo "tauri-cli not found: cargo install tauri-cli --version '^2'" >&2; exit 1; }

  # the installer's file name comes from tauri.conf.json, not Cargo.toml: they must agree
  tauri_version=$(sed -n 's/^ *"version": *"\(.*\)".*/\1/p' "$root/rust/fml-gui/tauri.conf.json" | head -1)
  [ "$tauri_version" = "$version" ] || { echo "rust/fml-gui/tauri.conf.json says $tauri_version but rust/Cargo.toml says $version: make them the same" >&2; exit 1; }

  triple="" triple_args=()
  if [ "$target" = mac ] || [ "$target" = macos ]; then
    case "${arch:-native}" in
      native) ;;
      arm64) triple=aarch64-apple-darwin ;;
      x64) triple=x86_64-apple-darwin ;;
      universal) triple=universal-apple-darwin ;;
      *) echo "unknown --arch: $arch (arm64, x64, universal)" >&2; exit 1 ;;
    esac
    if [ -n "$triple" ]; then
      triple_args=(--target "$triple")
      # make sure the Rust standard library for the requested architecture is installed
      case "$triple" in
        universal-apple-darwin) rustup target add aarch64-apple-darwin x86_64-apple-darwin ;;
        *) rustup target add "$triple" ;;
      esac
    fi
  elif [ -n "$arch" ]; then
    echo "--arch only applies to --target mac" >&2; exit 1
  fi

  mkdir -p "$dist"
  stamp=$(mktemp)
  echo "== building the installer ($target, $bundles) =="
  # cargo tauri build runs in rust/fml-gui; the workspace puts the bundles under rust/target/
  (cd "$root/rust/fml-gui" && cargo tauri build --bundles "$bundles" ${triple_args[@]+"${triple_args[@]}"})

  # Tauri puts bundles under target/[<triple>/]release/bundle/<kind>/
  found=0
  while IFS= read -r f; do
    cp -R "$f" "$dist/"
    echo "  $dist/$(basename "$f")"
    found=1
  done < <(find "$root/rust/target" -path '*/release/bundle/*' -type f \
    \( -name '*.dmg' -o -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*-setup.exe' \) -newer "$stamp" 2>/dev/null)
  rm -f "$stamp"
  [ "$found" = 1 ] || { echo "no installer found under rust/target/ (did the bundle step run?)" >&2; exit 1; }

  if [ "$cli" = 1 ]; then
    echo "== building the fml-cli command-line binary =="
    exe=""; [ "$target" != windows ] || exe=".exe"
    if [ "$triple" = universal-apple-darwin ]; then
      # plain cargo has no universal target: build both architectures and join them with lipo
      (cd "$root/rust" && cargo build --release -p fml-cli --target aarch64-apple-darwin &&
        cargo build --release -p fml-cli --target x86_64-apple-darwin)
      mkdir -p "$root/rust/target/universal-apple-darwin/release"
      lipo -create -output "$root/rust/target/universal-apple-darwin/release/fml-cli" \
        "$root/rust/target/aarch64-apple-darwin/release/fml-cli" "$root/rust/target/x86_64-apple-darwin/release/fml-cli"
      bin="$root/rust/target/universal-apple-darwin/release/fml-cli"
    elif [ -n "$triple" ]; then
      (cd "$root/rust" && cargo build --release -p fml-cli --target "$triple")
      bin="$root/rust/target/$triple/release/fml-cli$exe"
    else
      (cd "$root/rust" && cargo build --release -p fml-cli)
      bin="$root/rust/target/release/fml-cli$exe"
    fi
    [ -f "$bin" ] || { echo "the command-line binary was not found at $bin" >&2; exit 1; }
    # the archive's label: the system and, on a Mac, the architecture
    label="$target"
    case "$target" in
      mac|macos)
        case "${triple:-native}" in
          aarch64-apple-darwin) label=mac-arm64 ;;
          x86_64-apple-darwin) label=mac-x64 ;;
          universal-apple-darwin) label=mac-universal ;;
          *) case "$(uname -m)" in arm64|aarch64) label=mac-arm64 ;; *) label=mac-x64 ;; esac ;;
        esac ;;
    esac
    package_cli "$bin" "$label" "$exe"
  fi
  echo "done: $dist"
}

release_it() {
  die() { echo "release: $*" >&2; exit 1; }
  # run a command, or only show it with --dry-run
  run() { if [ "$dry" = 1 ]; then echo "  [dry-run] $*"; else "$@"; fi; }
  # a problem stops a real release; a dry run only reports it
  problem() { if [ "$dry" = 1 ]; then echo "  [dry-run] would stop: $*"; else die "$*"; fi; }

  command -v gh >/dev/null || die "gh (the GitHub CLI) is not installed"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in (gh auth login)"
  # the version to release: the one given (--release 0.5.3), else the current one
  local version="${rel_version:-$version}"
  local tag="v$version"
  shopt -s nullglob
  local assets=("$dist"/*_"$version"_* "$dist"/*-"$version"-*)
  shopt -u nullglob
  [ "${#assets[@]}" -gt 0 ] || die "no files for $version in $dist (build it first: $0 --target <t>)"

  cd "$root"
  git fetch -q origin 2>/dev/null || true
  local commit
  if [ -n "$rel_commit" ]; then
    commit=$(git rev-parse -q --verify "$rel_commit^{commit}") || die "$rel_commit is not a commit"
    [ -n "$(git branch -r --contains "$commit" 2>/dev/null)" ] || problem "commit $(git rev-parse --short "$commit") is not on GitHub: push it first"
  elif [ "$version" = "$current" ]; then
    commit=$(git rev-parse HEAD)
    [ -z "$(git status --porcelain)" ] || problem "uncommitted changes: commit them first (this script never commits)"
    local upstream
    upstream=$(git rev-parse --abbrev-ref '@{u}' 2>/dev/null) || upstream=""
    if [ -z "$upstream" ]; then
      problem "this branch has no upstream: push it first"
    elif [ "$commit" != "$(git rev-parse '@{u}')" ]; then
      problem "HEAD is not the same as $upstream: push (or pull) first"
    fi
  else
    # an older version: its tag if there is one, else the newest commit whose rust/Cargo.toml says that version
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
      commit=$(git rev-list -n1 "$tag")
    else
      commit=""
      local c
      for c in $(git log --format=%H -- rust/Cargo.toml); do
        if [ "$(git show "$c:rust/Cargo.toml" 2>/dev/null | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)" = "$version" ]; then commit=$c; break; fi
      done
      [ -n "$commit" ] || die "no tag $tag and no commit with version $version in rust/Cargo.toml"
    fi
    [ -n "$(git branch -r --contains "$commit" 2>/dev/null)" ] || problem "commit $(git rev-parse --short "$commit") is not on GitHub: push it first"
  fi
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null && [ "$(git rev-list -n1 "$tag")" != "$commit" ]; then
    problem "tag $tag already exists on another commit"
  fi

  local notes
  notes=$(mktemp)
  awk -v ver="$version" 'index($0, "## [" ver "]") == 1 { p = 1; next } /^## \[/ { p = 0 } p' "$root/CHANGELOG.md" > "$notes"
  local notes_args=(--notes-file "$notes")
  if [ ! -s "$notes" ]; then
    echo "  CHANGELOG.md has no section for $version: GitHub will generate the notes"
    notes_args=(--generate-notes)
  fi

  local exists=0
  gh release view "$tag" >/dev/null 2>&1 && exists=1
  echo "== release $tag =="
  echo "  commit:  $(git log -1 --format='%h %s' "$commit")"
  echo "  files:"
  printf '    %s\n' "${assets[@]##*/}"
  if [ "$exists" = 1 ]; then echo "  the release exists: the files above are added to it"; else echo "  a new release will be created$([ "$draft" = 1 ] && echo ' (draft)')"; fi
  if [ "$yes" != 1 ] && [ "$dry" != 1 ]; then
    read -r -p "Tag, push and publish? [y/N] " ans
    case "$ans" in y|Y|yes) ;; *) echo "stopped"; exit 1 ;; esac
  fi

  git rev-parse -q --verify "refs/tags/$tag" >/dev/null || run git tag -a "$tag" -m "fml $version" "$commit"
  run git push origin "$tag"
  if [ "$exists" = 1 ]; then
    run gh release upload "$tag" "${assets[@]}" --clobber
  else
    local draft_args=()
    [ "$draft" = 1 ] && draft_args=(--draft)
    run gh release create "$tag" "${assets[@]}" --title "fml $version" "${notes_args[@]}" ${draft_args[@]+"${draft_args[@]}"}
  fi
  rm -f "$notes"
  if [ "$dry" = 1 ]; then echo "dry run: nothing was tagged, pushed or published"; return; fi
  echo "release done: $(gh release view "$tag" --json url -q .url 2>/dev/null || echo "$tag")"
}

delete_release() {
  die() { echo "delete-release: $*" >&2; exit 1; }
  run() { if [ "$dry" = 1 ]; then echo "  [dry-run] $*"; else "$@"; fi; }
  command -v gh >/dev/null || die "gh (the GitHub CLI) is not installed"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in (gh auth login)"
  local tag="v${del_version:-$version}"
  cd "$root"
  local exists=0 remote_tag=0 local_tag=0
  gh release view "$tag" >/dev/null 2>&1 && exists=1
  git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1 && remote_tag=1
  git rev-parse -q --verify "refs/tags/$tag" >/dev/null && local_tag=1
  [ "$exists" = 1 ] || [ "$remote_tag" = 1 ] || [ "$local_tag" = 1 ] || die "no release or tag $tag found"
  echo "== delete release $tag =="
  [ "$exists" = 1 ] && echo "  GitHub release: $(gh release view "$tag" --json url -q .url) (with its files)" || echo "  no GitHub release"
  if [ "$keep_tag" = 1 ]; then
    echo "  the tag is kept"
  else
    echo "  tag on GitHub: $([ "$remote_tag" = 1 ] && echo yes || echo no), local tag: $([ "$local_tag" = 1 ] && echo yes || echo no)"
  fi
  if [ "$yes" != 1 ] && [ "$dry" != 1 ]; then
    read -r -p "Delete? This cannot be undone. [y/N] " ans
    case "$ans" in y|Y|yes) ;; *) echo "stopped"; exit 1 ;; esac
  fi
  if [ "$exists" = 1 ]; then
    if [ "$keep_tag" = 1 ]; then run gh release delete "$tag" --yes; else run gh release delete "$tag" --yes --cleanup-tag; fi
  fi
  if [ "$keep_tag" != 1 ]; then
    [ "$exists" = 1 ] || [ "$remote_tag" = 0 ] || run git push origin ":refs/tags/$tag"
    [ "$local_tag" = 0 ] || run git tag -d "$tag"
  fi
  echo "delete done: $tag"
}

if [ "$delete" = 1 ]; then delete_release; exit 0; fi
if [ -n "$target" ]; then build_target; fi
if [ "$release" = 1 ]; then release_it; fi
