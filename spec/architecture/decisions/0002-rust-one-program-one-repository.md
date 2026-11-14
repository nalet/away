# ADR-0002: Written in Rust, as one program in one repository

**Status:** Proposed · 2026-11-14. Nothing of it is built.

## Context

- What reads a machine's bytes must not be open to the mistakes that C and C++ allow: reading past the end of a buffer, using memory that was given back. A large share of the security holes found in software written in those languages are of that kind, and a gateway reads bytes from machines it can't vouch for all day.
- The quality *It runs on little* ([Run it oneself](../../goals/README.md#run-it-oneself)) proposes two processor cores and 2 GiB for twenty desktops of office work. That leaves no room for a process per session, or for a pause to tidy memory in the middle of a picture.
- A part of the code runs in the browser ([ADR-0005](./0005-the-shells-screen-in-the-browser.md)).
- Anyone has to be able to build it from its source with one command (`build-from-source`), and to see what an image holds (`see-image-contents`).
- Away's parts ([ADR-0003](./0003-three-parts-with-their-own-rights.md)) share most of their code: the format of the files, the messages between them, the checks.

## Decision

1. **Rust**, for everything that runs beside the machines and for every library. The compiler's version is written down in the repository, so every build uses the same one.
2. **One repository** holds all of Away: the libraries, the parts, the web app, the chart, the specification, the documentation and the site. One version number names all of it.
3. **One program**, `away`, holds every part. Its first word chooses one: `away server`, `away gateway`, `away issuer`. `away check` runs the check of the setup from a command line.
4. **One image** holds the program and the web app's files, and nothing else: no shell, no package manager, no second program. The chart runs it three times, each time as one part with that part's rights.
5. **What others wrote is listed.** Every library Away is built from directly is written down with what it is for. The build fails on a licence that is not on the list of allowed ones, on a known security hole, and on a source other than the language's public registry. The kinds expected are few: running many connections at once, HTTP, TLS, the cryptography, the database's client, picture formats. None speaks a remote protocol ([ADR-0001](./0001-own-protocol-code.md)).
6. **Two exceptions to the language.** The web app is TypeScript ([ADR-0011](./0011-the-web-app.md)). The shell's screen is Rust, compiled for the browser.

## Consequences

- The mistakes above can't be made in ordinary Rust, and the libraries that read a machine's bytes allow themselves nothing else ([ADR-0001](./0001-own-protocol-code.md)).
- One build, one image to sign, to verify and to list. No part can be a version behind another.
- A session costs memory and no process, so many of them fit in little.
- Rust takes longer to build than most languages, and fewer people write it. A stranger who wants to change Away has to know it or learn it.
- Every part's code is in every container. What separates the parts is their rights, not their code ([ADR-0003](./0003-three-parts-with-their-own-rights.md)).
- A change to any part is a release of all of them.

### What this rules out

- **No part in another language**, beyond the two exceptions.
- **No versions of their own for the parts**, and no image for each.
- **No library without a line saying what it is for.**
