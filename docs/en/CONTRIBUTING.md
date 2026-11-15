# 🤝 Limiteron Contributing Guide

Thank you for your interest in the Limiteron project! This document describes how to take part in development and submit code.

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🛠️ Development Environment](#️-development-environment)
- [🔄 Development Workflow](#-development-workflow)
- [🪝 Pre-Commit Checks](#-pre-commit-checks)
- [📐 Code Style](#-code-style)
- [🔀 Pull Request Process](#-pull-request-process)
- [✉️ Commit Message Convention](#️-commit-message-convention)

</details>

---

## 🛠️ Development Environment

- **Rust 1.97.1+** (edition 2024, pinned by [rust-toolchain.toml](../../rust-toolchain.toml))
- `cargo`, `rustfmt`, `clippy`
- Git hook tooling (choose one; functionally equivalent):
  - [lefthook](https://github.com/evilmartians/lefthook): install with `lefthook install`
  - [pre-commit](https://pre-commit.com/): install with `pip install pre-commit && pre-commit install`
- (Optional) PostgreSQL 15+ / Redis 7+: for persistence- and cache-related integration tests

```bash
git clone https://github.com/Kirky-X/limiteron
cd limiteron
lefthook install
cargo test --workspace --no-default-features --features full
```

## 🔄 Development Workflow

Every development task should follow this test-first loop (Red → Green → Commit → Next):

1. **Define the interface**: define the trait / API signature first (`trait Xxx { ... }`), without an implementation
2. **Write tests**: write unit tests against the interface (`#[cfg(test)] mod tests { ... }`); they should fail at this point (red)
3. **Write code**: implement the interface so the tests pass (green)
4. **Run tests**: `cargo test --features <corresponding feature> --lib`, make sure all tests pass
5. **Impact analysis**: check the impact of this change on other modules and identify code that needs coordinated updates
6. **Move on**: adjust follow-up tasks based on the impact analysis, then start the next loop

> ⚠️ Note that the `postgres` / `sqlite` / `mysql` storage drivers are mutually exclusive; use explicit feature combinations for tests and verification, never `--all-features`.

## 🪝 Pre-Commit Checks

The following checks run automatically before a commit (see [.pre-commit-config.yaml](../../.pre-commit-config.yaml) and [lefthook.yml](../../lefthook.yml)):

| Category | Check |
|------|------|
| Format | `cargo fmt --all -- --check` |
| Static analysis | `cargo clippy --all-targets --no-default-features --features full -- -D warnings` |
| Compilation | `cargo check --no-default-features --features full` |
| Supply chain | `cargo deny check` (pre-push additionally runs `cargo audit` and the coverage ≥90% gate) |
| General hygiene | trailing-whitespace / end-of-file-fixer / check-yaml / check-toml / typos |
| Secret protection | private-key scan and detect-secrets (with the `.secrets.baseline` false-positive baseline) |

> **Using `--no-verify` to skip commit hooks is forbidden.**

## 📐 Code Style

- Follow the naming and architecture conventions of the existing codebase; convention over novelty
- Dependencies must be feature-gated (`optional = true`)
- 100-character maximum line width ([rustfmt.toml](../../rustfmt.toml)), 4-space indentation
- Wildcard imports are forbidden (`warn-on-all-wildcard-imports`)
- Use `ahash` + `DashMap`; `std::collections::HashMap`/`HashSet` are forbidden
- Use `parking_lot` instead of `std::sync` primitives
- All public APIs must have doc comments
- Unsafe code must document its safety invariants

## 🔀 Pull Request Process

1. Create a feature branch from `main`: `git checkout -b feature/<description>`
2. Write code and make sure the tests pass: `cargo test --workspace --no-default-features --features full`
3. Make sure all commit hooks pass
4. Open the PR and link the related issues
5. Wait for CI to pass and for code review

## ✉️ Commit Message Convention

Use Conventional Commits:

- `feat(<module>): <description>` — new feature
- `fix(<module>): <description>` — bug fix
- `chore(<module>): <description>` — build/tooling/docs
- `refactor(<module>): <description>` — refactoring
- `docs(<module>): <description>` — documentation
