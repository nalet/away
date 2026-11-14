# Connections

What reaches what: every connection between two components, by the layer of
the one it starts from. A connection that is not here is one Away doesn't
make, and the chart's network rules refuse it
([ADR-0003](./decisions/0003-three-parts-with-their-own-rights.md)).

What each kind of connection means is on the
[front page](./README.md#the-state-of-the-model). A connection comes with the
first release unless its note begins with *There* or *Later*.

<!-- generated: connections -->
## In the browser

| From |  | To | On | Note |
|---|---|---|---|---|
| `web` | loads | `server` | / | The pages' files: nothing is loaded from anywhere else. |
| `web` | calls | `server` | /api | With the sign-in's cookie; a request that changes something also carries a header that only Away's own page can set. |
| `web` | sends to | `sign-in-service` | the organisation's sign-in page | The browser goes there to sign in and comes back with a code for the server. |
| `web` | uses | `session-view` | — | — |
| `session-view` | streams | `gateway` | /session | One WebSocket a session, with the sign-in's cookie and a ticket the server gave for this session. |
| `session-view` | uses | `shell-screen` | — | — |
| `session-view` | uses | `wire` | — | — |

## Away's parts

| From |  | To | On | Note |
|---|---|---|---|---|
| `server` | signs in | `sign-in-service` | OpenID Connect | A code for tokens, their renewal, the sign-out. |
| `server` | writes | `database` | the database away | — |
| `server` | reads | `files` | the mounted files | At its start and on every change. |
| `server` | reads | `cluster-secrets` | its own: the client's secret, the key for what the database holds encrypted, its signing key, the password of the account that reads the farms' feeds | Mounted. |
| `server` | fetches | `farm-feed` | HTTPS | Only where the files name a farm: what the farm publishes, each thing's icon and how it is reached, every five minutes, as an account that opens nothing. |
| `server` | calls | `gateway` | internal | End a session, test a machine, announce an update. |
| `gateway` | calls | `server` | internal | Take over a session with its ticket; report its steps, its use and its end. |
| `gateway` | asks | `issuer` | internal | For one session: a certificate for its key, a kept password, or a signature with a kept key. |
| `gateway` | opens | `machines` | RDP | Port 3389 unless the machine's file says otherwise. |
| `gateway` | opens | `machines` | VNC | Port 5900 unless the machine's file says otherwise. |
| `gateway` | opens | `machines` | SSH | Port 22 unless the machine's file says otherwise. |
| `gateway` | calls | `domain` | Kerberos | Only where the configuration names a domain: it proves the person with their certificate and gets the ticket for the machine. |
| `gateway` | reads | `cluster-secrets` | its own signing key | Mounted. |
| `gateway` | uses | `rdp` | — | — |
| `gateway` | uses | `vnc` | — | — |
| `gateway` | uses | `ssh` | — | — |
| `gateway` | uses | `picture` | — | — |
| `gateway` | uses | `wire` | — | — |
| `issuer` | fetches | `sign-in-service` | the provider's public keys | To check a person's sign-in itself. |
| `issuer` | reads | `cluster-secrets` | its authority's keys and the kept accounts | The one part that may read the kept accounts. |
| `issuer` | asks | `authority` | one of the authority's published interfaces | Only where the configuration says that the organisation's own authority issues; a proof settles which interface. |
| `issuer` | calls | `server` | internal | What is public of it: its state, the names of the kept accounts that exist, its authority's certificate and the list of those it withdrew. |
| `server` | puts out | `log-system` | lines on its output | What happens, and every record as it is written. |
| `gateway` | puts out | `log-system` | lines on its output | — |
| `issuer` | puts out | `log-system` | lines on its output | A line of its own for every sign-in it makes. |
| `gateway` | opens | `relay` | — | Later. The relay connects out to the gateway and machines are opened through it. |

## Away's own protocol code

| From |  | To | On | Note |
|---|---|---|---|---|
| `rdp` | uses | `windows-sign-in` | — | — |

## What Away rests on

| From |  | To | On | Note |
|---|---|---|---|---|
| `way-in` | passes on | `server` | / | — |
| `way-in` | passes on | `gateway` | /session | The chart sets the way in's time limits for connections that stay open. |

## Outside Away

| From |  | To | On | Note |
|---|---|---|---|---|
| `domain` | fetches | `server` | /authority | Only where the configuration says that Away's own authority issues: the list of certificates it withdrew, which a domain asks for before it accepts one. |
| `monitoring` | measures | `server` | /metrics | — |
| `monitoring` | measures | `gateway` | /metrics | — |
| `monitoring` | measures | `issuer` | /metrics | — |

## Making and shipping it

| From |  | To | On | Note |
|---|---|---|---|---|
| `chart` | fetches | `registry` | the image | There. |
| `pipelines` | publishes | `registry` | the image and the chart, signed | There. |
| `pipelines` | tests | `test-servers` | on every change | — |
| `site-pipeline` | publishes | `docs` | the site | There. |
| `test-program` | uses | `rdp` | — | — |
<!-- end: connections -->
