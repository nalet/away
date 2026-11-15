# ADR-0007: On a machine as oneself, by a certificate that lasts minutes

**Status:** Accepted · 2026-11-14. The Windows half is built for a domain whose organisation has an authority of its own ([ADR-0008](./0008-the-authority-for-windows-sign-ins.md)); the shell's half is not built. The Windows half was tried on a real domain on 2026-11-15 before anything was built around it (see [Validation](#validation)).

## Context

Signing in to Away proves to Away who somebody is. It proves nothing to a machine. A Windows machine signs in whoever shows a credential that Windows itself accepts, and a shell whoever shows one its own configuration trusts. Away has no password to show: the person typed it at the sign-in service, if at all.

The model wants a person on the machine as themselves with nothing asked (`be-oneself-on-machine`), from a device the organisation doesn't manage, with an ordinary account, and with an identity of their own for file shares and printers (`reach-network-as-oneself`). What opens a machine should be worth little when stolen (*What opens a machine is worth little when stolen*), and Away must not be able to sign in as the directory's administrators (*It cannot sign in as the directory's administrators*).

For a Windows machine in a domain, the ways to sign someone in without their password are few:

| Way | What comes of it |
|---|---|
| a service account asks the domain for a ticket in the person's name | The machine takes it only in a mode meant for administrators, and the session has no identity for the network. |
| the person's own device passes its sign-in to the domain along | Only a device that is itself in the domain has one to pass. |
| a certificate in the person's name, shown as a smart card | An ordinary sign-in of an ordinary account, from any device. The domain has to trust whoever issues the certificate. |

A shell has a way of the same kind: a certificate for one key, signed by an authority the machine trusts.

## Decision

1. **A certificate made for one session, valid for five minutes.** The gateway makes a key for the session and asks the issuer for a certificate for it. The key never leaves the gateway's memory, and the issuer never sees it.
2. **On a Windows machine in a domain, the gateway uses the certificate twice.** First it proves the person to the domain, over Kerberos, and so passes the sign-in that Windows asks for before it shows a desktop. Then it presents the certificate as a smart card: a card that exists only in the gateway's memory, which the machine reads over the session's connection as it would read a card at the person's desk. Windows signs the person in as it does with a card: an ordinary sign-in, with their own identity for the network.

   ```mermaid
   sequenceDiagram
       participant G as the gateway
       participant I as the issuer
       participant D as a domain controller
       participant M as the Windows machine
       G->>I: a certificate for this session's key, for this person
       I->>I: checks the person's sign-in, the session, the account
       I-->>G: the certificate, valid for five minutes
       G->>D: Kerberos, with the certificate in place of a password
       D-->>G: a ticket for the machine
       G->>M: the sign-in before the desktop, with the ticket
       M->>G: reads the smart card, over the connection
       G-->>M: the card answers with the session's key
       M->>D: signs the person in, as with a card
       M-->>G: the desktop
   ```

3. **A desktop that locked, or one that is opened again, gets a new certificate.** The gateway asks the issuer again and presents the card again (`unlock-without-password`).
4. **On a shell, the issuer signs a certificate for the session's key**, in the format OpenSSH reads, naming the person's account. Where the operator names the gateway's addresses, the certificate is good from those alone. A machine trusts the issuer's public key with one line of its configuration (`make-shells-trust`).
5. **The issuer refuses** a person whose sign-in the sign-in service didn't sign, or signed too long ago; an account that isn't the person's own, as their sign-in names it; a session the server doesn't answer for; and an account, or a group of one, on the list of those it never signs in as. The issuer asks the server whose a session is at the moment it is asked for a certificate, and is handed what the sign-in service signed about that person a moment before, which it checks itself: a session that ended gets nothing from then on, and the server holds no key to sign with. The directory's administrators are on that list unless an administrator of Away takes one off by name. Every certificate is a line of the issuer's own and a record: who, as whom, which machine, its number, until when.
6. **Where there is no identity to be, there is none.** A machine outside a domain and a desktop over VNC are opened with a kept account or with what the person types ([ADR-0010](./0010-secrets-live-in-the-cluster.md)).

### Validation

[Package 3](../../plan/README.md) proves the Windows half with Away's own code and no browser: on a real domain, with a certificate of the domain's own authority, an ordinary account is signed in on a Windows machine with a certificate of five minutes and nothing typed, reads a file share as itself, and an administrator's account is refused. That a domain accepts such a certificate for an ordinary account, from a device outside it, has been seen with other software; that Away's own code does it is what the package shows.

Tried on 2026-11-15, on a real domain and with its own authority: an ordinary account was signed in on a Windows machine with a certificate of five minutes and nothing typed, from a device outside the domain, and read a file share as itself. That an administrator's account is refused there is still to be shown.

## Consequences

- Nothing is typed, from any device, with an ordinary account, and the person is themselves on the network.
- What is stolen with a certificate is worth five minutes.
- A certificate names a person and no machine. What ties it to one session is that its key never leaves the gateway and that it runs out. The quality's proposal says so.
- The domain has to trust an authority for sign-ins, which is a weighty thing to ask of whoever answers for the domain ([ADR-0008](./0008-the-authority-for-windows-sign-ins.md)).
- The gateway has to reach a domain controller, and the clocks have to agree.
- A machine has to allow signing in with a smart card over RDP, as Windows does unless someone turned it off.

### What this rules out

- **No password of a person anywhere in Away**, unless they type one for a machine that wants it.
- **No being oneself over VNC**, and none on a Windows machine outside a domain in the first release.
- **No sign-in as an administrator of the directory**, unless one is named.
- **No certificate that outlasts minutes**, and none that leaves the gateway.
