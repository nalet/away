# Components

Every component of Away, layer by layer: what it is, its kind, when it comes,
and how many capabilities of the first release it carries. The lines under a
table say the rest: what a component keeps, where its code is, what runs it,
and the decisions that govern it. How to read the columns is on the
[front page](./README.md#how-to-read-it).

A path after Code is where a component's code will be. Only what says *there*
under Comes exists.

<!-- generated: components -->
## In the browser

What a person's browser runs.

| Component | Is | Kind | Comes | Carries |
|---|---|---|---|---|
| `web` | the web app: Away's pages around the sessions | web | first release | 44 |
| `session-view` | the session view: the picture, the keys and the pointer, the shell's screen | web | there | 37 |

- `web`: Built from the design system's components; the frame is Away's own until the design system has it. Keeps: in the browser's own storage, which sessions this browser has open and how wide the list of windows was left. Code: `web/`. Runs: served by the server. Governed by [ADR‑0006](./decisions/0006-the-sign-in-is-held-by-the-server.md), [ADR‑0011](./decisions/0011-the-web-app.md).
- `session-view`: Drawn on a canvas, apart from the pages. Code: `web/src/session/`. Runs: part of the web app. Governed by [ADR‑0004](./decisions/0004-the-session-and-its-picture.md), [ADR‑0005](./decisions/0005-the-shells-screen-in-the-browser.md), [ADR‑0011](./decisions/0011-the-web-app.md).

## Away's parts

What the chart runs from Away's one image.

| Component | Is | Kind | Comes | Carries |
|---|---|---|---|---|
| `server` | the server: the pages, the requests, people's sign-ins, the rules, the records | service | there | 55 |
| `gateway` | the gateway: the sessions, each with one connection to the browser and one to the machine | service | there | 46 |
| `issuer` | the issuer: makes the sign-ins on machines | service | there | 15 |
| `relay` | a relay placed in a network Away is not in | relay | later | — |

- `server`: The only part that reaches the database and the sign-in service; it reaches no machine. Keeps: nothing itself; what it holds is in the database. Code: `crates/away-server/`. Runs: run by the chart, as server. Governed by [ADR‑0002](./decisions/0002-rust-one-program-one-repository.md), [ADR‑0003](./decisions/0003-three-parts-with-their-own-rights.md), [ADR‑0006](./decisions/0006-the-sign-in-is-held-by-the-server.md), [ADR‑0009](./decisions/0009-one-database-and-files.md), [ADR‑0014](./decisions/0014-a-farm-says-what-it-publishes.md), [ADR‑0015](./decisions/0015-a-widely-used-database-client.md).
- `gateway`: The only part that reaches machines; the first release runs one. Keeps: in memory and for as long as a session lasts, its picture, a shell's last bytes and what signed it in. Code: `crates/away-gateway/`. Runs: run by the chart, as gateway. Governed by [ADR‑0002](./decisions/0002-rust-one-program-one-repository.md), [ADR‑0003](./decisions/0003-three-parts-with-their-own-rights.md), [ADR‑0004](./decisions/0004-the-session-and-its-picture.md), [ADR‑0005](./decisions/0005-the-shells-screen-in-the-browser.md), [ADR‑0007](./decisions/0007-short-lived-certificates.md).
- `issuer`: It checks a person's sign-in itself before it signs anything; the status page calls it signing in on machines. Keeps: nothing itself; its authority's keys and the kept accounts are the cluster's. Code: `crates/away-issuer/`. Runs: run by the chart, as issuer. Governed by [ADR‑0002](./decisions/0002-rust-one-program-one-repository.md), [ADR‑0003](./decisions/0003-three-parts-with-their-own-rights.md), [ADR‑0007](./decisions/0007-short-lived-certificates.md), [ADR‑0008](./decisions/0008-the-authority-for-windows-sign-ins.md), [ADR‑0010](./decisions/0010-secrets-live-in-the-cluster.md).
- `relay`: For machines the gateway can't reach itself.

## Away's own protocol code

The libraries the parts and the session view are built from.

| Component | Is | Kind | Comes | Carries |
|---|---|---|---|---|
| `rdp` | the RDP code | library | first release | 10 |
| `windows-sign-in` | the Windows sign-in: Kerberos, the sign-in before the desktop, the smart card in memory | library | first release | 3 |
| `vnc` | the VNC code | library | first release | 2 |
| `ssh` | the SSH code | library | first release | 3 |
| `shell-screen` | the shell's screen: turns what a shell writes into what it shows | library | first release | 7 |
| `picture` | the picture: a desktop as the gateway keeps it, and the rectangles it sends | library | first release | 5 |
| `wire` | the session's messages between the browser and the gateway | contract | there | 1 |

- `rdp`: For a Windows desktop and its programs, and for desktops of other systems that speak RDP. Code: `crates/away-rdp/`. Runs: in the gateway. Governed by [ADR‑0001](./decisions/0001-own-protocol-code.md).
- `windows-sign-in`: Code: `crates/away-winauth/`. Runs: in the gateway. Governed by [ADR‑0001](./decisions/0001-own-protocol-code.md), [ADR‑0007](./decisions/0007-short-lived-certificates.md).
- `vnc`: Code: `crates/away-vnc/`. Runs: in the gateway. Governed by [ADR‑0001](./decisions/0001-own-protocol-code.md).
- `ssh`: Code: `crates/away-ssh/`. Runs: in the gateway. Governed by [ADR‑0001](./decisions/0001-own-protocol-code.md), [ADR‑0007](./decisions/0007-short-lived-certificates.md).
- `shell-screen`: What is elsewhere called a terminal emulator; later it runs in the gateway too, to hold a shell while the browser is closed. Code: `crates/away-term/`. Runs: in the browser, as WebAssembly. Governed by [ADR‑0001](./decisions/0001-own-protocol-code.md), [ADR‑0005](./decisions/0005-the-shells-screen-in-the-browser.md).
- `picture`: Code: `crates/away-picture/`. Runs: in the gateway. Governed by [ADR‑0004](./decisions/0004-the-session-and-its-picture.md).
- `wire`: Written once for each side, and both are tried against the same examples. Code: `crates/away-wire/`. Runs: in the gateway and in the browser. Governed by [ADR‑0004](./decisions/0004-the-session-and-its-picture.md).

## What Away rests on

What the cluster gives it, and the database.

| Component | Is | Kind | Comes | Carries |
|---|---|---|---|---|
| `database` | the database | database | first release | — |
| `files` | the files: the machines, the groups, the rules and the settings | files | first release | — |
| `cluster-secrets` | the cluster's secrets | store | first release | — |
| `way-in` | the way in: the cluster's router or ingress, as it is there | proxy | first release | — |

- `database`: PostgreSQL. Keeps: the sign-ins, each person's settings, favourites and last used, the machines' identities, the sessions as they run, the records. Runs: run by the chart, or brought by the operator. Governed by [ADR‑0009](./decisions/0009-one-database-and-files.md), [ADR‑0015](./decisions/0015-a-widely-used-database-client.md).
- `files`: Written by the operator and kept where they keep their configuration. Keeps: what is in force. Runs: mounted into the server, from a ConfigMap. Governed by [ADR‑0009](./decisions/0009-one-database-and-files.md), [ADR‑0014](./decisions/0014-a-farm-says-what-it-publishes.md).
- `cluster-secrets`: Each part has its own mounted; the issuer alone reads the kept accounts. Keeps: the kept accounts, the issuer's authority keys, each part's own keys. Governed by [ADR‑0010](./decisions/0010-secrets-live-in-the-cluster.md), [ADR‑0014](./decisions/0014-a-farm-says-what-it-publishes.md).
- `way-in`: It passes the pages on to the server and a session's connection on to the gateway. Governed by [ADR‑0004](./decisions/0004-the-session-and-its-picture.md).

## Outside Away

What Away reaches and does not run.

| Component | Is | Kind | Comes | Carries |
|---|---|---|---|---|
| `sign-in-service` | the sign-in service: the organisation's identity provider | identity | first release | — |
| `machines` | the machines | machine | first release | — |
| `farm-feed` | a farm's feed: where a farm of Windows machines says what it publishes | machine | first release | — |
| `domain` | the Windows domain: its domain controllers | directory | first release | — |
| `authority` | the organisation's authority: its own certificate authority | authority | first release | — |
| `log-system` | the log system: the one that is there | logs | first release | — |
| `monitoring` | the monitoring: the one that is there | monitoring | first release | — |

- `sign-in-service`: Any provider that speaks OpenID Connect. Keeps: the people and their groups. Governed by [ADR‑0006](./decisions/0006-the-sign-in-is-held-by-the-server.md).
- `machines`: Reached the way each already offers: RDP, VNC or SSH; nothing of Away's runs on them.
- `farm-feed`: Only where the files name a farm: the farm's own web access, which the server asks and opens nothing on. Keeps: the desktops and the programs the farm publishes. Governed by [ADR‑0014](./decisions/0014-a-farm-says-what-it-publishes.md).
- `domain`: For signing in on its machines as oneself; an installation without one leaves it out and lacks nothing else. Keeps: the accounts. Governed by [ADR‑0008](./decisions/0008-the-authority-for-windows-sign-ins.md).
- `authority`: Where the configuration says that the organisation's own authority issues the Windows sign-ins. Governed by [ADR‑0008](./decisions/0008-the-authority-for-windows-sign-ins.md).
- `log-system`: It collects the lines every part puts out.
- `monitoring`: It reads every part's measures.

## Making and shipping it

What builds, tests, installs and describes Away.

| Component | Is | Kind | Comes | Carries |
|---|---|---|---|---|
| `chart` | the chart that installs Away | chart | there | 12 |
| `pipelines` | the pipelines: build, test, sign, list, publish | pipeline | there | 6 |
| `site-pipeline` | the pipeline that checks the model and publishes the site | pipeline | there | — |
| `test-program` | the test program: tries a machine with Away's own protocol code, with no browser | tests | there | — |
| `test-servers` | the servers the tests run against: a shell, a desktop over VNC, a desktop over RDP | tests | first release | 1 |
| `registry` | the registry the project publishes to | registry | there | — |
| `docs` | the documentation, the specification and the site | docs | there | 5 |

- `chart`: No kinds of its own and no rights beyond its namespace. Code: `chart/`. Governed by [ADR‑0003](./decisions/0003-three-parts-with-their-own-rights.md), [ADR‑0012](./decisions/0012-one-chart-no-special-rights.md).
- `pipelines`: Code: `.github/workflows/`. Runs: on GitHub. Governed by [ADR‑0002](./decisions/0002-rust-one-program-one-repository.md), [ADR‑0013](./decisions/0013-built-tested-signed-by-the-pipelines.md).
- `site-pipeline`: Code: `.github/workflows/site.yml`. Runs: on GitHub. Governed by [ADR‑0013](./decisions/0013-built-tested-signed-by-the-pipelines.md).
- `test-program`: What the two proofs are shown with; it is not in the image. Code: `crates/away-try/`. Runs: by hand, and by the pipeline: against a stand-in and the test servers.
- `test-servers`: A desktop over RDP on Linux and a sign-in service are there; a Windows machine in a domain needs a place of its own. Code: `tests/machines/`. Runs: started by the pipeline. Governed by [ADR‑0001](./decisions/0001-own-protocol-code.md), [ADR‑0013](./decisions/0013-built-tested-signed-by-the-pipelines.md).
- `registry`: Keeps: the image and the chart, their signatures, the records of their builds, the lists of their contents. Governed by [ADR‑0013](./decisions/0013-built-tested-signed-by-the-pipelines.md).
- `docs`: Code: `docs/`, `spec/`, `site/`. Runs: the site is built and published by its pipeline. Governed by [ADR‑0013](./decisions/0013-built-tested-signed-by-the-pipelines.md).
<!-- end: components -->
