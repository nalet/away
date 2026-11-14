# ADR-0010: Secrets and keys live in the cluster, and only the issuer reads a kept account

**Status:** Accepted · 2026-11-14. Nothing of it is built.

## Context

Away holds secrets of three kinds:

- **kept accounts**: a name with a password or a key that Away keeps for a machine which knows no directory (`keep-secret`, `use-kept-secret`). Everyone a rule lets in is signed in with it and types nothing;
- **the issuer's authority keys**, which sign the certificates of [ADR-0007](./0007-short-lived-certificates.md);
- **what the parts need for themselves**: the server's secret at the sign-in service, the key for what the database holds encrypted, the database's password, each part's key for signing its calls.

The model wants a kept secret never shown again, to anybody (*A kept secret is never shown again*), what has to stay secret encrypted with the key kept elsewhere (`protect-secrets`), and the part that signs people in apart from the rest (`sign-apart`). The screens already say where a kept account's secret is: *held by the cluster* ([chapter 12](../../screens/admin-machines.md)).

## Decision

1. **Every secret is a secret of the cluster**, in Away's own namespace. Away has no store of secrets of its own.
2. **A machine's file names the secret of its kept account and never holds it.** The operator makes that secret the way they make any other. The issuer tells the server which kept accounts exist, by name alone, so a file that names one that isn't there is refused.
3. **Only the issuer may read the kept accounts and its authority's keys.** The server and the gateway are given their own secrets and no right to read any other.
4. **A kept key never leaves the issuer.** When a shell is signed in with one, the gateway sends the issuer what the machine wants signed, and the issuer signs it.
5. **A kept password is handed to the gateway for the moment of signing in**, because a machine's protocol wants the password itself. The gateway uses it and drops it. It asks with the same proofs as for a certificate: the session the server signed and the person's sign-in.
6. **No secret can be printed.** In the code, a value that holds a secret is of a kind that can't be written to a log, a record or a page. That is checked when Away is built, not when it runs.
7. **What the database holds of secrets is encrypted with a key that is not in the database**: the renewals of people's sign-ins ([ADR-0006](./0006-the-sign-in-is-held-by-the-server.md)). The key is one of the server's own secrets.
8. **Whether the cluster keeps its own secrets encrypted is the cluster's setting.** Away can't see it, and the installation guide says so.

## Consequences

- Away has no vault to protect, to back up or to get wrong in the first release.
- Secrets are made, changed and reviewed with the tools an operator already uses for them, and the files only ever name them.
- One small part can read a kept account. A broken gateway learns the passwords of the sessions it signs in while it is broken, and no key.
- Whoever may read secrets in Away's namespace may read these. The cluster's administrators are trusted.
- A kept account can't be typed into the browser until something there may write a secret (`add-machine-in-browser`, later).
- Changing a kept password on the machine from time to time is left to the operator (`change-kept-secret`, not planned).

### What this rules out

- **No secret in a file, in the database unencrypted, in a record or in a log.**
- **No page and no interface that shows a kept secret**, not even to an administrator.
- **No kept key outside the issuer.**
