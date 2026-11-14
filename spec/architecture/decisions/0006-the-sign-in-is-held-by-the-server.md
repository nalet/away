# ADR-0006: The sign-in to Away is held by the server

**Status:** Accepted · 2026-11-14. Nothing of it is built.

## Context

People sign in where their organisation already signs in, over OpenID Connect (`sign-in`, `use-own-provider`). Away keeps no people of its own: who somebody is and which groups they are in comes from the sign-in service (`take-people-from-provider`). From that follow:

- The sign-in renews itself for as long as the organisation allows, and nothing shows (`stay-signed-in`).
- When it can't be renewed, Away asks for a new one and keeps what is open for a time the operator sets ([chapter 3](../../screens/start.md), figure 3.5).
- A right that is taken away is gone within a minute, as proposed (*A right that is taken away is gone at once*, `end-on-lost-right`).
- What a person's sign-in is worth: with it, Away signs them in on machines.

The tokens that make up a sign-in can live in the page, as many web apps have it, or in the server.

## Decision

1. **The server signs people in**, as a client of the sign-in service with a secret of its own: the browser is sent to the organisation's sign-in page and comes back with a code, which the server exchanges for the tokens.
2. **The tokens stay in the server.** The browser gets a cookie that names the sign-in and says nothing else. No script can read it, it travels over TLS only, and it is sent to Away alone.
3. **Every request carries the cookie**, and so does a session's connection. A request that changes something also carries a header that only Away's own page can set, so another site can't make a person's browser ask for it.
4. **Who somebody is comes from the sign-in service every time.** Their name, their groups and the name of their account on machines are read from what the service signed. Which entry names each is a setting (`name-account-claim`). The roles of the administration are groups of the service (`name-roles`).
5. **The server renews each sign-in before it runs out, and at least once a minute while its person has a session open.** What comes back is checked against the rules again. A session whose rule no longer gives it ends, and its record says why.
6. **A renewal that the sign-in service refuses stops the person's sessions at once.** They take no key and no click, and nothing new of them is shown. The person is asked to sign in again, beside the session. With a new sign-in everything is as it was. Without one, the sessions end when the operator's time has passed. Away can't tell an account that is gone from an organisation that wants its password typed again: both refuse the renewal. So both get the same answer.
7. **Signing out** ends the sign-in in the server, and with it the cookie, and ends the sessions as [chapter 3](../../screens/start.md) says.

## Consequences

- There is no token in the page. A script that got into the page can't carry a sign-in away.
- A right follows the sign-in service within a minute, with any service that speaks OpenID Connect, and with nothing set up there beyond the client.
- One place ends a sign-in, and knows every session that rests on it.
- The server holds what renews everyone's sign-in. It is kept encrypted, with the key outside the database ([ADR-0010](./0010-secrets-live-in-the-cluster.md)), and it is why the server reaches no machine and signs nobody in itself ([ADR-0003](./0003-three-parts-with-their-own-rights.md)).
- The sign-in service is asked once a minute for each person with an open session.
- A person whose account was closed keeps their sessions, unusable, until the operator's time has passed. An operator who wants them ended at once sets that time to nothing, and then a password that has to be typed again ends sessions too.

### What this rules out

- **No token in the browser**, in a page or in its storage.
- **No people, groups or passwords of Away's own.**
- **No use of Away's interface by another program with a person's sign-in** in the first release. Doing by a script what an administrator does (`do-by-script`) comes later, with a sign-in of its own kind.
- **No way in while the sign-in service is down** (`get-in-without-provider`, later).
