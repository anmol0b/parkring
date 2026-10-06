# Contributing to parkring

Thanks for helping. parkring is small on purpose and verified heavily, so the
bar for changes to the concurrent code is high, but bug reports, docs fixes,
benchmarks on new hardware and tests are always welcome.

## Before you start

* For anything beyond a small fix, open an issue first so we can agree on the
  approach.
* A soundness bug is a security issue: see [SECURITY.md](SECURITY.md).
* By contributing you agree your work is licensed under the MIT license.

## The checks CI runs

Run these before opening a pull request. The first four are quick.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --doc
```

Changes to anything under `src/` also need:

```sh
# loom: every interleaving, up to a preemption bound
RUSTFLAGS="--cfg loom" cargo test -p parkring --release \
  --test loom --test loom_scq --test loom_pool --test loom_channel
RUSTFLAGS="--cfg loom" cargo test -p parkring --release --lib deque
# the same with the portable parker
RUSTFLAGS="--cfg loom --cfg parkring_force_condvar" cargo test -p parkring --release --test loom

# Miri: undefined behaviour, aliasing, leaks
cargo +nightly miri test -p parkring --lib --test drop_semantics --test channel --test deque --test pool
```

CI also checks the MSRV (Rust 1.85), FreeBSD, 32-bit Linux, ThreadSanitizer,
cargo-deny and the public API snapshot.

## Rules for concurrent code

* **Every primitive goes through `crate::sync`.** Atomics, `Mutex`, `Condvar`,
  `Arc` and `UnsafeCell` come from `src/sync/primitives.rs`, so loom can
  replace them. CI rejects direct `std::sync` use elsewhere in `src/`.
* **Every `unsafe` block has a `// SAFETY:` comment** that names the
  invariant (clippy enforces this). Update [docs/UNSAFE.md](docs/UNSAFE.md)
  when you add or change one.
* **Every memory-ordering change comes with a loom model** that fails without
  it. If the change is a fence or an ordering that loom cannot express, add a
  mutant (`--cfg parkring_mutant="..."`) and a CI job that requires loom to
  catch it; see `.github/workflows/ci.yml`.
* **Every bug fix comes with a regression test** (a loom model, a Miri case or
  a test in `tests/regressions.rs`) that fails before the fix.
* **Benchmarks report losses too.** Performance claims go in
  `docs/BENCHMARKS.md` with the machine, the command and the numbers.

## Public API changes

`public-api.txt` is a snapshot of the public API, and CI fails if they differ.
If you changed the API on purpose, regenerate it with the pinned nightly:

```sh
cargo install cargo-public-api --locked
RUSTUP_TOOLCHAIN=nightly-2026-01-04 cargo public-api -p parkring --omit blanket-impls > public-api.txt
```

A change that removes or alters something in that file is a breaking change
and needs a major release; adding is fine in a minor release. See the
README's "Stability" section.

## Commits and changelog

* Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `perf:`, `docs:`, `test:`, `ci:`; `!` for breaking).
* Add a line under `[Unreleased]` in `CHANGELOG.md` for anything a user would
  notice.

## Releases

Maintainers only. release-plz opens a release pull request from the changelog
and commits; merging it publishes to crates.io and tags the release.
