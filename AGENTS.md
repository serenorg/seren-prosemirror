# Repository Guidelines

## Scope

- This repository owns the `seren-prosemirror` Rust library: ProseMirror JSON validation, plain-text extraction, and conversion between ProseMirror JSON and GitHub Flavored Markdown, plus Org-mode behind the optional `orgmode` feature.
- It is a library crate with no binary, service, database, or deployment. Application editors, storage, rendering, and HTML sanitization belong to the Seren applications that depend on this crate.
- Keep the shared document schema consistent across Markdown, Org-mode, validation, and plain-text extraction. Complete schema changes across every converter, the validator, tests, and `README.md`, and do not retain compatibility fallbacks after a cutover.
- Treat input documents and Markdown or Org text as untrusted. Keep recursion bounded by `MAX_DEPTH` and return `ProseMirrorError` instead of panicking.

## Structure

- The crate entry point is `src/lib.rs`, which defines the `ProseMirror` converter and the public exports.
- Markdown conversion is in `src/markdown.rs`, Org-mode conversion in `src/orgmode.rs` (compiled only with `orgmode`), validation in `src/schema.rs`, plain-text extraction in `src/text.rs`, attachment URI handling in `src/attachment.rs`, and errors in `src/error.rs`.
- Unit tests are in `src/tests.rs`. Doctests are in the `src/lib.rs` crate documentation.
- CI is `.github/workflows/ci.yml`. Local hooks are in `.pre-commit-config.yaml`. There is no Makefile and no generated code, and `Cargo.lock` is not committed.

## Validation

- Run the CI checks: `cargo fmt --all -- --check`, `cargo check --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features`.
- Check the default feature set with `cargo clippy --all-targets -- -D warnings` and `cargo test`, and the minimum supported Rust version with `cargo +1.88.0 check --all-features`.
- Build the docs with `RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps`, and scan dependencies with `cargo deny -L error check advisories bans sources`.
- Install the hooks with `prek install` and run them with `prek run --all-files`.
- Test public behavior through the `ProseMirror` API, `validate_document`, and `extract_plain_text`. Name tests after the behavior they prove, not an issue, incident, review, or removed implementation. Do not read source or documentation files to assert implementation text.
- Report the exact checks run and any remaining risks.

## Security and Operations

- The crate handles no secrets, credentials, or network access, and stores no data. Keep `unsafe_code = "forbid"` in `Cargo.toml`.
- Converted output passes link and image URL schemes through unchanged and is not safe to render as trusted HTML. Preserve the `README.md` security notice when changing escaping or validation.
- Releases are Cargo package versions. Keep `rust-version` in `Cargo.toml` aligned with the CI minimum supported Rust version check.
- Keep comments concise. Explain invariants and non-obvious constraints, not issue history or implementation narration.

## Changes and Commits

- Keep changes scoped and preserve unrelated work. Use existing patterns and avoid new dependencies without a clear rationale.
- Use Conventional Commits with a concise single-line subject. Do not include issue numbers, ticket URLs, personal signatures, co-author trailers, generated-by notices, or tool attribution.
- Do not put issue or pull request numbers, URLs, or commit hashes as history in code, comments, or test names.
- Never bypass hooks with `git commit --no-verify`.

## Git Remotes

- Remote publication is blocked by default and remains owned by the user.
- An agent may publish one ordinary, non-force branch ref only when the user explicitly authorizes that exact push operation in the current turn.
- The exact authorization syntax is a single standalone line:
  `AUTHORIZE_GIT_PUSH_ONCE: <exact direct git push command>`.
  It authorizes only that command, in that working directory, once during the current turn.
- Force pushes, tag pushes, mirror pushes, remote-ref deletions, and multi-ref pushes remain prohibited.
- Local commits are allowed when requested.
