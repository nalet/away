# ADR-0014: A farm says itself what it publishes, and the server asks it

**Status:** Accepted · 2026-11-14. The owner chose this way the same day, among three that were put to them. It is built: the server reads a farm's feed, what the farm publishes is in force beside the files ([the machines and the rules](../../../docs/machines.md#a-farm)), and a desktop the farm publishes opens through its broker.

## Context

A farm of Windows machines publishes desktops and programs through a broker. Which programs there are is decided on the farm, by whoever runs it, and it changes: a program is published, renamed, withdrawn.

Away knows machines from files ([ADR-0009](./0009-one-database-and-files.md)). A farm written into the files, program by program with each icon, is right on the day it is written. A program published the day after is missing in Away until somebody notices and changes a file, and one that was withdrawn still shows and fails when it is opened. The files would be a second copy of something the farm already says.

The farm says it in a feed: its web access lists, to an account that signs in, every desktop and program that account may use, each with its name, its icon, and a file that says how it is reached. It is the same list the farm's own web page and the clients of Windows show.

Three ways were put to the owner:

- **A. Away asks the feed.** The files name the farm and its feed; everything else is the farm's.
- **B. A job beside Away writes the files** from the farm, on a schedule. Nothing of Away changes, and there is one more thing to run, with its own account and its own failures, between the farm and the files.
- **C. The files stay written by hand**, with a tool that says where they differ from the farm.

Two things in the architecture bear on A:

- [ADR-0003](./0003-three-parts-with-their-own-rights.md) gives the server no way to a machine. A farm's web access is a machine of the farm.
- [ADR-0010](./0010-secrets-live-in-the-cluster.md) keeps the accounts that open machines with the issuer alone. Reading the feed takes an account of the organisation's directory, and the server is the part that reads.

## Decision

1. **The files name a farm by its feed, and nothing of what it publishes.** A farm is an entry of the files like a machine: its name, the address of its feed, and the groups everything it publishes is in.
2. **The server asks each farm's feed every five minutes**, and takes what it says: a desktop becomes a machine by the name the farm gives it, the programs become the programs of one machine that has the farm's name, each with the farm's own icon. What a farm publishes is in force beside what the files say, in the place the farm is written at among the machines, and the rules give it like everything else.
3. **The files go first.** A desktop that a farm publishes under the name of a machine in the files is left out, and the administration names it. A farm can't take a machine's place.
4. **A feed that can't be read changes nothing.** What the farm published before stays, and the administration says why the feed could not be read. What a farm publishes is held in memory alone: after the server starts, a farm shows nothing until its feed was read once.
5. **The server reaches a farm's web access, at the address the files name, and nothing else of a farm.** It follows no address that leads elsewhere, opens no desktop and no program, and speaks no protocol of a machine. This is the one way from the server to a machine, and it amends [ADR-0003](./0003-three-parts-with-their-own-rights.md) by exactly that.
6. **The account that reads the feeds is the server's own secret**, a secret of the cluster like its others ([ADR-0010](./0010-secrets-live-in-the-cluster.md)). It is no kept account: nobody is signed in on a machine with it, and the issuer never sees it. One account reads every farm of an installation.
7. **The account signs in the way the farm's web access asks**, with NTLM inside the secured connection. The hashes and the cipher of it are libraries', as everywhere ([ADR-0001](./0001-own-protocol-code.md)); the messages are the ones Away already writes for signing in before a desktop.
8. **What the account sees is what Away offers.** A farm shows an account what that account may use. Away gives it to the people its rules name, and whether one of them may open a program stays the farm's to say, when they open it.
9. **The files name the machines that serve a farm, and the gateway goes on to no other.** A feed says what a farm publishes and not which machines serve it, and a farm's broker sends each connection on to one of them. The farm's entry names those machines, and what the farm publishes carries them like a collection written in the files: the gateway goes on to one of them, or to the broker's own address, and nowhere else (`follow-broker`).

## Consequences

- A program the farm publishes shows in Away within minutes, with its icon, and one it withdraws is gone. Nobody changes a file for it.
- The files hold what only the operator knows: that there is a farm, where its feed is, and who is given what it publishes.
- The server holds the password of an account of the directory. Whoever breaks the server learns it, and can read what the farm publishes. An account made for this alone, which may sign in on no machine, gives them nothing more; the installation guide says to make it so.
- The server reaches addresses outside the cluster beside the sign-in service. The chart's network rules name the farms' port, and an operator can narrow them to the farms' addresses.
- What a farm publishes is not reviewed like a file. Who may see it still is: the rule that gives the farm's group is in the files. So are the machines a session may land on.
- A machine that is added to a farm is added to the farm's entry too. Until it is, a connection the broker sends there is not made, and the page says so.
- A farm whose web access takes no NTLM, or sends its readers to another address to sign in, is not read. The administration says which of the two it is.
- A person may be shown a program that the farm won't open for them, where the account sees more than they may use. The farm refuses it when it is opened.

### What this rules out

- **No list of a farm's programs in the files** beside a feed that is read: one of the two would be wrong.
- **No way from the server to a machine other than a farm's feed.** Testing a machine and opening one stay the gateway's.
- **No reading as the person.** The feed is read as one account, at the server's pace, and never with the sign-in of whoever looks at a page.
- **No change on the farm.** Away reads what the farm publishes. Publishing a program is done on the farm.
