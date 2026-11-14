# The libraries Away is built from

Away is built from little that others wrote, and what there is, is listed. This
page names every library that Away uses directly, with what it is for. The
build writes the table from `Cargo.toml` and `Cargo.lock`, and fails when a
library has no line that says what it is for.

<!-- generated: libraries -->
| Library | What it is for | Version in this build | With |
|---|---|---|---|
| [tokio](https://crates.io/crates/tokio) | runs many connections at once | 1.54.1 | `rt-multi-thread`, `net`, `signal`, `macros`, `time`, `sync` |
| [hyper](https://crates.io/crates/hyper) | speaks HTTP to browsers, to the cluster's checks and to the sign-in service | 1.12.0 | `http1`, `server`, `client` |
| [hyper-util](https://crates.io/crates/hyper-util) | lets hyper use tokio's connections and timers | 0.1.21 | `tokio` |
| [http-body-util](https://crates.io/crates/http-body-util) | holds the body of an HTTP answer | 0.1.5 | — |
| [rustls](https://crates.io/crates/rustls) | secures the connection to a machine and to the sign-in service: TLS is never Away's own | 0.23.46 | `ring`, `std`, `tls12` |
| [tokio-rustls](https://crates.io/crates/tokio-rustls) | lets rustls secure tokio's connections | 0.26.6 | `ring`, `tls12` |
| [webpki-roots](https://crates.io/crates/webpki-roots) | the public authorities a sign-in service's certificate is checked against, since the image holds no others | 1.0.12 | — |
| [ring](https://crates.io/crates/ring) | checks the signature on what a sign-in service signed | 0.17.14 | — |
| [sha2](https://crates.io/crates/sha2) | makes the fingerprint a machine is known by, and binds a sign-in to its connection | 0.11.1 | — |
| [md4](https://crates.io/crates/md4) | turns a password into the key Windows knows it by: part of signing in with a name and a password | 0.11.0 | — |
| [md-5](https://crates.io/crates/md-5) | the hash that signing in with a name and a password is built on | 0.11.0 | — |
| [hmac](https://crates.io/crates/hmac) | makes the proofs and the keys of signing in with a name and a password | 0.13.0 | — |
| [rc4](https://crates.io/crates/rc4) | the cipher that signing in with a name and a password seals its messages with, and the taking of a licence too | 0.2.0 | — |
| [sha1](https://crates.io/crates/sha1) | the second hash that the taking of a licence makes its keys and its checks with | 0.11.0 | — |
| [num-bigint](https://crates.io/crates/num-bigint) | does the arithmetic of the old RSA step in taking a licence, which the protocol defines as it is | 0.5.1 | — |
| [getrandom](https://crates.io/crates/getrandom) | takes random bytes from the system, for keys and for numbers used once | 0.2.17, 0.4.3 | — |
| [saphyr-parser](https://crates.io/crates/saphyr-parser) | reads the files of machines and rules, which are YAML, and knows the line each thing is written in | 0.3.4 | — |

Away names 17 libraries. With what they bring with them, a build holds 80 in all, each in the version Cargo.lock names.
<!-- end: libraries -->

## The pages

The pages are built from the design system and little else. `web/package.json`
names their libraries, and what each is for. The design system comes from its
own release, by that release's address; every other library from the public
registry. `web/package-lock.json` holds each one's exact version and checksum,
and the build fails on a licence that `deny.toml` doesn't allow.

<!-- generated: pages -->
| Library | What it is for | Version in this build | In the pages |
|---|---|---|---|
| [@fontsource/inter](https://www.npmjs.com/package/@fontsource/inter) | the font of the pages' text, as files of the build, so that none is fetched from a font service | 5.3.0 | yes |
| [@fontsource/jetbrains-mono](https://www.npmjs.com/package/@fontsource/jetbrains-mono) | the font of names, labels and code, as files of the build | 5.3.0 | yes |
| [@nalet/design-system](https://www.npmjs.com/package/@nalet/design-system) | the components and the tokens the pages are made of | 0.4.0 | yes |
| [lucide-react](https://www.npmjs.com/package/lucide-react) | the icons, which the design system's components use as well | 1.73.0 | yes |
| [react](https://www.npmjs.com/package/react) | draws the pages | 19.3.1 | yes |
| [react-dom](https://www.npmjs.com/package/react-dom) | puts what React draws into the browser's page | 19.3.1 | yes |
| [@axe-core/playwright](https://www.npmjs.com/package/@axe-core/playwright) | checks each page against the rules for accessibility, in the pages' tests | 4.14.0 | no, it only builds them |
| [@playwright/test](https://www.npmjs.com/package/@playwright/test) | opens the pages in a real browser and goes through them as a person does, in the pages' tests | 1.65.1 | no, it only builds them |
| [@types/react](https://www.npmjs.com/package/@types/react) | tells TypeScript what React offers | 19.3.0 | no, it only builds them |
| [@types/react-dom](https://www.npmjs.com/package/@types/react-dom) | tells TypeScript what react-dom offers | 19.3.0 | no, it only builds them |
| [@vitejs/plugin-react](https://www.npmjs.com/package/@vitejs/plugin-react) | lets vite build what is written for React | 6.3.0 | no, it only builds them |
| [typescript](https://www.npmjs.com/package/typescript) | checks the pages' code before it is built | 7.0.2 | no, it only builds them |
| [vite](https://www.npmjs.com/package/vite) | builds the pages into files whose names carry their content's hash | 8.3.8 | no, it only builds them |

The pages name 13 libraries, 6 of which are in them. With what those bring with them, the pages hold 7 in all, each in the version web/package-lock.json names.
<!-- end: pages -->

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

Away shows the same in itself: in the settings, *About Away* lists every part
the program is built from, with its version and its licence, beside the version
that runs, where its source is, and the licence Away is under. The program
holds that list as `crates/away-server/built-from.json`, which
`scripts/contents.py --parts` writes from the lock files; the build fails when
the two differ.

## Adding a library

1. Name it under `[workspace.dependencies]` in `Cargo.toml`, with the features
   it needs and nothing more, and say what it is for after a `#` on the same
   line.
2. Take it into a part with `name.workspace = true`.
3. Run `python3 scripts/libraries.py`, which writes the table above.
4. Run `python3 scripts/contents.py --parts crates/away-server/built-from.json`,
   which writes the list Away shows of itself.

A library that could be twenty lines of Away's own is twenty lines of Away's
own.
