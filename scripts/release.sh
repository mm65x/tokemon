#!/usr/bin/env bash
# Semi-automatic verification and release helper for tokemon.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
    cat <<'EOF'
Usage:
  scripts/release.sh check [--allow-dirty]        Run full local verification + publish dry-run
  scripts/release.sh rc <X.Y.Z-suffix>            Tag and push a pre-release from develop (no crates.io publish)
  scripts/release.sh prepare <patch|minor|major|X.Y.Z>
                                                  Create release/vX.Y.Z from develop, bump version, verify, and push PR
  scripts/release.sh publish [--yes]              Verify master CI, dry-run publish, confirm, and push vX.Y.Z tag
EOF
}

current_version() {
    python3 -c '
import re, sys
text = open("Cargo.toml", encoding="utf-8").read()
m = re.search(r"(?m)^version\s*=\s*\"([^\"]+)\"", text)
if not m:
    sys.exit("Could not find package version in Cargo.toml")
print(m.group(1))
'
}

set_cargo_version() {
    local next_version="$1"
    python3 - "$next_version" <<'PY'
import re, sys
next_ver = sys.argv[1]
path = "Cargo.toml"
text = open(path, encoding="utf-8").read()
updated, count = re.subn(
    r'(?m)^(version\s*=\s*")[^"]+(")',
    rf'\g<1>{next_ver}\g<2>',
    text,
    count=1,
)
if count != 1:
    sys.exit("Failed to update version in Cargo.toml")
open(path, "w", encoding="utf-8").write(updated)
PY
}

compute_next_version() {
    local current="$1"
    local spec="$2"
    python3 - "$current" "$spec" <<'PY'
import re, sys
current, spec = sys.argv[1], sys.argv[2]
m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?", current)
if not m:
    sys.exit(f"Unsupported current Cargo.toml version: {current}")
major, minor, patch = map(int, m.groups())

if spec == "patch":
    print(f"{major}.{minor}.{patch + 1}")
elif spec == "minor":
    print(f"{major}.{minor + 1}.0")
elif spec == "major":
    print(f"{major + 1}.0.0")
elif re.fullmatch(r"\d+\.\d+\.\d+", spec):
    print(spec)
else:
    sys.exit(f"Invalid version or bump spec '{spec}' (expected patch, minor, major, or X.Y.Z)")
PY
}

ensure_clean_tree() {
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "Error: working tree is not clean. Commit or stash changes first." >&2
        git status -sb >&2
        exit 1
    fi
}

run_checks() {
    local allow_dirty=0
    local run_publish_dry_run=1

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --allow-dirty)
                allow_dirty=1
                ;;
            --skip-publish-dry-run)
                run_publish_dry_run=0
                ;;
            *)
                echo "Unknown check option: $1" >&2
                exit 1
                ;;
        esac
        shift
    done

    echo "==> [check] cargo fmt -- --check"
    cargo fmt -- --check

    echo "==> [check] cargo clippy (pedantic, warnings denied)"
    cargo clippy -- -W clippy::pedantic -A clippy::module_name_repetitions -D warnings

    echo "==> [check] cargo test"
    cargo test

    if [[ "$(uname -s)" == "Darwin" ]] && command -v swift >/dev/null 2>&1; then
        echo "==> [check] swift test --package-path macos"
        swift test --package-path macos
    fi

    echo "==> [check] scripts/smoke-test.sh"
    ./scripts/smoke-test.sh

    echo "==> [check] cargo package --list"
    if [[ "$allow_dirty" -eq 1 ]]; then
        cargo package --list --allow-dirty >/dev/null
    else
        cargo package --list >/dev/null
    fi

    if [[ "$run_publish_dry_run" -eq 1 ]]; then
        echo "==> [check] cargo publish --dry-run"
        if [[ "$allow_dirty" -eq 1 ]]; then
            cargo publish --dry-run --allow-dirty
        else
            cargo publish --dry-run
        fi
    fi

    echo "==> All checks passed."
}

confirm_yes_no() {
    local prompt="$1"
    local reply=""
    read -r -p "$prompt [y/N] " reply
    [[ "$reply" =~ ^[Yy]$ ]]
}

cmd_check() {
    run_checks "$@"
}

cmd_rc() {
    if [[ $# -ne 1 ]]; then
        echo "Error: rc requires a pre-release version (e.g. 0.2.8-alpha.1)" >&2
        usage
        exit 1
    fi
    local version="$1"
    if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+-[0-9A-Za-z.-]+$ ]]; then
        echo "Error: pre-release version must match X.Y.Z-suffix (got '$version')" >&2
        exit 1
    fi

    local tag="v$version"
    ensure_clean_tree

    local branch
    branch="$(git branch --show-current)"
    if [[ "$branch" != "develop" ]]; then
        echo "Error: pre-releases must be tagged from 'develop' (current: '$branch')" >&2
        exit 1
    fi

    git fetch origin --tags
    if [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/develop)" ]]; then
        echo "Error: local 'develop' is not aligned with 'origin/develop'." >&2
        exit 1
    fi

    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null || \
       git ls-remote --tags origin "refs/tags/$tag" | grep -q .; then
        echo "Error: tag '$tag' already exists." >&2
        exit 1
    fi

    local crate_ver
    crate_ver="$(current_version)"
    if [[ "$crate_ver" == "$version" ]]; then
        run_checks
    else
        run_checks --skip-publish-dry-run
    fi

    local prev_tag
    prev_tag="$(git describe --tags --abbrev=0 2>/dev/null || true)"
    echo ""
    echo "Commits since ${prev_tag:-beginning}:"
    if [[ -n "$prev_tag" ]]; then
        git log "$prev_tag..HEAD" --oneline
    else
        git log -n 20 --oneline
    fi
    echo ""

    if ! confirm_yes_no "Push pre-release tag $tag to origin? (GitHub Release only, NO crates.io publish)"; then
        echo "Aborted."
        exit 1
    fi

    git tag -a "$tag" -m "Pre-release $tag"
    git push origin "$tag"
    echo "Pushed pre-release tag $tag."
}

cmd_prepare() {
    if [[ $# -ne 1 ]]; then
        echo "Error: prepare requires <patch|minor|major|X.Y.Z>" >&2
        usage
        exit 1
    fi

    ensure_clean_tree

    local branch
    branch="$(git branch --show-current)"
    if [[ "$branch" != "develop" ]]; then
        echo "Error: prepare must start from 'develop' (current: '$branch')" >&2
        exit 1
    fi

    git fetch origin --tags
    if [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/develop)" ]]; then
        echo "Error: local 'develop' is not aligned with 'origin/develop'." >&2
        exit 1
    fi

    local cur_ver next_ver
    cur_ver="$(current_version)"
    next_ver="$(compute_next_version "$cur_ver" "$1")"
    if [[ "$next_ver" == "$cur_ver" ]]; then
        echo "Error: target version '$next_ver' matches current Cargo.toml version." >&2
        exit 1
    fi

    local tag="v$next_ver"
    local rel_branch="release/$tag"

    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null || \
       git ls-remote --tags origin "refs/tags/$tag" | grep -q .; then
        echo "Error: tag '$tag' already exists." >&2
        exit 1
    fi

    if git rev-parse -q --verify "refs/heads/$rel_branch" >/dev/null || \
       git ls-remote --heads origin "$rel_branch" | grep -q .; then
        echo "Error: branch '$rel_branch' already exists." >&2
        exit 1
    fi

    echo "Preparing release $tag (from $cur_ver) on branch $rel_branch..."
    git checkout -b "$rel_branch"
    set_cargo_version "$next_ver"
    cargo check --quiet

    run_checks --allow-dirty

    git add Cargo.toml Cargo.lock
    git commit -m "chore(release): prepare $tag"
    ensure_clean_tree
    cargo publish --dry-run

    local prev_tag
    prev_tag="$(git describe --tags --abbrev=0 2>/dev/null || true)"
    echo ""
    echo "Release branch $rel_branch is ready. Commits since ${prev_tag:-beginning}:"
    if [[ -n "$prev_tag" ]]; then
        git log "$prev_tag..HEAD" --oneline
    else
        git log -n 20 --oneline
    fi
    echo ""

    if confirm_yes_no "Push branch $rel_branch to origin?"; then
        git push -u origin "$rel_branch"
        if command -v gh >/dev/null 2>&1; then
            if confirm_yes_no "Open release PR from $rel_branch to master via gh?"; then
                gh pr create \
                    --base master \
                    --head "$rel_branch" \
                    --title "chore(release): $tag" \
                    --body "Prepare release $tag."
            fi
        fi
    else
        echo "Release commit created locally on $rel_branch (not pushed)."
    fi
}

cmd_publish() {
    local auto_yes=0
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --yes)
                auto_yes=1
                ;;
            *)
                echo "Unknown publish option: $1" >&2
                exit 1
                ;;
        esac
        shift
    done

    ensure_clean_tree

    local branch
    branch="$(git branch --show-current)"
    if [[ "$branch" != "master" ]]; then
        echo "Error: publish must run from 'master' (current: '$branch')" >&2
        exit 1
    fi

    git fetch origin --tags
    local head_sha origin_sha
    head_sha="$(git rev-parse HEAD)"
    origin_sha="$(git rev-parse origin/master)"
    if [[ "$head_sha" != "$origin_sha" ]]; then
        echo "Error: local 'master' ($head_sha) does not match 'origin/master' ($origin_sha)." >&2
        exit 1
    fi

    local version
    version="$(current_version)"
    if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Error: stable release Cargo.toml version must be X.Y.Z (got '$version')" >&2
        exit 1
    fi

    local tag="v$version"
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null || \
       git ls-remote --tags origin "refs/tags/$tag" | grep -q .; then
        echo "Error: tag '$tag' already exists." >&2
        exit 1
    fi

    if command -v gh >/dev/null 2>&1; then
        echo "==> [publish] Checking GitHub Actions CI status on master ($head_sha)..."
        local ci_ok
        ci_ok="$(gh run list --branch master --commit "$head_sha" --workflow ci.yml --json status,conclusion --limit 5 | python3 -c '
import json, sys
runs = json.load(sys.stdin)
for run in runs:
    if run.get("status") == "completed" and run.get("conclusion") == "success":
        print("ok")
        sys.exit(0)
sys.exit(1)
' || true)"
        if [[ "$ci_ok" != "ok" ]]; then
            echo "Error: no passing ci.yml run found for master commit $head_sha." >&2
            exit 1
        fi
    fi

    run_checks

    echo ""
    echo "=== Ready to Publish ==="
    echo "  Version: $tag"
    echo "  Commit:  $(git log -1 --oneline)"
    echo "  Actions: Pushing $tag triggers GitHub Release binaries, crates.io publish, and Homebrew tap update."
    echo ""

    if [[ "$auto_yes" -ne 1 ]]; then
        local typed=""
        read -r -p "Type '$tag' to tag and push to origin: " typed
        if [[ "$typed" != "$tag" ]]; then
            echo "Confirmation mismatch. Aborted."
            exit 1
        fi
    fi

    git tag -a "$tag" -m "Release $tag"
    git push origin "$tag"
    echo "Pushed tag $tag."

    if ! git merge-base --is-ancestor "$head_sha" origin/develop 2>/dev/null; then
        echo ""
        echo "Note: origin/develop does not yet contain $tag from master."
        echo "Sync develop with:"
        echo "  git checkout develop && git merge --no-ff origin/master && git push origin develop"
    fi
}

cmd="${1:-}"
if [[ -z "$cmd" ]]; then
    usage
    exit 1
fi
shift

case "$cmd" in
    check)
        cmd_check "$@"
        ;;
    rc)
        cmd_rc "$@"
        ;;
    prepare)
        cmd_prepare "$@"
        ;;
    publish)
        cmd_publish "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo "Unknown subcommand: $cmd" >&2
        usage
        exit 1
        ;;
esac
