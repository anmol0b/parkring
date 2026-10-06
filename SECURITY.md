# Security policy

parkring is a concurrency library with `unsafe` code. A soundness bug (undefined
behaviour reachable from safe code, a data race, a double drop, use after free)
is treated as a security issue, even if no exploit is known.

## Reporting

Please report privately, not in a public issue:

* GitHub: **Security → Report a vulnerability** on
  [anmol0b/parkring](https://github.com/anmol0b/parkring/security/advisories/new)
  (private vulnerability reporting).

Include the parkring version, platform, and a reproducer if you have one. A
loom model, a Miri failure or a failing test is ideal, but a description is
enough.

## What to expect

* An acknowledgement within 7 days.
* A fix, or a plan with a timeline, within 30 days for a confirmed soundness
  bug.
* A patch release for every supported version, a RustSec advisory, and credit
  in the advisory and changelog unless you prefer otherwise.

## Supported versions

| version | supported |
|---|---|
| 1.x (latest minor) | yes |
| < 1.0 | no; please upgrade |

## Scope

In scope: anything in the published `parkring` crate. Out of scope: the
benchmarks (`crates/parkring-bench`), the fuzz targets, and the launch video
tooling, which are not published.

The `unsafe` code and the invariants it relies on are listed in
[docs/UNSAFE.md](docs/UNSAFE.md).
