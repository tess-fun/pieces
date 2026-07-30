# Templates

Starting points for new projects. Each one arrives lint-clean, CI-wired, and
already depending on a tagged `pieces`.

```bash
cargo install cargo-generate --locked   # once

cargo generate --git https://github.com/tess-fun/pieces templates/bin --name my-tool
cargo generate --git https://github.com/tess-fun/pieces templates/lib --name my-lib
```

| Template | For |
|---|---|
| `bin` | A CLI binary. Layered config, telemetry, `--print-config`, classified exit codes, end-to-end tests against the built binary. |
| `lib` | A library crate. A `Coded` error enum and the strict lint set. |

Both prompt for a description, the `pieces` tag to pin, and (for `bin`) an
environment-variable prefix.

## What `bin` gives you working on the first run

- `my-tool print-config` — the resolved configuration with secrets redacted.
  The first thing to reach for when two machines behave differently.
- Precedence that already works: defaults → `config.toml` → `PREFIX_*` env →
  CLI flags. Verified by the generated integration tests, not just documented.
- Exit codes a caller can branch on: `64` bad usage, `66` missing input, `70`
  our bug — derived from `pc_error::Code`, so you never hand-maintain them.
- Logs on stderr, output on stdout, so `my-tool | jq` works with logging on.
- `just link` / `just unlink` to develop against a local `pieces` checkout, and
  `just bump-stack v0.3.0` to move to a new release.

## Two templating gotchas, already handled

Both files below would break if templated naively, so they are worth knowing
about before you add more templates:

- **`Justfile` is copied verbatim** (`exclude` in `cargo-generate.toml`). `just`
  uses `{{ }}` for its own variables and Liquid would eat them. The cost: no
  `cargo-generate` placeholders may appear in a Justfile.
- **`.github/workflows/ci.yml` wraps GitHub expressions in `{% raw %}`**, since
  `${{ github.ref }}` is also `{{ }}`. The `{{stack_tag}}` placeholder sits
  outside the raw block so it still gets substituted.

## Adding a template

Copy the closest existing one and verify it actually builds before committing —
a template that does not compile costs more than no template. The check that
catches real problems:

```bash
cargo generate --path templates/bin --name scratch-check --define description="x" \
  --define stack_tag=v0.2.0 --define env_prefix=SCRATCH
cd scratch-check && just ci
```
