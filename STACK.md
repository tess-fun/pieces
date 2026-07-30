# The Stack

A reusable Rust foundation for diverse projects. Shared libraries live in one
workspace (`pieces`), consumed by project repos via tagged git deps.

Crate prefix: `pc-` (rename if you want a different name — do it now, not later).

---

## Repo topology

```
tess-fun/
├── pieces/                  # THE stack. One repo, cargo workspace, tagged releases.
│   ├── Cargo.toml           # [workspace.dependencies] + [workspace.lints]
│   ├── crates/
│   │   ├── pc-error/        # layer 0 — built
│   │   ├── pc-config/       #         — built
│   │   ├── pc-telemetry/    #         — built
│   │   ├── pc-testkit/      #         — built
│   │   ├── pc-service/      # layer 1 kits — not yet, see Sequencing
│   │   ├── pc-cli/
│   │   ├── pc-store/
│   │   └── pc-app/
│   ├── templates/           # cargo-generate templates — bin, lib
│   └── .github/workflows/
│       └── rust-ci.yml      # reusable workflow, called by every project
└── project-a/               # consumer. git dep on pieces @ tag.
    project-b/
```

**Why the templates and the reusable workflow live in `pieces` rather than
their own repos:** a project pins `pieces` at a tag for its crates anyway. Putting the
workflow and the templates at that same tag means CI, the templates, and the
libraries all move together — one bump, not three — and there is no extra repo
to keep in sync. Separate repos only earn their keep once you have non-Rust
projects that need the same workflows.

**Why one repo for all libs, not one repo per lib:** atomic cross-crate refactors,
one CI, one version tag, one `cargo update`. The cost — you can't version
`pc-error` independently of `pc-cli` — does not matter for a solo maintainer.
Revisit only if an outside consumer appears.

### Consuming the stack

```toml
# project-a/Cargo.toml
[dependencies]
pc-config    = { git = "https://github.com/tess-fun/pieces", tag = "v0.4.0" }
pc-telemetry = { git = "https://github.com/tess-fun/pieces", tag = "v0.4.0" }
```

All `pieces` crates must be on the **same tag** — mixing tags gives you two
copies of `pc-error` in the graph and confusing trait-mismatch errors.

### Iterating on the stack from inside a project

The move that makes git deps feel like path deps:

```toml
# project-a/Cargo.toml — bottom of file
[patch."https://github.com/tess-fun/pieces"]
pc-error     = { path = "../pieces/crates/pc-error" }
pc-config    = { path = "../pieces/crates/pc-config" }
pc-telemetry = { path = "../pieces/crates/pc-telemetry" }
```

Edit the lib, `cargo run` in the project, see it immediately. Commit + tag
`pieces`, bump the tag, delete the `[patch]` block before merging. Put that
block behind a `just link` / `just unlink` recipe so you never ship it by
accident, and add a CI check that fails if `[patch.` appears in a project
`Cargo.toml` on the default branch.

---

## Public stack, private projects

`pieces` is **public**; every project that consumes it is **private**. That
split is deliberate, and it removes the single most annoying failure mode of a
shared-library setup.

### Why not make the stack private too

A project's default `GITHUB_TOKEN` is scoped to that project's repo only. If
`pieces` were private, every project's CI would fail at `cargo fetch` with a
404 on a repo you can open fine in a browser, and the fix would be a
fine-grained PAT pasted into each project repo as an Actions secret, re-pasted
on expiry.

A free GitHub organization does **not** fix that: org-level secrets are not
accessible by private repositories on GitHub Free. Only GitHub Team and above.

Meanwhile the contents of layer 0 are a `Code` enum, some figment layering, a
`Secret` newtype, and `tracing` setup — no business logic, no domain knowledge,
nothing worth hiding. Making the stack public costs nothing real and deletes
the whole auth problem.

Bonus: public repos get **unlimited free Actions minutes**. Private repos bill
against 2,000/month. `pieces` is the repo you will iterate on hardest, and its
CI now costs nothing out of the budget your projects share.

### If a future crate really is proprietary

Split it out into a second, private `pieces-private` repo at that point — not
pre-emptively. The reusable CI workflow already accepts an optional
`stack-token` secret for exactly this, and skips the auth step when it is
absent. Wire it up the day you need it:

```yaml
jobs:
  ci:
    uses: tess-fun/pieces/.github/workflows/rust-ci.yml@v0.1.0
    secrets:
      stack-token: ${{ secrets.PIECES_TOKEN }}
```

### Still worth setting locally

```toml
# ~/.cargo/config.toml
[net]
git-fetch-with-cli = true
```

Not needed for `pieces` any more, but cargo's built-in git client handles SSH
agents and credential helpers poorly, and you will hit that the first time you
add any private git dependency.

### Keeping Actions minutes down in private project repos

Rust CI is minute-hungry and projects still bill against the 2,000. The shared
workflow already does the first two; the rest is per-project:

- `Swatinem/rust-cache` on every job
- `cargo-nextest` instead of `cargo test`
- fmt + clippy + test on PRs; a full target matrix **only on tag**
- `concurrency: { group: ${{ github.ref }}, cancel-in-progress: true }`

### Smaller notes

- **Dependency bumps.** Renovate handles cargo git-tag deps; Dependabot's
  coverage of them is unverified. Do not depend on either — `just bump-stack`
  rewriting the tag across projects is the reliable path, automated PRs a bonus.
- **`cargo-dist` installers** pull release assets from the repo. Private
  project repos mean an install script needs a token; only relevant when you
  ship binaries to someone other than yourself.

---

## Layer 0 — Foundation

Every project gets these. Universal, stable, worth building before you have a
second consumer.

| Concern | Crates | Your wrapper |
|---|---|---|
| Errors (libs) | `thiserror` | — |
| Errors (bins) | `anyhow`, or `color-eyre` / `miette` for pretty reports | `pc-error` |
| Config | `figment` (layered) + `serde` | `pc-config` |
| Logging / tracing | `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `opentelemetry-otlp` | `pc-telemetry` |
| Metrics | `metrics` + `metrics-exporter-prometheus` | `pc-telemetry` |
| Serialization | `serde`, `serde_json`, `toml` | — |
| Time | `jiff` (pick one and never mix) | — |
| IDs | `uuid` v7, or `ulid` | — |
| Secrets in memory | `zeroize` | `pc-config::Secret` |
| Async runtime | `tokio` | — |
| Testing | `insta`, `rstest`, `proptest`, `assert_cmd`, `predicates`, `wiremock`, `testcontainers` | `pc-testkit` |

### `pc-error`

- A `Code` enum (`NotFound`, `Invalid`, `Conflict`, `Internal`, `Unauthorized`, …)
  that is the **single vocabulary** all your errors map into.
- `impl From<Code> for http::StatusCode` (feature-gated) and
  `impl From<Code> for ExitCode`. This is the payoff: define the failure once,
  every surface renders it correctly.
- A `Report` type for user-facing rendering with a cause chain.
- Rule: libraries return `thiserror` enums that carry a `Code`. Binaries use
  `anyhow`/`eyre` and only translate at the boundary.

### `pc-config`

- One function: `pc_config::load::<MyConfig>(app_name)`, or `Loader` when you
  need to add CLI overrides or point at an explicit file.
- Fixed precedence: defaults → `/etc/<app>/config.toml` → per-user config dir →
  `./<app>.toml` → `./config.toml` → explicit `--config` file → `<APP>_*` env
  vars → CLI overrides.
- `load_with_origins` reports which layers actually contributed. "Which file
  did that setting come from" is the question you will ask most often.
- `Secret` (a `zeroize::Zeroizing<String>` newtype, not `secrecy` — fewer deps
  and a more stable API) has no `Display`, redacts in `Debug` and `Serialize`,
  and zeroes on drop.
- `to_redacted_json` is the whole implementation of `--print-config`: safe by
  construction, because the redaction lives in `Secret`'s own `Serialize`.

### `pc-telemetry`

- `pc_telemetry::init(&TelemetryConfig) -> Guard`. One line in every `main`.
- Auto-selects human-readable output on a TTY, JSON otherwise.
- `RUST_LOG` honored; OTLP export enabled by env var, off by default.
- Installs a panic hook that logs through `tracing` before aborting.
- The `Guard` flushes spans on drop — otherwise you lose the last traces on exit.

### `pc-testkit`

- `insta` settings (snapshot dir, redactions for UUIDs/timestamps).
- `#[pc_test]` macro: builds a tokio runtime, initializes capture-mode telemetry,
  loads a test config.
- `TempDb` helper (testcontainers Postgres, migrations applied, dropped after).
- Deterministic clock and ID generator for reproducible snapshots.

---

## Layer 1 — Domain kits

Pick per project. **Do not write these before two projects need them.**

### Service kit → `pc-service`

| Concern | Choice | Note |
|---|---|---|
| HTTP server | `axum` | Tower ecosystem, minimal magic |
| Middleware | `tower-http` | trace, cors, compression, timeout, request-id, body-limit |
| Database | `sqlx` | Compile-time-checked SQL + built-in migrations. Prefer over an ORM. |
| ORM (if you must) | `sea-orm` | Only when the schema is genuinely dynamic |
| Validation | `garde` | Derive-based; `validator` is the older alternative |
| OpenAPI | `utoipa` or `aide` | `aide` integrates more naturally with axum |
| Auth | `jsonwebtoken`, `argon2`, `oauth2` | Never hand-roll password hashing |
| Cache | `moka` (in-process), `fred` (Redis) | |
| Background jobs | `apalis`, or a Postgres-backed queue | Postgres queue is usually enough |
| Rate limit | `tower_governor` | |

`pc-service` provides an `App::builder()` that pre-wires: telemetry layer,
request-id propagation, `/healthz` + `/readyz`, graceful shutdown on SIGTERM,
`pc-error` → HTTP response mapping, panic-to-500 catch, and a standard JSON
error envelope. A new service should be ~40 lines of `main.rs`.

### CLI kit → `pc-cli`

| Concern | Choice |
|---|---|
| Arg parsing | `clap` (derive) |
| Completions / man | `clap_complete`, `clap_mangen` |
| Diagnostics | `miette` |
| Color | `owo-colors` + `anstream` (respects `NO_COLOR`, pipes) |
| Tables | `comfy-table` |
| Progress | `indicatif` |
| Prompts | `inquire` |
| Release | `cargo-dist` (now `dist`) |

`pc-cli` provides a `#[derive(GlobalArgs)]` giving every binary `--verbose`,
`--quiet`, `--json`, `--config`, `--no-color`; a `Output` trait with
`human()` / `json()` so every command is scriptable for free; and exit-code
mapping from `pc_error::Code`.

### Desktop kit → `pc-app`

Three real options, genuinely different:

- **Tauri v2** — web frontend in a system webview. Best if the UI is web-shaped
  and you want polish + mobile targets. Cost: you're maintaining a JS frontend.
- **egui / eframe** — pure Rust, immediate mode. Best for tools, dashboards,
  internal apps. Fast to write, looks like a tool, not a product.
- **Dioxus** — RSX in Rust, desktop/web/mobile. One language, less mature than
  either of the above.

Recommendation: **egui for anything internal, Tauri when it ships to users.**
Don't try to abstract over both.

`pc-app` covers what's shared regardless: settings persistence in the correct
per-OS dir (`directories`), file logging with rotation, crash reporting,
updater wiring.

### Data kit → `pc-store` / `pc-pipeline`

| Concern | Choice |
|---|---|
| Dataframes | `polars` |
| Columnar / interchange | `arrow`, `parquet` |
| Embedded analytics | `duckdb` |
| Object storage | `object_store` — one API over S3/GCS/Azure/local. Use it always. |
| Embedded KV | `redb` or `fjall` |

`pc-store` wraps `object_store` with your URL conventions and credential loading
so a project never touches provider-specific config.

---

## Layer 2 — Cross-cutting

| Concern | Crate |
|---|---|
| HTTP client | `reqwest` + `reqwest-middleware` (retry, tracing) |
| Retry / backoff | `backon` |
| CPU parallelism | `rayon` |
| TLS | `rustls` (set it as the default across the workspace; avoid `openssl`) |
| Compression | `flate2`, `zstd` |
| Templating | `minijinja` |
| Regex | `regex` |
| CLI-visible paths | `directories`, `camino` (`Utf8PathBuf` — worth it) |

Pin `rustls` as the TLS backend in `[workspace.dependencies]` with
`default-features = false`. Mixed `openssl`/`rustls` graphs are a recurring
cross-compilation tax.

---

## Layer 3 — Tooling (where the speed actually comes from)

This layer is worth more than any library. Build it first.

| Tool | Purpose |
|---|---|
| `cargo-generate` | `cargo generate --git .../pieces templates/bin` → running project |
| `just` | Task runner. `just test`, `just lint`, `just link`, `just release` |
| `cargo-nextest` | Faster, better test output, real per-test isolation |
| `bacon` | Background `cargo check` on save (replaces `cargo-watch`) |
| `cargo-deny` | License + advisory + duplicate-dep gate in CI |
| `cargo-shear` | Finds unused dependencies |
| `cargo-release` | Tag + changelog for the `pieces` workspace |
| `dist` (`cargo-dist`) | Cross-platform binary releases + installers |
| `lefthook` | Pre-commit fmt/clippy |
| `renovate` | Dependency PRs against `pieces` only; projects follow via tag bump |

### Workspace-level enforcement

Put this in `pieces/Cargo.toml` **and** in every project's root — this is how you
get consistency without discipline:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
todo = "warn"
```

Plus a pinned `rust-toolchain.toml`, a shared `rustfmt.toml`, and a
`deny.toml`. The template ships all four.

### CI

One reusable workflow, versioned alongside the crates it builds:

```yaml
jobs:
  ci:
    uses: tess-fun/pieces/.github/workflows/rust-ci.yml@v0.1.0
    secrets:
      stack-token: ${{ secrets.PIECES_TOKEN }}
```

Jobs: `lint` (fmt + clippy), `test` (nextest + doctests), `audit` (cargo-deny),
and `no-local-patch` — which fails the build if a `just link` `[patch]` block
was committed, because otherwise CI silently tests code that is not what ships.

Fix the workflow once; every project picks it up on its next tag bump.

---

## Sequencing

The failure mode here is building all of the above before writing a real
project, and encoding guesses as API. Concretely:

1. ~~`pieces` with `pc-error`, `pc-config`, `pc-telemetry`, `pc-testkit`, plus
   the whole of Layer 3.~~ **Done — `v0.1.0`.**
2. ~~The `cargo-generate` templates, the reusable CI workflow, the `Justfile`.~~
   **Done — `v0.2.0`.** This is the compounding part.
3. **Project #1** — `cargo generate ... templates/bin`, then build it normally.
   Anything reusable stays *in the project*.
4. **Project #2** — when you reach for something you wrote in #1, *then* extract
   it to `pieces` and add a domain template. Rule of three for anything
   opinionated.
5. Layer 1 kits emerge from steps 3–4. They will be better for having had two
   real consumers first.

Building the `bin` template was itself step 2's real payoff: as the first
consumer of Layer 0 it immediately exposed a missing piece — there was no
ergonomic way to get a `Coded` error through a `?`, because a blanket
`From<E> for Report` collides with core's reflexive impl. That became
`pc_error::ResultExt` (`.classify()` / `.context(..)`) in `v0.2.0`. Two more
consumers will find two more of those.

Layer 0 is exempt from rule-of-three because error/config/telemetry shape is
knowable in advance and every project needs all three on day one.
