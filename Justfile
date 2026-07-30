# pieces — task runner. `just` with no arguments lists everything.
#
# Install the toolchain this expects:  just setup

default:
    @just --list --unsorted

# --- everyday ---

# Fast type-check of everything, including tests.
check:
    cargo check --workspace --all-features --all-targets

# Watch and re-check on save.
watch:
    bacon check-all

# Run the whole test suite.
test:
    cargo nextest run --workspace --all-features
    cargo test --workspace --all-features --doc

# Format, then lint as CI does.
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-features --all-targets -- -D warnings

fmt:
    cargo fmt --all

# Everything CI runs, in CI's order. Run before pushing.
ci: lint test audit
    @echo "ok"

# --- hygiene ---

# Licenses, advisories, banned crates, unexpected sources.
audit:
    cargo deny check

# Dependencies declared but never used.
unused:
    cargo shear

docs:
    cargo doc --workspace --all-features --no-deps --open

# --- release ---

# Deliberately plain git + perl rather than cargo-release: these crates are
# never published to a registry, so a release is only a version bump and an
# annotated tag. One less tool whose CLI can change under you.

# Tag a new version of the stack, e.g. `just release 0.2.0`.
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "working tree is dirty; commit or stash first" >&2
        exit 1
    fi
    # [workspace.package] version, and the version= on every internal path dep.
    perl -0pi -e 's/(\[workspace\.package\]\nversion = ")[^"]+/${1}{{version}}/' Cargo.toml
    perl -pi -e 's/^(pc-[a-z]+ = \{ path = "crates\/pc-[a-z]+", version = ")[^"]+/${1}{{version}}/' Cargo.toml
    # Templates pin a pieces tag; a release should hand out the new one.
    perl -pi -e 's/^(default = ")v[0-9.]+/${1}v{{version}}/' templates/*/cargo-generate.toml
    cargo check --workspace --quiet
    git commit -am "release v{{version}}"
    git tag -a "v{{version}}" -m "v{{version}}"
    echo "tagged v{{version}} — now: git push origin main --tags"

# --- consumer-side helpers ---
#
# These run in a *project* repo, not here. Copied into the project templates;
# kept here so there is one definition to fix.

# Point this project at your local pieces checkout.
link path="../pieces":
    #!/usr/bin/env bash
    set -euo pipefail
    if grep -q '^\[patch\.' Cargo.toml; then
        echo "already linked"
        exit 0
    fi
    cat >> Cargo.toml <<EOF

    # LOCAL ONLY — remove before committing. See \`just unlink\`.
    [patch."https://github.com/tess-fun/pieces"]
    pc-error = { path = "{{path}}/crates/pc-error" }
    pc-config = { path = "{{path}}/crates/pc-config" }
    pc-telemetry = { path = "{{path}}/crates/pc-telemetry" }
    pc-testkit = { path = "{{path}}/crates/pc-testkit" }
    EOF
    echo "linked to {{path}}"

# Remove the local patch block.
unlink:
    #!/usr/bin/env bash
    set -euo pipefail
    perl -0pi -e 's/\n# LOCAL ONLY.*?\n(?=\n|\z)//s' Cargo.toml
    # No `cargo update` here: the lockfile refreshes itself on the next build,
    # and forcing a fetch makes `unlink` fail when you are offline or
    # unauthenticated — exactly when you most need it to work.
    echo "unlinked"

# --- setup ---

# Install the cargo subcommands the recipes above expect. Slow the first time.
setup:
    cargo install cargo-nextest cargo-deny cargo-shear bacon --locked
    @echo "also useful: cargo install cargo-generate dist --locked"
