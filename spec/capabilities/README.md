# Capabilities

What Away does, for whom, for which kind of connection, and when. It is the
layer of the specification under the [goals](../goals/README.md): it says that
a capability exists and when it is planned. How a capability looks and behaves
is for the chapters that follow the review.

On the site the model is also a [map](https://nalet.github.io/away/spec/capabilities/map/):
a lane for each part, a cluster for each area, a tile for each capability,
coloured by where it stands for a session over RDP, VNC or SSH, or for Away
itself. A tile opens the capability's fact sheet: where it stands, its note, and
the expectations that name it.

> **A plan, and no product yet.** Nothing is built, so today the model says
> what is planned: for the first release, for later, or not at all. As Away is
> built the same cells turn to "partial" and "built".
>
> **Held against what is expected of it.** What people expect of remote access
> they run themselves is kept as a list of [expectations](./expectations.md).
> Each names the capabilities that meet it, so the model says of each whether
> the first release meets it.

## How to read it

A **capability** is something someone can do with Away, or something Away does
for them, at the grain where a release either has it or hasn't: "Open one
program of a Windows machine, without its desktop", "Be signed in on the machine
as oneself, with nothing asked", "End a session nobody has used for a set time".
A screen is not a capability; it is where one is offered.

**Who** it is for:

| Who | Is |
|---|---|
| person | someone who works on a machine |
| admin | someone who says which machines there are and who reaches them, and who reads the records |
| operator | whoever installs and runs Away |
| system | Away itself, when nobody asks |
| anyone | anyone at all: someone who checks Away, or wants to change it |

**Where it stands** is said four times over, once for each kind of connection
and once for Away itself:

| Column | Is |
|---|---|
| RDP | a session with a Windows machine: its desktop, or one of its programs |
| VNC | a session with a desktop that offers VNC: a Mac, a Linux desktop, anything with a screen |
| SSH | a shell |
| Away | Away itself, around the sessions: what does not depend on the protocol |

A capability about a session stands somewhere for at least one protocol and has
a dash for Away itself. A capability about Away around the sessions is the
other way round.

| Value | Means |
|---|---|
| built | there |
| partial | there, with the gap its note names |
| v1 | planned for the first release |
| later | wanted, after the first release |
| no | not planned: left out on purpose, and its note says why |
| — | does not apply: a shell has no picture to sharpen |

**The first release** is the one that can take over a day's work. It opens a
Windows desktop and its programs, a Mac or a Linux desktop, and a shell; it
signs a person in on the machine without another password where the machine
allows it; it lets in the people a rule names, and keeps a record of every
connection. What makes that more comfortable, or easier to run, comes later.

Taken as a whole, a capability is **built** when every cell that applies is,
**partly built** when some are, and otherwise as soon as its soonest cell: in
the **first release**, **later**, or **not planned**.

## Areas and keys

Capabilities are grouped in areas, the areas in parts: in a session, around the
sessions, administration, the platform. Only a capability is counted and
checked; an area is a heading. A part is where a capability is offered; what it
is there for is its area's **goal**, one of the goals of
[what Away is for](../goals/README.md).

A capability's key, such as `open-program`, is short and stable: it says what
the capability does, not where it sits, so it doesn't change when an area or a
wording does.

## The files

| File | Written by | Holds |
|---|---|---|
| [areas.csv](./areas.csv) | hand | the areas: part, goal, what each is about |
| [capabilities.csv](./capabilities.csv) | hand | one row a capability |
| [expectations.csv](./expectations.csv) | hand | one row a thing people expect of remote access they run themselves, with the capabilities that meet it |
| the tables of these pages | `scripts/model.py` | never edited by hand |

To change the model, edit the CSV files and run `python3 scripts/model.py`,
which checks them and writes the tables. [Changing the model](../../docs/model.md)
has the steps.

## What is checked

`python3 scripts/model.py --check` runs on every change and fails when:

- a file has other columns than the model's, a value is none of those the
  model allows, or a key is there twice;
- an area or an expectation is for no goal of [goals.csv](../goals/goals.csv),
  a capability is in no area, or an expectation names a capability that
  doesn't exist;
- a capability stands somewhere both for a protocol and for Away itself, or for
  neither;
- a goal has nothing below it, or an area has no capability;
- a table on these pages is not what the data says.

## The state of the model

<!-- generated: overview -->
| Part | Areas | Capabilities | built | partly built | first release | later | not planned |
|---|---|---|---|---|---|---|---|
| [in a session](./session.md) | 7 | 67 | 4 | 16 | 19 | 23 | 5 |
| [around the sessions](./app.md) | 5 | 49 | 16 | 3 | 9 | 17 | 4 |
| [administration](./admin.md) | 4 | 56 | 12 | 2 | 13 | 26 | 3 |
| [the platform](./platform.md) | 3 | 33 | 15 | 1 | 8 | 7 | 2 |
| all | 19 | 205 | 47 | 22 | 49 | 73 | 14 |

| For | built | partly built | first release | later | not planned | does not apply |
|---|---|---|---|---|---|---|
| a session over RDP | 6 | 7 | 24 | 25 | 5 | 138 |
| a session over VNC | 9 | 7 | 11 | 11 | 15 | 152 |
| a session over SSH | 0 | 0 | 27 | 12 | 2 | 164 |
| Away itself | 44 | 4 | 26 | 45 | 9 | 77 |

An expectation names 166 of the 205 capabilities; no expectation names the other 39.
<!-- end: overview -->

## What is expected of it

What people expect of remote access they run themselves, goal by goal, and what
the plan makes of each: [expectations](./expectations.md) has every one with
the capabilities that meet it, and says how the states are worked out.

<!-- generated: expected -->
| Expected, for the goal | Expectations | there | partly | first release | first release in part | later | not planned |
|---|---|---|---|---|---|---|---|
| [Work on a machine that is somewhere else](./expectations.md#work-on-a-machine-that-is-somewhere-else) | 24 | 3 | 10 | 4 | 0 | 5 | 2 |
| [Get in without another password](./expectations.md#get-in-without-another-password) | 16 | 3 | 4 | 5 | 1 | 2 | 1 |
| [Look at a machine together](./expectations.md#look-at-a-machine-together) | 4 | 0 | 0 | 0 | 0 | 3 | 1 |
| [Know the machines](./expectations.md#know-the-machines) | 13 | 3 | 2 | 1 | 0 | 6 | 1 |
| [Decide who reaches what](./expectations.md#decide-who-reaches-what) | 13 | 3 | 0 | 4 | 2 | 4 | 0 |
| [Know what happened](./expectations.md#know-what-happened) | 9 | 1 | 1 | 1 | 1 | 5 | 0 |
| [Run it oneself](./expectations.md#run-it-oneself) | 16 | 4 | 5 | 1 | 1 | 2 | 3 |
| [Trust it](./expectations.md#trust-it) | 4 | 3 | 1 | 0 | 0 | 0 | 0 |
| all | 99 | 20 | 23 | 16 | 5 | 27 | 8 |
<!-- end: expected -->

## Pages

| Page | Covers |
|---|---|
| [the map](https://nalet.github.io/away/spec/capabilities/map/) | every capability as a tile, by part and area, and its fact sheet (on the site only) |
| [in a session](./session.md) | opening a machine, the picture, keyboard and pointer, the clipboard and files, sound, the shell, a phone or a tablet |
| [around the sessions](./app.md) | signing in, the frame, finding a machine, working together, one's own settings |
| [administration](./admin.md) | the register of machines, who reaches what, sessions as they run, what happened |
| [the platform](./platform.md) | installing and configuring, running it, checking it |
| [expectations](./expectations.md) | what people expect of remote access, and what the plan makes of it |
| [what is left out](./left-out.md) | everything marked not planned, and why |
