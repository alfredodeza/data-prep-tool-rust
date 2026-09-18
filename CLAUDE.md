# CLAUDE.md

Guidance for Claude Code (and any contributor) when working in this repository.

## Core workflow: Test-Driven Development

This project follows strict TDD. For every fix or improvement:

1. **Write a failing test first.** Before changing any implementation code, add or
   modify a test in `tests/` (or a `#[cfg(test)]` module) that captures the bug or
   the new behavior. Run `cargo test` and confirm it fails for the expected reason.
2. **Make the minimal change** to `src/` needed to make that test pass.
3. **Run `cargo test` again** and confirm the full suite passes.

Never write implementation code before there is a failing test that justifies it.

### Never add a feature without a test

Every new feature, flag, or code path must ship with a test that exercises it.
If a change has no test covering it, it is not done. Bug fixes need a regression
test that fails without the fix and passes with it.

## Required checks before considering any change complete

After tests pass, always run the linter and formatter, and confirm the project
compiles cleanly:

```sh
cargo fmt          # format the code
cargo clippy --all-targets -- -D warnings   # lint, warnings denied
cargo build        # confirm it compiles
cargo test         # confirm the full suite passes
```

Or simply use the Makefile target that bundles these:

```sh
make check   # fmt-check + lint + test
```

If `make check` reports formatting issues, run `make fmt` to fix them, then
re-run `make check`.

## Summary of rules

- TDD only: failing test → implementation → passing test. No exceptions.
- No feature or fix lands without a test that would fail without it.
- Always run `cargo fmt` and `cargo clippy -- -D warnings` before finishing.
- Always confirm `cargo build` (and `cargo test`) succeed before finishing.
- Prefer `make check` as the single pre-commit gate.
