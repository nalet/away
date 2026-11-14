# ADR-0015: The server speaks to its database with a client that is widely used

**Status:** Accepted · 2026-11-14. The owner decided it the same day, against a client of Away's own that had been built as a proof: a client that many use is the more sustainable one. It is built: the server reaches a PostgreSQL through that client, and is tried against a real one on every change.

## Context

Everything Away keeps while it runs is in one PostgreSQL database, which only the server reaches ([ADR-0009](./0009-one-database-and-files.md)). Somebody's code has to speak the database's protocol, and there are two ways to get it.

- **A library.** The client most programs in this language use is complete and long tried, against every version of the database and every way a database asks its clients to sign in. In this build it brings about 40 libraries with it that Away did not hold before.
- **Away's own.** What the server needs of the protocol is small and published. A client of Away's own was built as a proof, in some 600 lines, with no library more. It signed in with a proof of the password alone, and would have needed more written for every database that asks otherwise: the first real database it met keeps its passwords the old way, which it did not speak.

Three things in the model bear on the choice:

- The quality *It rests on little that others wrote* forbids a library that speaks RDP, VNC or SSH, and wants every other one listed and checked. A database library is allowed by it.
- A database is no hostile machine. It is the organisation's own. The reasons that made the machines' protocols Away's own ([ADR-0001](./0001-own-protocol-code.md)) are that a capability must never wait for someone else's release, and that the code a hostile machine attacks is read and fuzzed with the rest. Neither holds here.
- The records will be in this database, and no connection is made without one ([ADR-0009](./0009-one-database-and-files.md)). The code that writes them has to be right, with every database an installation may bring. Many people's use is the better proof of that than one project's tests.

## Decision

1. **The server speaks to its database through the client that most programs in this language use**, and through the library that lets it secure the connection as Away secures every other. Both are listed with what they are for, and checked on each build, like every library ([the libraries](../../../docs/libraries.md)).
2. **What is Away's own stays small and is in the server, behind the store**: where the database is, how its connection is secured, one connection that is opened anew when it is gone, and the database's shape. No other part reaches a database.
3. **A value never travels inside a statement.** Every statement goes with its values apart from it, and the database puts the two together. The store has no way to write a value into a statement's text.
4. **The connection is secured unless the installation says otherwise**: the database then has to show a certificate for its name, of an authority the server is told to trust. An installation can say that the connection is encrypted and the certificate is not checked, for a database that shows a certificate no authority signed, or that it is not secured at all. Both are said by name in its values, and neither is what Away does by itself.
5. **The server signs in the way the database asks**: with a proof of its password where the database takes one, which is what a database of today asks for, and otherwise as it is set.
6. **One connection a server, one statement at a time.** More connections come when a measure asks for them.
7. **Every change is tried against a real PostgreSQL** by the pipeline ([ADR-0013](./0013-built-tested-signed-by-the-pipelines.md)): what Away keeps, the database's shape step by step, a connection the database closed, a wrong password, a secured connection, a certificate of an unknown authority, and a database that keeps passwords the old way.

## Consequences

- The part of Away that has to be right with every database is code that many others run every day, and its mistakes are found and fixed by more people than read Away.
- The build holds about 40 libraries more, each listed, licensed and checked for known holes on every build. *It rests on little that others wrote* still holds by its target, and by less of a margin.
- A release of the client that fixes a hole is taken with an update of one line.
- What the database offers beyond today's use is there when something needs it: several connections, values in the database's own kinds, notices.
- Over a connection that is not secured, a database set to ask for the password itself is given it. The installation guide says to let the database ask for a proof, as it does unless it is set otherwise.

### What this rules out

- **No statement put together from what somebody typed.**
- **No client of Away's own for the database.** The proof is not kept.
- **No library between the server and the database that writes statements for it.**
