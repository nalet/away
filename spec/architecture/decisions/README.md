# Decisions

Why Away is built the way it is: the decisions that would cost the most to
find again. Each is a record of its own, with the components and the
capabilities it governs and the work packages that rest on it.

> **A record is Proposed until the owner accepts it**, by its number. A work
> package begins only once the records it rests on are accepted.

<!-- generated: decisions -->
| Record | Decision | Status | Work packages that rest on it |
|---|---|---|---|
| [ADR‑0001](./0001-own-protocol-code.md) | Away speaks RDP, VNC and SSH with code of its own | Accepted | [2](../../plan/README.md#2-rdp-proven-without-a-browser), [6](../../plan/README.md#6-a-shell), [9](../../plan/README.md#9-a-desktop-over-vnc), [10](../../plan/README.md#10-a-desktop-over-rdp), [13](../../plan/README.md#13-programs) |
| [ADR‑0002](./0002-rust-one-program-one-repository.md) | Written in Rust, as one program in one repository | Accepted | [1](../../plan/README.md#1-the-ground) |
| [ADR‑0003](./0003-three-parts-with-their-own-rights.md) | Three parts, each with its own rights | Accepted | [1](../../plan/README.md#1-the-ground), [7](../../plan/README.md#7-sessions-as-they-run) |
| [ADR‑0004](./0004-the-session-and-its-picture.md) | One connection a session, and the gateway keeps the picture | Accepted | [6](../../plan/README.md#6-a-shell), [7](../../plan/README.md#7-sessions-as-they-run), [9](../../plan/README.md#9-a-desktop-over-vnc), [10](../../plan/README.md#10-a-desktop-over-rdp), [13](../../plan/README.md#13-programs) |
| [ADR‑0005](./0005-the-shells-screen-in-the-browser.md) | A shell's screen is worked out in the browser | Accepted | [6](../../plan/README.md#6-a-shell) |
| [ADR‑0006](./0006-the-sign-in-is-held-by-the-server.md) | The sign-in to Away is held by the server | Accepted | [4](../../plan/README.md#4-getting-in-and-the-frame), [7](../../plan/README.md#7-sessions-as-they-run) |
| [ADR‑0007](./0007-short-lived-certificates.md) | On a machine as oneself, by a certificate that lasts minutes | Accepted | [3](../../plan/README.md#3-the-windows-sign-in-proven-without-a-browser), [8](../../plan/README.md#8-a-shell-as-oneself), [11](../../plan/README.md#11-windows-as-oneself) |
| [ADR‑0008](./0008-the-authority-for-windows-sign-ins.md) | Who issues Windows sign-ins is a setting, and without a domain nobody has to | Accepted | [3](../../plan/README.md#3-the-windows-sign-in-proven-without-a-browser), [11](../../plan/README.md#11-windows-as-oneself), [12](../../plan/README.md#12-the-organisations-own-authority) |
| [ADR‑0009](./0009-one-database-and-files.md) | Machines and rules are files; everything else Away keeps is in one database | Accepted | [5](../../plan/README.md#5-the-register-the-rules-and-home) |
| [ADR‑0010](./0010-secrets-live-in-the-cluster.md) | Secrets and keys live in the cluster, and only the issuer reads a kept account | Accepted | [6](../../plan/README.md#6-a-shell) |
| [ADR‑0011](./0011-the-web-app.md) | The web app is built from the design system, and loads nothing from outside | Accepted | [4](../../plan/README.md#4-getting-in-and-the-frame), [14](../../plan/README.md#14-by-touch) |
| [ADR‑0012](./0012-one-chart-no-special-rights.md) | Installed by one chart, with no rights beyond its own namespace | Accepted | [1](../../plan/README.md#1-the-ground), [15](../../plan/README.md#15-status-and-the-check-of-the-setup) |
| [ADR‑0013](./0013-built-tested-signed-by-the-pipelines.md) | Built, tested, signed and listed by the project's own pipelines | Accepted | [1](../../plan/README.md#1-the-ground), [16](../../plan/README.md#16-ready-to-release) |
| [ADR‑0014](./0014-a-farm-says-what-it-publishes.md) | A farm says itself what it publishes, and the server asks it | Accepted | — |
| [ADR‑0015](./0015-own-database-client.md) | The server speaks to its database with code of its own | Proposed | — |

## What each governs

The components a record is about, and the capabilities it decides most. A component's key is explained on [components](../components.md), a capability's on [where each capability is carried](../carried.md).

| Record | Components | Capabilities |
|---|---|---|
| [ADR‑0001](./0001-own-protocol-code.md) | `rdp`, `windows-sign-in`, `vnc`, `ssh`, `shell-screen`, `test-servers` | `open-desktop`, `open-program`, `open-shell`, `run-its-tests` |
| [ADR‑0002](./0002-rust-one-program-one-repository.md) | `server`, `gateway`, `issuer`, `pipelines` | `build-from-source`, `see-image-contents` |
| [ADR‑0003](./0003-three-parts-with-their-own-rights.md) | `server`, `gateway`, `issuer`, `chart` | `sign-apart`, `end-session`, `update-in-one-step` |
| [ADR‑0004](./0004-the-session-and-its-picture.md) | `session-view`, `gateway`, `picture`, `wire`, `way-in` | `fit-to-window`, `exact-at-rest`, `adapt-to-line`, `pointer-without-delay`, `reconnect-by-itself`, `set-address` |
| [ADR‑0005](./0005-the-shells-screen-in-the-browser.md) | `shell-screen`, `session-view`, `gateway` | `run-full-screen-programs`, `scroll-back`, `select-shell-text` |
| [ADR‑0006](./0006-the-sign-in-is-held-by-the-server.md) | `web`, `server`, `sign-in-service` | `sign-in`, `stay-signed-in`, `sign-in-again`, `sign-out`, `take-people-from-provider`, `use-own-provider`, `end-on-lost-right` |
| [ADR‑0007](./0007-short-lived-certificates.md) | `gateway`, `issuer`, `windows-sign-in`, `ssh` | `be-oneself-on-machine`, `reach-network-as-oneself`, `unlock-without-password`, `make-shells-trust`, `name-account-claim`, `record-sign-ins-made` |
| [ADR‑0008](./0008-the-authority-for-windows-sign-ins.md) | `issuer`, `domain`, `authority` | `connect-domain-authority`, `certify-domain-controllers` |
| [ADR‑0009](./0009-one-database-and-files.md) | `server`, `database`, `files` | `keep-as-files`, `see-what-is-in-force`, `see-refused-change`, `record-connections`, `record-changes`, `read-records`, `send-records-on` |
| [ADR‑0010](./0010-secrets-live-in-the-cluster.md) | `issuer`, `cluster-secrets` | `keep-secret`, `use-kept-secret`, `protect-secrets` |
| [ADR‑0011](./0011-the-web-app.md) | `web`, `session-view` | `move-between-views`, `use-on-phone`, `use-without-mouse`, `call-nobody-else` |
| [ADR‑0012](./0012-one-chart-no-special-rights.md) | `chart` | `install-with-chart`, `run-without-special-rights`, `set-address`, `use-own-certificates`, `check-setup`, `see-health`, `give-measures` |
| [ADR‑0013](./0013-built-tested-signed-by-the-pipelines.md) | `pipelines`, `site-pipeline`, `test-servers`, `registry`, `docs` | `verify-image`, `see-image-contents`, `build-from-source`, `run-its-tests`, `report-in-private` |
| [ADR‑0014](./0014-a-farm-says-what-it-publishes.md) | `server`, `files`, `farm-feed`, `cluster-secrets` | `read-programs-from-host`, `see-all-programs`, `give-program-icons` |
| [ADR‑0015](./0015-own-database-client.md) | `server`, `database` | `keep-favourites`, `see-last-used`, `record-connections`, `record-changes` |
<!-- end: decisions -->

## How a record is written

A record is short and has four parts:

| Part | Says |
|---|---|
| Context | what the model asks for, and the ways there are to meet it |
| Decision | what is done, in numbered points |
| Consequences | what gets better, and what it costs |
| What this rules out | the ways that are closed by it, so nobody proposes them again without knowing |

Where a decision rests on something that has not been shown yet, the record
has a part named Validation: what is measured or proven, in which work
package, and what happens if it fails.

A record states as fact only what exists. Everything else in it is a decision
about what will be built.

## Adding one

A new record takes the next number and the same parts. Its row in
[decisions.csv](../decisions.csv) names the components and the capabilities it
governs. `python3 scripts/model.py` then writes the table above.
