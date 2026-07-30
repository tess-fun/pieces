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
    uses: tess-fun/pieces/.github/workflows/rust-ci.yml@v0.2.2
```

No secrets: `pieces` is public. Pass `secrets: stack-token: …` only if the
project also depends on a private repo.

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
| Errors (bins) | `pc_error::Report` + `ResultExt` — no `anyhow` needed; `miette` only if you want source-span diagnostics | `pc-error` |
| Config | `figment` (layered) + `serde` | `pc-config` |
| Logging / tracing | `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `opentelemetry-otlp` | `pc-telemetry` |
| Metrics | `metrics` + `metrics-exporter-prometheus` | `pc-telemetry` |
| Serialization | `serde`, `serde_json` (figment already parses TOML, so you rarely need `toml` directly) | — |
| Time | `jiff` (pick one and never mix) | — |
| IDs | `uuid` v7, or `ulid` | — |
| Secrets in memory | `zeroize` | `pc-config::Secret` |
| Async runtime | `tokio` | — |
| Testing | `insta`, `rstest`, `proptest`, `assert_cmd`, `predicates`, `wiremock`, `testcontainers` | `pc-testkit` |

### `pc-error`

- A `Code` enum — `Invalid`, `Unauthenticated`, `Forbidden`, `NotFound`,
  `Conflict`, `Exhausted`, `Unavailable`, `Timeout`, `Internal` — the **single
  vocabulary** every error in the stack maps into.
- `Code::status()` → `http::StatusCode` (behind the `http` feature) and
  `Code::exit_code()` → `u8` following `sysexits.h`. Plus `is_retryable()` and
  `is_client_fault()`, so backoff policy and alerting severity both fall out of
  the same classification. That is the payoff: classify once at the origin, and
  every surface renders it correctly.
- `Report` is the boundary type. `Display` shows only the top message (safe in
  an API response body); `chained()` renders the whole cause chain (what you
  log or print).
- Rule: libraries return `thiserror` enums implementing `Coded`. Binaries carry
  `Report` and reach it with `ResultExt::classify()` / `.context("…")`. A
  blanket `From<E: Coded> for Report` is impossible — it collides with core's
  reflexive `From<Report> for Report` — which is why the extension trait exists.

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

Shipped:

- `pc_telemetry::init(&Config) -> Guard`. One line in every `main`.
- Human-readable output on a TTY, JSON otherwise — so a container log is
  structured without anyone remembering a `--log-format` flag.
- `RUST_LOG` takes precedence over the configured filter, one-directionally, so
  an operator can always turn logging up on a running deployment.
- Logs to stderr, keeping stdout machine-readable for CLIs.
- A panic hook that logs through `tracing`, with location and payload, before
  deferring to the previous hook.
- `Config` derives `Deserialize`, so it nests inside an app's own config struct.

Not yet built:

- **OTLP export.** `Guard` exists and is documented as load-bearing precisely so
  that adding this later does not touch a single `main`. Today its drop is a
  no-op.
- **Metrics.** `metrics` + a Prometheus exporter, when something needs them.

### `pc-testkit`

Shipped:

- `trace()` — idempotent tracing into libtest's capture buffer. Safe to call
  from every test.
- `Sandbox` — a self-cleaning temp dir with `write`/`read`/`path`, plus
  `persist()` for debugging a failure.
- `Seq` — deterministic ids, so snapshots do not change every run.
- `Clock` — frozen time that only moves when a test moves it.

Not yet built, and deliberately so — each needs a real consumer to shape it:

- `insta` settings (snapshot dir, redactions for uuids/timestamps).
- A `#[pc_test]` attribute macro (tokio runtime + telemetry + test config).
- `TempDb` (testcontainers Postgres with migrations applied). Waits on the
  service kit; a DB helper with no service to test is a guess.

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

In use. `just setup` installs all of these; `just` itself is the one
bootstrap prerequisite (`brew install just`).

| Tool | Purpose |
|---|---|
| `cargo-generate` | `cargo generate --git .../pieces templates/bin` → running project |
| `just` | Task runner. `just ci`, `just link`, `just release`, `just bump-stack` |
| `cargo-nextest` | Faster, better output, real per-test process isolation |
| `bacon` | Background `cargo check` on save (replaces `cargo-watch`) |
| `cargo-deny` | License + advisory + banned-crate + source gate, run in CI |
| `cargo-shear` | Finds declared-but-unused dependencies |

Deliberately **not** used:

- **`cargo-release`.** These crates never reach a registry, so a release is only
  a version bump and an annotated tag. `just release` does that in plain git and
  perl — one less tool whose CLI can change underneath you. It also refuses to
  tag without a matching `CHANGELOG.md` section.

Not yet needed, add when the situation arises:

- **`dist` (formerly `cargo-dist`)** — cross-platform binary releases and
  installers. Only once you ship a binary to someone other than yourself.
- **`lefthook`** — pre-commit fmt/clippy. CI already blocks bad pushes; this
  only shortens the feedback loop.
- **`renovate`** — dependency PRs. Point it at `pieces` only and let projects
  follow via `just bump-stack`. Renovate handles cargo git-tag deps;
  Dependabot's coverage of them is unverified.

### Workspace-level enforcement

Lints declared once at the root and inherited beat lints nobody remembers to
add. In a workspace use `[workspace.lints]` plus `[lints] workspace = true` in
each member; in a single-crate project use `[lints]` directly. The templates
ship the right form for each.

```toml
[lints.rust]
unsafe_code = "forbid"
missing_debug_implementations = "warn"   # libraries only
missing_docs = "warn"                    # libraries only
unreachable_pub = "warn"

[lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
todo = "warn"
dbg_macro = "warn"
print_stdout = "warn"                    # libraries only
print_stderr = "warn"                    # libraries only
module_name_repetitions = "allow"        # pedantic noise not worth the churn
missing_errors_doc = "allow"
```

The "libraries only" rows matter: printing is a *binary's whole job*, so the
`bin` template omits those two, and requiring docs on a binary's private
internals is busywork.

Two traps worth knowing:

- `unwrap_used` fires in tests too. Fix it in `clippy.toml` with
  `allow-unwrap-in-tests` (and the `expect`/`panic`/`print` equivalents), not
  with scattered `#[allow]`.
- That setting does **not** reach `tests/`. Integration tests compile as their
  own crate without `cfg(test)`, so clippy's in-test detection never fires —
  state `#![allow(clippy::unwrap_used, clippy::expect_used)]` at the top of each
  integration test file.

Plus a pinned `rust-toolchain.toml`, a shared `rustfmt.toml`, a `clippy.toml`,
and a `deny.toml`. Both templates ship all four.

### CI

One reusable workflow, versioned alongside the crates it builds:

```yaml
jobs:
  ci:
    uses: tess-fun/pieces/.github/workflows/rust-ci.yml@v0.2.2
```

No secrets: `pieces` is public. Pass `secrets: stack-token: …` only if the
project also depends on a private repo.

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
