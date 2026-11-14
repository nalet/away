# ADR-0003: Three parts, each with its own rights

**Status:** Accepted · 2026-11-14. The three parts run, each with its own rights, and the cluster refuses what the model doesn't list. The gateway asks the server over TLS, at an address of the server's own that an authority of the installation certifies; that a call is also signed with the caller's own key is not built. The issuer does nothing yet.

## Context

Away opens the door to every machine it knows ([Trust it](../../goals/README.md#trust-it)). Three things in it are worth stealing:

- **people's sign-ins**, which say who somebody is;
- **the keys that sign people in on machines**, and the kept accounts;
- **the sessions themselves**: what is on a screen, what is typed.

One of its tasks is far more exposed than the others: reading what a machine sends. A machine can be hostile, or can have been taken over, and the gateway reads its bytes.

If one process did everything, breaking it at its most exposed point would give all three. The capability `sign-apart` already asks that the part which signs people in runs apart from the rest, with rights of its own.

## Decision

Away runs as three parts. Each reaches only what its task needs.

| Part | Its task | It reaches | It never reaches |
|---|---|---|---|
| the server | answers the browser's pages and requests; holds people's sign-ins; reads the files; decides who reaches what; writes the records | the database, the sign-in service, the gateway, the feed of a farm the files name ([ADR-0014](./0014-a-farm-says-what-it-publishes.md)) | a machine, beyond that feed; the issuer; a kept account; the authority's keys |
| the gateway | holds the sessions: one connection to the browser and one to the machine each | the machines, a domain's controllers, the issuer, the server | the database; the sign-in service; the cluster's secrets, beyond a key of its own |
| the issuer | answers one question: sign this person in on this machine | the kept accounts and its authority's keys, the sign-in service's public keys, the organisation's authority where it issues, the server | a machine; the database; the browser |

1. **The issuer trusts neither of the others.** Before it signs anything it checks three things itself: that the sign-in service signed the person's sign-in, and recently; that the server signed the session it is asked for; and that the account is not one it never signs in as ([ADR-0007](./0007-short-lived-certificates.md)). It writes a line of its own for every sign-in it makes.
2. **Nobody but the gateway asks the issuer for anything.** The issuer tells the server what is public of itself: whether it is well, which kept accounts exist, by name alone, and its authority's certificate with the list of those it withdrew. The server never calls it.
3. **The gateway holds nothing that outlasts a session.** What signs a person in is in its memory for as long as the sign-in takes, or for as long as the session needs it, and nowhere else.
4. **Every call between parts is made over TLS and is signed with the caller's own key.** The chart gives each part its key and the others' public halves.
5. **The cluster refuses what the model doesn't list.** The chart sets the cluster's network rules from [the connections](../connections.md): the gateway alone may reach machines, the server alone the database, the issuer alone the authority.
6. **The first release runs one of each.** An update replaces the gateway, and with it the connections of its sessions: the people in them are told as it begins, their desktops are back by themselves, and their shells end ([chapter 16](../../screens/errors.md)). The server and the issuer are replaced without anyone noticing. More than one gateway comes with `run-several`.

## Consequences

- Breaking the gateway, where a hostile machine would try, gives no signing key, no kept account, no database and no sign-in of anyone who has no session there.
- Breaking the server gives the sign-ins of the people who are signed in to Away, and no way to be anyone else: the issuer wants the sign-in service's own word for each person, and refuses the accounts on its list whatever the server says.
- The issuer is small enough to be read whole by a reviewer.
- A sign-in that Away made shows twice: in the records, and in the issuer's own line. Hiding one takes breaking both parts.
- Three workloads to run instead of one, keys between them to hand out, and one more call when a session signs in.
- The gateway can't write a record itself. It reports to the server, so a connection is only made once the server has written its record ([ADR-0009](./0009-one-database-and-files.md)).

### What this rules out

- **No part that reaches both the machines and the database.**
- **No way to a machine from the server**, except to the feed in which a farm says what it publishes ([ADR-0014](./0014-a-farm-says-what-it-publishes.md)). Testing a machine is done by the gateway, when the server asks.
- **No session through an update of the gateway in the first release**, beyond desktops coming back by themselves.
- **No separation where Away runs as one container** (`install-as-one-container`, later): the three parts are then one process, and its guide will say so.
