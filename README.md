# pieces

The shared Rust foundation. Every project depends on a tagged version of this
repo rather than reinventing error handling, config, and telemetry.

[STACK.md](STACK.md) is the design document — the full crate map, the repo
topology, and what deliberately does *not* live here yet.

## Crates

| Crate | What it gives you |
|---|---|
| [`pc-error`](crates/pc-error) | One `Code` vocabulary that maps to HTTP status, exit status, and retry policy; `ResultExt` to get there from a `?` |
| [`pc-config`](crates/pc-config) | Layered config with fixed precedence, plus a `Secret` that cannot leak |
| [`pc-telemetry`](crates/pc-telemetry) | One-line `tracing` setup: TTY-aware format, `RUST_LOG`, panic capture |
| [`pc-testkit`](crates/pc-testkit) | Test-only: tracing capture, filesystem sandbox, deterministic ids and clock |

## Starting a project

```bash
cargo generate --git https://github.com/tess-fun/pieces templates/bin --name my-tool
```

See [templates/](templates) for what you get. Both templates arrive lint-clean
and CI-wired.

## Using it from an existing project

```toml
[dependencies]
pc-config = { git = "https://github.com/tess-fun/pieces", tag = "v0.1.0" }
pc-error = { git = "https://github.com/tess-fun/pieces", tag = "v0.1.0" }
pc-telemetry = { git = "https://github.com/tess-fun/pieces", tag = "v0.1.0" }
```

Keep every `pieces` crate on the **same tag**. Mixing tags puts two
incompatible copies of `pc-error` in the graph, and the resulting trait
mismatch errors do not mention versions at all.

This repo is public so that private projects can depend on it with no
credentials at all — see
[STACK.md § Public stack, private projects](STACK.md#public-stack-private-projects).
CI for a consuming project is one job:

```yaml
jobs:
  ci:
    uses: tess-fun/pieces/.github/workflows/rust-ci.yml@v0.1.0
```

## Working on it

```bash
just setup   # install cargo-nextest, cargo-deny, cargo-shear, bacon
just         # list every task
just ci      # exactly what CI runs
```

To iterate on a `pieces` crate from inside a project, run `just link` in the
project — it points the dependency at your local checkout. `just unlink`
before committing; CI rejects a committed `[patch]` block.

## Releasing

```bash
# 1. add a `## [0.3.0]` section to CHANGELOG.md, stating whether it breaks
# 2. then:
just release 0.3.0
git push origin main --tags
```

`just release` refuses to tag without a matching changelog section, bumps the
workspace version and the templates' pinned tag, then commits and tags.

Consumers move when they bump their tag, never before. That is the point: a
change here cannot break a project that has not opted in.
