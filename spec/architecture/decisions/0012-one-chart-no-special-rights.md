# ADR-0012: Installed by one chart, with no rights beyond its own namespace

**Status:** Accepted · 2026-11-14. Nothing of it is built.

## Context

Somebody runs Away themselves ([Run it oneself](../../goals/README.md#run-it-oneself)). The model wants it installed on Kubernetes or OpenShift with one chart (`install-with-chart`), with no special rights on the cluster (`run-without-special-rights`), behind the reverse proxy and with the certificates that are there (`set-address`, `use-own-certificates`), and by someone who has not seen it, in an hour.

How a cluster takes a chart differs: by hand, from a pipeline, from a tool that follows a repository. Away shouldn't care.

## Decision

1. **One chart, in the repository, published beside the image.** It makes:

   | What | For |
   |---|---|
   | three workloads from the one image | the server, the gateway and the issuer ([ADR-0003](./0003-three-parts-with-their-own-rights.md)) |
   | the way in | a route or an ingress on Away's address: a session's connection goes to the gateway, everything else to the server |
   | network rules | only the connections [the model lists](../connections.md) are allowed |
   | the files | the machines, groups, rules and settings, from the chart's values or from a ConfigMap the operator names |
   | a database, if asked | for an installation that brings none ([ADR-0009](./0009-one-database-and-files.md)) |
   | what the monitoring takes, if asked | where the measures are, and rules for alerts that are ready to use |

2. **Nothing that needs rights over the cluster.** No kinds of Away's own, no role beyond its namespace, no privileged container. Every container runs as an ordinary user and never as root, can't write to its own files, and has no capabilities, so it fits the strictest profile a cluster has without an exception.
3. **Inside its namespace, one right:** the issuer may read secrets ([ADR-0010](./0010-secrets-live-in-the-cluster.md)). The server and the gateway ask the cluster for nothing.
4. **One file of values sets it up:** the address, the sign-in service, which entries of a sign-in name the groups and the account, the domain for Windows sign-ins, the database, whom people turn to.
5. **Every part answers three addresses of its own**, never on Away's public one: whether it is alive, whether it is ready, and its measures. It writes what happens as lines a log collector reads.
6. **`away check` tests a setup from end to end** and says what is wrong in words that say what to do: the sign-in service, the database, the files, a machine of each kind, the authority and the domain. The status page shows the same ([chapter 15](../../screens/admin-status.md)).
7. **An update is the chart, applied again with the new version.** The server and the issuer are replaced first and unnoticed. The gateway tells the people in its sessions, waits a time the operator sets, and is replaced ([ADR-0003](./0003-three-parts-with-their-own-rights.md)).
8. **How the chart gets into a cluster is the operator's affair.** Nothing in it assumes a pipeline, a tool or a cluster of one make.

## Consequences

- Whoever may make a namespace's workloads may install Away. Nobody has to be asked for rights over the cluster.
- The network rules make the separation of [ADR-0003](./0003-three-parts-with-their-own-rights.md) something the cluster enforces, where the cluster enforces such rules at all. Where it doesn't, the check of the setup says so.
- An operator's own way of rolling things out keeps working, and Away's configuration lives where theirs does.
- A setting is changed by changing the values and applying the chart, not in the browser.
- The chart has to hold for two kinds of cluster, and is tested on both on every change.

### What this rules out

- **No operator of Away's own** in the first release, and no kinds of its own.
- **No installation that asks for rights over the cluster.**
- **No setting that exists in the browser only.**
- **No installation as one container** yet (`install-as-one-container`, later, from the same program).
