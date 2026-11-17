# The specification

The specification says what Away is for and what it has to do, before any of it
is built. It is written for the people who decide what Away becomes and for the
people who build it, and every release is checked against it.

> **Agreed on 14 November 2026.** The owner reviewed this specification and
> agreed to it: the goals, the cut between the first release and later, the
> screens, a target for every quality, every decision, and the work packages in
> their order. It stays a model: where building shows a row wrong, the row is
> changed here first, and what is built follows it.

It has five parts:

| Part | What it specifies |
|---|---|
| [what it is for](./goals/README.md) | the goals: what people want, what that takes, what being open adds; the qualities each has to be met with |
| [capabilities](./capabilities/README.md) | what Away does, for whom, for which kind of connection, and when: in the first release, later, or not at all; what is expected of it, and what is left out |
| [screens](./screens/README.md) | what Away looks like: every screen of the first release as a drawing, with its parts, its rules and its words |
| [architecture](./architecture/README.md) | how Away is built: its components, what each reaches and keeps, which of them carries each capability, and the decisions behind it, each a record of its own |
| [work packages](./plan/README.md) | in what order the first release is built: what each package delivers, and the gate that ends it |

Each part rests on the ones before it. A component carries capabilities, a
work package delivers them, and a program checks that none of the first
release is left without either.

Away's pages take their look and their rules from the
[nalet design system](https://nalet.github.io/design-system/), which holds what
every product shares. The specification doesn't repeat them.

## The state of the model

<!-- generated: state -->
|  | Rows | Of them |
|---|---|---|
| Goals | 8 | 3 what people want, 4 what that takes, 1 what being open adds |
| Qualities | 35 | 35 with a target, 35 with a proposal |
| Expectations | 99 | 20 there, 23 partly, 16 first release, 5 first release in part, 27 later, 8 not planned |
| Capabilities | 205 | in 19 areas: 47 built, 21 partly built, 50 first release, 73 later, 14 not planned |
| Figures | 84 | in 15 chapters; they show 61 of the 68 capabilities the first release holds for a person or an administrator |
| Components | 31 | in 6 layers: 11 there, 19 first release, 1 later |
| Connections | 46 | of 16 kinds |
| Decision records | 16 | 1 proposed, 15 accepted |
| Work packages | 16 | 2 proofs and 14 builds: 10 agreed, 1 begun, 5 done |
<!-- end: state -->

A capability counts once, by the soonest it is planned for any kind of
connection. How the words are meant is in
[how to read the capabilities](./capabilities/README.md#how-to-read-it) and
[how to read the expectations](./capabilities/expectations.md#how-to-read-it).

## Before the first release is built

The five parts say what the first release is, how it is built and in what
order, and every decision is accepted. What still stands between them and a
release:

| Open | What it takes |
|---|---|
| Two proofs | Whether Away's own code reaches a Windows desktop, and whether it signs an ordinary account in there with a short-lived certificate that a domain accepts. The first release rests on both, so [work packages](./plan/README.md) 2 and 3 prove them with no browser, before anything is built around them. |
| Machines to test against | The tests run against a real server of each protocol. A build can start some of them by itself: a shell, a desktop over VNC, a desktop over RDP on Linux. Windows machines it cannot, and the sign-in needs a domain that exists for testing. [What the build needs first](./plan/README.md#what-the-build-needs-first) lists them. |
| The work packages | [Sixteen](./plan/README.md), each ending at a gate. A package begins when the owner names it, and the table there says how far each is. |

The frame's components, the top bar, the rail and a phone's tab bar, are
drafts in the design system. Away builds its own from those drafts and offers
them back ([ADR-0011](./architecture/decisions/0011-the-web-app.md)), so
nothing waits for them.

## How it is kept

Written by hand, counted by a program. The model is twelve tables, kept as CSV
files beside these pages; `scripts/model.py` checks them and writes every table
and number on the pages from them, and the site draws its maps from the same
data. The screens are drawn in HTML files beside their chapters, and
`scripts/render-mockups.mjs` makes their pictures. [Changing the model](../docs/model.md)
says how.
