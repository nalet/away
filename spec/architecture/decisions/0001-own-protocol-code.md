# ADR-0001: Away speaks RDP, VNC and SSH with code of its own

**Status:** Accepted · 2026-11-14. Its hardest part is judged by a proof before anything is built around it (see [Validation](#validation)); the first proof's pictures matched on all three kinds of machine on 2026-11-14 ([what it found](#what-the-first-proof-found)), and the owner's word on it is open. On the owner's word of the same day the proof's machines are Windows 11, Windows Server 2025 and Linux, and the first release also takes the licence a machine for many people hands out and follows a broker to the machine that serves a collection.

## Context

Away opens a machine by speaking the machine's own protocol: RDP to a Windows machine, VNC to a desktop of another system, SSH to a shell. Somebody's code has to speak them, and there are two ways to get it.

- **Someone else's.** Libraries for all three exist. A gateway built on one does what the library does, and gets a new ability when the library does. What the first release wants of RDP is where libraries are thinnest: signing in before the desktop with a certificate in place of a password, a smart card that is no card, single programs whose windows the machine reports one by one.
- **Away's own.** Every capability on the list is then in reach, at a cost that is known to be large.

Three things in the model bear on the choice:

- The code that reads what a machine sends is where a hostile machine attacks Away. The quality *A hostile machine cannot harm Away or the person's browser* ([Trust it](../../goals/README.md#trust-it)) wants that code fuzzed on every change and reviewed by someone outside. Both are easier for code that was written for this purpose and is built with the rest.
- The quality *It rests on little that others wrote* proposes that no library in a release speaks a remote protocol.
- A shell's screen is worked out in the browser ([ADR-0005](./0005-the-shells-screen-in-the-browser.md)), so that code has to run there too.

RDP is the large one. It is a family of published specifications: the connection and its security, the graphics pipeline with its picture formats, keys and pointer, the clipboard, the display's size, single programs, the smart card channel, and the sign-in before the desktop with Kerberos under it. VNC and SSH are small beside it.

## Decision

1. **Each protocol is a library of Away's own**, in the repository, built and released with the rest.

   | Library | Speaks | What the first release needs of it |
   |---|---|---|
   | the RDP code | RDP | a desktop and single programs: the connection over TLS, the licence a machine for many people hands out, the broker's word on which machine serves a collection, the graphics pipeline and its picture formats except video, keys and pointer, text on the clipboard, the display's size and density, the windows of programs |
   | the Windows sign-in | Kerberos, the sign-in before the desktop, the smart card channel | signing in with a name and a password, and with a certificate presented as a smart card that exists only in memory ([ADR-0007](./0007-short-lived-certificates.md)) |
   | the VNC code | RFB | a desktop: signing in with a password, the common picture encodings, the pointer's shape, the desktop's size, text on the clipboard |
   | the SSH code | SSH | one shell: signing in with a password, a key or a certificate, checking the machine's key, the shell's size |
   | the shell's screen | what a shell writes | [ADR-0005](./0005-the-shells-screen-in-the-browser.md) |

2. **No input or output of their own.** A library takes bytes and gives bytes and events. It opens no connection, reads no clock and starts no thread; the gateway does that around it. The same code is therefore tested with recorded bytes, fuzzed, and run wherever it is needed, the browser included.
3. **No unsafe code in them.** The language's guarantees hold for every line that reads what a machine sent.
4. **The cryptography is not Away's own.** Ciphers, hashes, signatures, key exchange and TLS come from libraries that are widely used and reviewed. So do reading and writing certificates, and compressing data. Away writes the protocol around them, never the mathematics.
5. **Other implementations are counterparts in the tests, never parts of a release.** Every change runs against a real server of each protocol ([ADR-0013](./0013-built-tested-signed-by-the-pipelines.md)).
6. **Only what a capability needs.** A part of a protocol that no capability of the release asks for is not written: SSH forwards no ports and moves no files, VNC carries no files, RDP carries no sound, no printing and no drives in the first release.

### Validation

Own RDP is judged early and apart from the rest, by two proofs that need no browser ([the work packages](../../plan/README.md)):

- **Package 2** builds the RDP code far enough for a test program to sign in with a typed name and password and to save the desktop's picture, on Windows 11, on Windows Server 2025, and on an RDP server on Linux. The picture has to be the machine's screen, pixel for pixel.
- **Package 3** does the same for the sign-in with a certificate, on a real domain.

The design holds if both gates are met. If the first is not, the owner chooses between more time and the other way: an RDP library that exists, behind the seam the gateway already uses for its own code. The target of *It rests on little that others wrote* then changes, and nothing else of the architecture does.

### What the first proof found

The test program's picture was the machine's screen, pixel for pixel, on Windows 11, on Windows Server 2025 and on an RDP server on Linux. Three things came out of it that the packages after it build on:

- **A Windows Server draws at 16 bits a pixel** for a client that takes its picture as rectangles, unless the server is set otherwise; a workstation draws at 32. The picture of a server is therefore the server's screen with each colour cut to 16 bits, and it is that pixel for pixel. A server's full colours come with the graphics pipeline, which package 10 builds.
- **A machine for many people asks every client to take a licence.** One that counts the people who use it takes the client's answer and goes on: that is proven on Windows Server 2025. One that counts devices hands a licence over: that is tested against a stand-in, and not yet against a real machine. Keeping a licence and showing it again belongs to package 10.
- **Taking a licence has one step that no cryptography library offers.** The protocol seals a secret to the machine's key with RSA as it is, without the padding every library insists on. A library for large numbers does that arithmetic; the cipher and the hashes of the exchange are libraries'. Nothing rests on the step, because the whole exchange travels inside the secured connection. It is the one place where *No cryptography of Away's own* is read by its purpose and not by its letter.

## Consequences

- One body of code to read, fuzz and review, with one history.
- A capability is never out of reach because a library lacks it, and never waits for someone else's release.
- RDP is the largest single part of the build, and the part most likely to take longer than planned.
- Working with every machine has to be earned. Each Windows version and each server of another system behaves a little differently, and each difference found becomes a test.
- A security hole in this code is Away's to find and to fix. Nobody else looks at it unless they are asked to, which is why the first release asks.

### What this rules out

- **No library that speaks RDP, VNC or SSH in a release.** In the tests, as the other end, yes.
- **No cryptography of Away's own.**
- **No further protocol cheaply.** Each one is a library to write.
- **No part of a protocol on stock.** What no capability needs is not written.
