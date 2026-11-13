# The specification

The specification says what Away is for and what it has to do, before any of it
is built. It is written for the people who decide what Away becomes and for the
people who build it, and every release is checked against it.

> **A first cut, under review.** Nothing of Away is built yet. Every row of this
> specification is a proposal until the owner has reviewed it: a goal can be
> struck, a capability moved between the first release and later, a target set.
> What is built follows from the reviewed model, and not before.

It has two parts:

| Part | What it specifies |
|---|---|
| [what it is for](./goals/README.md) | the goals: what people want, what that takes, what being open adds; the qualities each has to be met with |
| [capabilities](./capabilities/README.md) | what Away does, for whom, for which kind of connection, and when: in the first release, later, or not at all; what is expected of it, and what is left out |

Not written yet: the architecture, which names the components that do all this
and the decisions behind them; and a chapter for each screen. Both follow the
review of these two parts.

Away's pages take their look and their rules from the
[nalet design system](https://nalet.github.io/design-system/), which holds what
every product shares. The specification doesn't repeat them.

## The state of the model

<!-- generated: state -->
|  | Rows | Of them |
|---|---|---|
| Goals | 8 | 3 what people want, 4 what that takes, 1 what being open adds |
| Qualities | 34 | 0 with a target, 34 with a proposal |
| Expectations | 92 | 44 first release, 12 first release in part, 28 later, 8 not planned |
| Capabilities | 184 | in 19 areas: 100 first release, 71 later, 13 not planned |
<!-- end: state -->

A capability counts once, by the soonest it is planned for any kind of
connection. How the words are meant is in
[how to read the capabilities](./capabilities/README.md#how-to-read-it) and
[how to read the expectations](./capabilities/expectations.md#how-to-read-it).

## How it is kept

Written by hand, counted by a program. The model is five tables, kept as CSV
files beside these pages; `scripts/model.py` checks them and writes every table
and number on the pages from them, and the site draws its maps from the same
data. [Changing the model](../docs/model.md) says how.
