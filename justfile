default:
    @just --list

# The full gate: formatting, then lints and tests for each feature set.
[doc("Format check, clippy and tests for each feature set")]
ci:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --all -- --check
    for features in "" --all-features --no-default-features; do
        cargo clippy --workspace --all-targets --locked $features -- -D warnings
        cargo nextest run --workspace --locked $features
        # nextest does not run doctests.
        cargo test --workspace --doc --locked $features
    done

# Cut a release: bump the workspace version and the bonfig-macros pin, run
# `ci` and a publish dry run, regenerate CHANGELOG.md with git-cliff (see
# cliff.toml), commit, tag `v<version>`, then run `publish`.
# Usage: `just release` picks the next version from the conventional commits
# since the last tag (`git cliff --bumped-version`); `just release 0.2.0` (or
# `v0.2.0`) sets it explicitly. Refuses to run with a dirty tree, an existing
# tag, or a version older than the current one.
[doc("Bump, changelog, commit, tag, then publish (VERSION: X.Y.Z, default from commits)")]
release VERSION="":
    #!/usr/bin/env bash
    set -euo pipefail
    test -z "$(git status --porcelain)" || { echo "dirty tree: commit or stash first" >&2; exit 1; }
    if git remote get-url origin >/dev/null 2>&1; then git fetch --tags --quiet origin; fi
    VER="{{VERSION}}"
    [[ -n "$VER" ]] || VER="$(git cliff --bumped-version 2>/dev/null)"
    VER="${VER#v}"
    [[ "$VER" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || { echo "not a version: '$VER' (want X.Y.Z)" >&2; exit 1; }
    CUR="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "bonfig") | .version')"
    # Equal is allowed: the current version may never have been tagged (the
    # tag check below is what stops re-releasing it).
    printf '%s\n%s\n' "$CUR" "$VER" | sort -VC || { echo "version $VER older than $CUR" >&2; exit 1; }
    ! git rev-parse -q --verify "refs/tags/v$VER" >/dev/null || { echo "tag v$VER already exists" >&2; exit 1; }
    # Bumps [workspace.package], the `=` pin on bonfig-macros, and Cargo.lock.
    # git-cliff writes only the changelog, so this step needs cargo-edit.
    cargo set-version --workspace "$VER"
    just ci
    cargo publish --workspace --locked --dry-run --allow-dirty
    git cliff --tag "v$VER" -o CHANGELOG.md
    git add Cargo.toml Cargo.lock CHANGELOG.md
    git commit -m "chore(release): v$VER"
    git tag "v$VER"
    just publish

# Publish both crates to crates.io (bonfig-macros first, cargo orders them),
# then push the commit and the tag. Run it alone to retry a failed publish.
# Needs `cargo login` first. HEAD must be the `v<version>` tag from `release`.
[doc("Publish both crates to crates.io and push the tag")]
publish:
    #!/usr/bin/env bash
    set -euo pipefail
    test -z "$(git status --porcelain)" || { echo "dirty tree: commit or stash first" >&2; exit 1; }
    VER="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "bonfig") | .version')"
    [[ "$(git rev-parse HEAD)" == "$(git rev-parse -q --verify "v$VER^{commit}" || true)" ]] \
        || { echo "HEAD is not tagged v$VER: run 'just release' first" >&2; exit 1; }
    cargo publish --workspace --locked
    if git remote get-url origin >/dev/null 2>&1; then
        git push origin HEAD "v$VER"
    else
        echo "no 'origin' remote: push the commit and tag v$VER yourself" >&2
    fi
