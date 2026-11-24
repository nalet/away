# ADR-0017: Away keeps the passwords of machines itself, sealed so that only the issuer opens them

**Status:** Accepted · 2026-11-24. Built for the password of a desktop over VNC. It replaces the first point of [ADR-0010](./0010-secrets-live-in-the-cluster.md), that Away has no store of secrets of its own; the rest of ADR-0010 stands. A kept name and password for RDP, and a kept key for SSH, are not built.

## Context

The owner asked for a store of the passwords Away signs people in with, "like if we have n Macs", and for it to work wherever Away is installed: "this product must also work standalone, as it is open source", and "scalable if for example we want to have multiple instances".

[ADR-0010](./0010-secrets-live-in-the-cluster.md) put every kept secret into a secret of the cluster, which the machine's file names. That holds no password Away doesn't already depend on, but an operator has to make and change a secret of the cluster for every machine, with tools outside Away, and an installation without such tools has no way at all. Products of this kind keep connection passwords in their own database and let an administrator type them in a page.

What has to stay true is what ADR-0003 and ADR-0010 protect: the server, which holds the sign-ins and the database, and the gateway, which reads what machines send, never hold a kept password beyond the moment the gateway signs a person in; nothing shows one again; and a stolen database or a broken server gives none.

## Decision

1. **Away keeps the passwords in its own database, sealed.** An administrator types a machine's password once, on the administration's page of passwords. The browser seals it before it is sent: it makes a key for this password alone, encrypts the password with that key (AES-256-GCM), bound to the name it is kept under, and encrypts the key to the issuer's public key (RSA-OAEP with SHA-256). The server checks that what arrives is sealed to the issuer's key and keeps it as it is. Neither the server nor the database ever holds the password.
2. **Only the issuer holds the key that opens them.** It is the installation's own: the chart makes it once, at the first install, as a secret of the cluster that is kept when Away is removed, and every issuer of the installation reads the same. The server reads the key's certificate, never the key, and learns the key from nothing else: a part that told the server a key of its own would have new passwords sealed to it.
3. **A password is opened for one session, as a kept secret of the cluster is** (ADR-0010, point 5). The gateway asks the issuer for the session's machine; the issuer asks the server whose the session is and checks the sign-in service's signature itself; the server hands the sealed password over with its answer; the issuer opens it, under the name the machine's entry gives and no other, has the server write the record, and hands it to the gateway for that sign-in alone.
4. **A machine's entry names the password the same way, wherever it is kept**: `signs-in: { kept: <name> }`. Away's own store is looked in first, then a folder of secrets of the cluster where the installation names one, as ADR-0010 has it.
5. **A password is never shown again.** It is replaced or removed. Each time it is kept, replaced, removed or opened is a record, with who, which name, and nothing of the password.
6. **More than one of each part keeps one store.** The database is the one all servers share, and the key the one all issuers read: any server takes a password, and any issuer opens it.
7. **The key can be replaced without a password being lost.** The installation gives the issuer the new key first and the old ones after it: new passwords are sealed to the first, and every one of them opens what was sealed to it. A password sealed to an old key is kept anew to move it to the new one.

## Consequences

- Nothing beyond Away is needed to keep the passwords of n machines, and an administrator manages them where they manage Away.
- A stolen database, a copy of it, or a broken server gives no password. A broken gateway learns the passwords of the sessions it signs in while it is broken, as with ADR-0010.
- The key is as precious as the passwords: without it none opens again. It has to be backed up with the database, and the installation guide says so.
- Whoever may read secrets in Away's namespace may read the key. The cluster's administrators are trusted, as with ADR-0010.
- The browser has to seal: Away's pages use the browser's own cryptography (WebCrypto), which every browser Away supports has.

### What this rules out

- **No password in the database or in a request as it is**, and no page or interface that gives one back.
- **No key that a part announces.** The server trusts the certificate the installation gives it, and nothing a part says.
- **No password sealed to one issuer alone.** Every issuer of an installation opens every password.
