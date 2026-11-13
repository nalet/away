# The specification

The specification says what Away is for and what it has to do, before any of it
is built. It is written for the people who decide what Away becomes and for the
people who build it, and every release is checked against it.

> **A first cut, under review.** Nothing of Away is built yet. Every row of this
> specification is a proposal until the owner has reviewed it: a goal can be
> struck, a capability moved between the first release and later, a target set.
> What is built follows from the reviewed model, and not before.

It has three parts:

| Part | What it specifies |
|---|---|
| [what it is for](./goals/README.md) | the goals: what people want, what that takes, what being open adds; the qualities each has to be met with |
| [capabilities](./capabilities/README.md) | what Away does, for whom, for which kind of connection, and when: in the first release, later, or not at all; what is expected of it, and what is left out |
| [screens](./screens/README.md) | what Away looks like: every screen of the first release as a drawing, with its parts, its rules and its words |

Not written yet: the architecture, which names the components that do all this
and the decisions behind them. It follows the review of these three parts.

Away's pages take their look and their rules from the
[nalet design system](https://nalet.github.io/design-system/), which holds what
every product shares. The specification doesn't repeat them.

## The state of the model

<!-- generated: state -->
|  | Rows | Of them |
|---|---|---|
| Goals | 8 | 3 what people want, 4 what that takes, 1 what being open adds |
| Qualities | 34 | 0 with a target, 34 with a proposal |
| Expectations | 98 | 50 first release, 12 first release in part, 28 later, 8 not planned |
| Capabilities | 195 | in 19 areas: 111 first release, 71 later, 13 not planned |
| Figures | 80 | in 15 chapters; they show 59 of the 65 capabilities the first release holds for a person or an administrator |
<!-- end: state -->

A capability counts once, by the soonest it is planned for any kind of
connection. How the words are meant is in
[how to read the capabilities](./capabilities/README.md#how-to-read-it) and
[how to read the expectations](./capabilities/expectations.md#how-to-read-it).

## Before the first release is built

The model and the drawings say what the first release is. Building it waits for
these:

| Open | What it takes |
|---|---|
| The review | The owner's word on the goals, on the cut between the first release and later, and on the screens. |
| Targets | Every quality has a proposal and no target. A target is what a release is tested against. |
| The architecture | The components, how they reach each other and what each keeps, with the decisions behind them: whether Away speaks RDP, VNC and SSH with code of its own or with someone else's; how a machine's picture reaches the browser; where Away keeps what it keeps; where a kept secret lives; who issues the sign-ins on Windows machines. |
| A proof of the sign-in | Being signed in on a Windows machine as oneself rests on a short-lived certificate that a domain accepts for an ordinary account. The first release rests on it, so it is proven on a real domain before anything is built around it. |
| Machines to test against | The tests run against a real server of each protocol. A build can start some of them by itself: a shell, a desktop over VNC, a desktop over RDP on Linux. A Windows machine in a domain it cannot, so that part needs a place of its own. |
| The frame's components | The top bar, the rail and the tab bar are drafts in the design system and no components yet. |
| Work packages | The order in which it is built, in pieces small enough to review one at a time. |

## How it is kept

Written by hand, counted by a program. The model is six tables, kept as CSV
files beside these pages; `scripts/model.py` checks them and writes every table
and number on the pages from them, and the site draws its maps from the same
data. The screens are drawn in HTML files beside their chapters, and
`scripts/render-mockups.mjs` makes their pictures. [Changing the model](../docs/model.md)
says how.
