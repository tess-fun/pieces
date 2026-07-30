# Changelog

Consumers move between versions by bumping a git tag, so this file exists to
answer one question: *will this break me?*

Every entry states that explicitly. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions are
[semver](https://semver.org/), with the pre-1.0 caveat that a minor bump may
break.

## Unreleased

## [0.2.2] — 2026-07-30

**Breaking: no.**

### Changed

- `templates/bin` renders `--print-config` straight from `Config` instead of a
  parallel `PrintableConfig` struct. Safe because `Secret` redacts itself in its
  own `Serialize` impl; the mirror struct would have drifted the first time a
  config field was added. 20 lines gone.
- Dropped `anyhow`, `toml`, and `camino` from `[workspace.dependencies]` — no
  crate used them. An unlisted-but-unused entry is never resolved, so its
  version drifts: the declared `toml = "0.9"` had already fallen out of step
  with the `0.8` that figment pulls, and the first crate to opt in would have
  tripped cargo-deny's `multiple-versions` check for nothing.

## [0.2.1] — 2026-07-30

**Breaking: no.** Fixes noise that every consumer saw.

### Fixed

- Template manifests renamed to `Cargo.toml.liquid`. Cargo scans every manifest
  in a git dependency's repo, so the literal `name = "{{project-name}}"` printed
  ``error: invalid character `{` in package name`` on *every consumer's* fetch.
  The build still succeeded, which is what made it insidious. A workspace
  `exclude` does not suppress this; the rename does, and `cargo-generate` strips
  the extension after rendering.

## [0.2.0] — 2026-07-30

**Breaking: no.** Additive.

### Added

- `pc_error::ResultExt` — `.classify()` and `.context(..)` to carry a `Coded`
  error through `?`. A blanket `impl<E: Coded> From<E> for Report` is impossible
  (it collides with core's reflexive `From<Report> for Report`), so an extension
  trait is the ergonomic route.
- `cargo-generate` templates in `templates/`: `bin` (CLI with layered config,
  telemetry, `--print-config`, exit codes from `pc_error::Code`, and eight
  integration tests) and `lib` (library crate with a `Coded` error enum).

Templates and the reusable CI workflow live inside this repo on purpose: a
project pins one tag and gets libraries, CI, and scaffolding that move together.

## [0.1.0] — 2026-07-28

Initial release. Layer 0 and the tooling layer.

### Added

- `pc-error` — a `Code` vocabulary mapping to HTTP status, `sysexits` exit
  codes, and retry policy, plus a `Report` boundary type that keeps the cause
  chain while `Display` shows only the top message.
- `pc-config` — figment layering with a fixed precedence and an origins report;
  `Secret` has no `Display`, redacts in `Debug` and `Serialize`, and zeroes on
  drop.
- `pc-telemetry` — one-line `tracing` init, format chosen by whether stderr is a
  TTY, `RUST_LOG` precedence, panic capture.
- `pc-testkit` — idempotent tracing capture, filesystem `Sandbox`, deterministic
  `Seq` and a manually advanced `Clock`.
- Tooling: pinned toolchain, workspace lints forbidding `unsafe`/`unwrap`/
  `expect`, `clippy.toml` relaxing those in tests, `deny.toml`, `Justfile`, and
  a `workflow_call` CI definition.

[0.2.2]: https://github.com/tess-fun/pieces/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/tess-fun/pieces/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/tess-fun/pieces/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/tess-fun/pieces/releases/tag/v0.1.0
