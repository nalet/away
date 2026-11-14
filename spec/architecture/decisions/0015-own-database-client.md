# ADR-0015: The server speaks to its database with code of its own

**Status:** Proposed · 2026-11-14. It is built, as the proposal's proof: the server signs in at a PostgreSQL with a proof of its password, asks statements with their values apart from them, and is tried against a real PostgreSQL on every change. Should the owner decide otherwise, a library takes the place of three files behind the store, and nothing else changes.

## Context

Everything Away keeps while it runs is in one PostgreSQL database, which only the server reaches ([ADR-0009](./0009-one-database-and-files.md)). Somebody's code has to speak the database's protocol, and there are two ways to get it.

- **A library.** The client most programs in this language use is complete and long tried. In this build it would bring about 35 libraries with it that Away does not otherwise hold, to a build that names 17 and holds 80. Away would use a small part of it: one connection, statements with values, and text.
- **Away's own.** What the server needs of the protocol is small and published: beginning a connection, signing in, one statement with its values, several statements at once for the database's shape, and reading rows and refusals. It is some 600 lines. The hashes, the stretching of the password and what secures the connection are libraries Away already has.

Three things in the model bear on the choice:

- The quality *It rests on little that others wrote* forbids a library that speaks RDP, VNC or SSH, and wants every other one listed and checked. A database library is allowed by it. This is a choice, not a rule.
- A database is no hostile machine. It is the organisation's own, and the server reaches it over a connection that is secured, or over a network nobody else is on. The reasons that made the machines' protocols Away's own ([ADR-0001](./0001-own-protocol-code.md)) are weaker here.
- The records will be in this database, and no connection is made without one ([ADR-0009](./0009-one-database-and-files.md)). The code that writes them has to be right.

## Decision

1. **The server speaks the database's protocol itself**, version 3.0, as far as it uses it. The code is in the server, behind the store: no other part reaches a database.
2. **A value never travels inside a statement.** Every statement goes with its values apart from it, and the database puts the two together. There is no function that writes a value into a statement's text, so there is no place where a name somebody typed could be read as a statement.
3. **The server proves its password and never sends it** (SCRAM-SHA-256). The database proves in its turn that it knows the password, and a database that does not is not spoken to. One that asks for the password itself, or for its old hash, is refused.
4. **The connection is secured unless the installation says otherwise**, and the database has to show a certificate for its name, of an authority the server is told to trust.
5. **One connection a server, one statement at a time.** A connection the database closed while nothing was asked is opened anew. More connections come when a measure asks for them.
6. **Everything travels as text.** The database writes each value as text and the server reads it as that.
7. **The cryptography is not Away's own**: the hashes, the stretching of the password and what secures the connection are libraries', as everywhere ([ADR-0001](./0001-own-protocol-code.md)).
8. **Every change is tried against a real PostgreSQL** by the pipeline ([ADR-0013](./0013-built-tested-signed-by-the-pipelines.md)): signing in, a wrong password, values that look like statements, refusals, long answers, a secured connection, a certificate of an unknown authority, and the database's shape step by step.

## Consequences

- The build holds no library more. What speaks to the database is read and tested with the rest.
- Being right is Away's to prove. The tests against a real database are that proof, and they grow with every statement the server learns.
- What the protocol offers beyond this is not there: several connections at once, values in the database's own formats, notices as they happen. Each is written when something needs it.
- One connection means a slow statement holds up the next. What Away keeps today is small; the records and the sessions will show whether one is enough.
- A database that only takes the password itself or its old hash can't be used until it is set to the proof. Every PostgreSQL that is still looked after takes it.

### What this rules out

- **No statement put together from what somebody typed.**
- **No signing in at a database with the password itself.**
- **No library between the server and the database that writes statements for it.**
