# ADR-0009: Machines and rules are files; everything else Away keeps is in one database

**Status:** Accepted · 2026-11-14. The files are built: the server reads machines and rules from them, checks a set as one, and applies it whole or not at all ([the machines and the rules](../../../docs/machines.md)). The database is begun: the server reaches one, changes its shape itself, and keeps what people star there ([ADR-0015](./0015-a-widely-used-database-client.md) says how it speaks to it). The records are not built yet, so neither outcome of a change to the files is a record so far: each is a line on the server's output.

## Context

Away keeps things of two kinds.

- **What an operator decides**: the machines, their groups, the rules, the settings. The model wants them as files that *can live in a repository and be reviewed like code* (`keep-as-files`), applied as they change, with a refused change shown and what was in force kept (`see-refused-change`). In the first release the browser changes none of them.
- **What comes about while Away runs**: people's sign-ins, each person's settings, favourites and last used machines, the identity each machine showed when it was first tested, the sessions as they run, and the records.

For the second kind the records set the bar. No connection is made without one (*No connection is made without a record*), nobody inside Away can change one, and every one also leaves Away as it is written (*A record cannot be changed afterwards*). An administrator reads and searches them in the browser (`read-records`).

Later come more than one instance (`run-several`) and backups (`back-up`).

## Decision

1. **Machines, groups, rules and settings are files.** The chart mounts them into the server, from its own values or from a ConfigMap the operator keeps. The server reads them when it starts and whenever they change.
2. **A change is applied whole or not at all.** The server checks the new set as one: every machine, every group a rule names, every secret a machine names. If anything is wrong the whole set is refused, the reason shows on the administration's pages and on the status page, and what was in force stays. Either outcome is a record.
3. **Everything else is in one PostgreSQL database**, and only the server reaches it. The operator brings one, or the chart runs one beside Away.
4. **A connection's record is written before the connection is made.** The gateway only opens a machine once the server has confirmed the record. A database that can't be written to means no new session, while the sessions that are open go on.
5. **Records are only ever added.** The server's account in the database may add records and read them, and may neither change nor remove one. Each record is also put out as a line on the server's output at the moment it is written, so the organisation's log system holds a copy that Away can't touch (`send-records-on`).
6. **The server changes the database's shape itself** when a new version starts, forward only.

## Consequences

- The register is text: it is compared, reviewed and rolled back with the tools an operator already has, and a mistake is refused before it takes effect.
- One store that is known everywhere, with tools for backing it up and for running more than one server against it later.
- The database is the one thing in an installation that isn't Away's own and that somebody has to look after.
- Without the database nobody opens a machine. That is the price of no connection without a record.
- A machine can't be added in the browser until something there may write (`add-machine-in-browser`, later). The files are then one source and the browser another, which is a question for that capability's own decision.
- Going back a version is not covered: the database's shape only moves forward (`go-back-a-version`, later).

### What this rules out

- **No kinds of Away's own in the cluster for machines and rules.** Defining a kind takes rights over the whole cluster, which Away doesn't ask for ([ADR-0012](./0012-one-chart-no-special-rights.md)).
- **No store inside the server's own container.**
- **No way to change or remove a record**, for anyone, through Away.
- **No secret in a file or in a record** ([ADR-0010](./0010-secrets-live-in-the-cluster.md)).
