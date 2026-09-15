#!/usr/bin/env bash
#
# Cut a release.
#
#   scripts/release.sh 0.4.0            # bump, commit, tag, push
#   scripts/release.sh 0.4.0 --dry-run  # show what would happen
#
# The version lives in exactly one place ([workspace.package] in the root
# Cargo.toml) and every crate inherits it, so this script only has to change one
# line. The release workflow independently refuses to publish a tag that
# disagrees with Cargo.toml, which is what keeps `ratchet --version` honest.

set -euo pipefail

DRY_RUN=0
VERSION=""

for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        -h|--help)
            sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            if [ -n "$VERSION" ]; then
                echo "error: unexpected argument '$arg'" >&2
                exit 1
            fi
            VERSION="$arg"
            ;;
    esac
done

if [ -z "$VERSION" ]; then
    echo "usage: scripts/release.sh <version> [--dry-run]" >&2
    exit 1
fi

# --- validate ---------------------------------------------------------------

if ! printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "error: version must look like MAJOR.MINOR.PATCH (got '$VERSION')" >&2
    exit 1
fi

cd "$(dirname "$0")/.."

if [ ! -f Cargo.toml ]; then
    echo "error: run this from the repository root" >&2
    exit 1
fi

current="$(grep -m1 '^version' Cargo.toml | sed -E 's/^version[[:space:]]*=[[:space:]]*"(.*)"/\1/')"
tag="v$VERSION"

if [ "$current" = "$VERSION" ]; then
    echo "error: Cargo.toml is already at $VERSION" >&2
    exit 1
fi

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag already exists" >&2
    exit 1
fi

# A dirty tree means the tag would not describe what was tested.
if [ -n "$(git status --porcelain)" ]; then
    echo "error: working tree is dirty — commit or stash first" >&2
    git status --short >&2
    exit 1
fi

branch="$(git rev-parse --abbrev-ref HEAD)"
if [ "$branch" != "main" ]; then
    echo "warning: releasing from '$branch', not 'main'" >&2
fi

echo "releasing $current -> $VERSION  (tag $tag, branch $branch)"

if [ "$DRY_RUN" = "1" ]; then
    echo "--dry-run: nothing changed"
    exit 0
fi

# --- check it actually builds and passes ------------------------------------

echo "running tests…"
cargo test --all --quiet

# --- bump, commit, tag, push ------------------------------------------------

# Only the [workspace.package] line, which is the first `version = ` in the file.
python3 - "$VERSION" <<'PY'
import pathlib, re, sys

version = sys.argv[1]
path = pathlib.Path("Cargo.toml")
text = path.read_text()

new, count = re.subn(
    r'(?m)^version = "[^"]*"',
    f'version = "{version}"',
    text,
    count=1,
)
if count != 1:
    sys.exit("error: could not find the workspace version line")

path.write_text(new)
print(f"Cargo.toml: version -> {version}")
PY

git add Cargo.toml
git commit -q -m "Release $tag"
git tag -a "$tag" -m "$tag"

echo "pushing…"
git push origin "$branch"
git push origin "$tag"

echo
echo "done. $tag pushed — the release workflow will build the binaries."
echo "watch: https://github.com/mahardikalgw/ratchet-harness/actions/workflows/release.yml"
