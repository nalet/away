# The libraries Away is built from

Away is built from little that others wrote, and what there is, is listed. This
page names every library that Away uses directly, with what it is for. The
build writes the table from `Cargo.toml` and `Cargo.lock`, and fails when a
library has no line that says what it is for.

<!-- generated: libraries -->
| Library | What it is for | Version in this build | With |
|---|---|---|---|
| [tokio](https://crates.io/crates/tokio) | runs many connections at once | 1.54.1 | `rt-multi-thread`, `net`, `signal`, `macros`, `time`, `sync` |
| [hyper](https://crates.io/crates/hyper) | speaks HTTP to browsers and to the cluster's checks | 1.12.0 | `http1`, `server` |
| [hyper-util](https://crates.io/crates/hyper-util) | lets hyper use tokio's connections and timers | 0.1.21 | `tokio` |
| [http-body-util](https://crates.io/crates/http-body-util) | holds the body of an HTTP answer | 0.1.5 | — |

Away names 4 libraries. They bring 23 more with them, which makes 27 in all.
<!-- end: libraries -->

## What the build refuses

On every change the pipeline runs `cargo deny check` with the rules of
[deny.toml](../deny.toml). It fails on:

| Refused | Why |
|---|---|
| a licence that is not on the list | everything in a release may be used and passed on under terms that are known |
| a known security hole | in any library, however far down |
| a version its author withdrew | |
| a source other than the language's public registry | no library comes from somebody's own repository or a file |

No library speaks RDP, VNC or SSH: that code is Away's own
([ADR-0001](../spec/architecture/decisions/0001-own-protocol-code.md)). The
cryptography is never Away's own.

## Every library, with its exact version

`Cargo.lock` names every library a build holds, with its exact version and a
checksum of its source. The list of what a published image holds, with every
licence, is attached to the image; [building and checking](./build.md) says how
to read it.

## Adding a library

1. Name it under `[workspace.dependencies]` in `Cargo.toml`, with the features
   it needs and nothing more, and say what it is for after a `#` on the same
   line.
2. Take it into a part with `name.workspace = true`.
3. Run `python3 scripts/libraries.py`, which writes the table above.

A library that could be twenty lines of Away's own is twenty lines of Away's
own.
