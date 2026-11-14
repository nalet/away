# 1 Introduction

These chapters draw Away's screens: what each one shows, how it behaves, and
what it says in each state, on a computer and on a phone. They are the third
part of the specification, after [what Away is for](../goals/README.md) and
[what it does](../capabilities/README.md).

> **Drawings, and no screenshots.** The screens are built from these drawings,
> which the owner agreed to on 14 November 2026. Until a screen exists, its
> figure says what it will be. Where building shows a drawing wrong, the
> drawing is changed first, and so is the capability behind it.

## 1.1 Scope

The chapters cover what a person sees in a browser: Away's own pages, a session
on a machine, and the pages of an administrator. They draw the first release.
A capability that comes later is not drawn, and a chapter says so where a
reader would look for it.

They don't cover what has no screen: how Away is installed and run, the files
that hold the machines and the rules, and what Away puts out for a log system.
Those are in [the platform's capabilities](../capabilities/platform.md).

## 1.2 How to read the figures

Every drawing is a numbered figure that the text refers to: figure 6.2 is the
second figure of chapter 6.

| Size | Drawn at | Used by |
|---|---|---|
| Computer | 1280 × 800 | a browser window on a computer or a tablet |
| Phone | 360 × 780 | a phone held upright |
| Phone, sideways | 780 × 360 | a phone held sideways |

Pink marks are notes on a drawing, never part of a screen. A pink number points
at a part of the screen, and the numbered list under the figure's caption says
what that part is and does. A dashed pink outline marks an area.

What a machine shows inside a session is a stand-in: a desktop with a few
windows, a shell with a few commands. It is there to show where the machine's
picture is, and is neither Away's nor any real system's look. The people, the
machines and the addresses in the drawings are made up.

## 1.3 How a chapter is built

A chapter specifies one screen, or a few that belong together:

1. **What it is for**, how people get to it, and which capabilities it draws.
2. **The screen**, a figure for a computer and, where it differs, for a phone,
   with the numbered parts and the rules behind what it shows and does.
3. **States**: loading, empty, couldn't load, with a figure for the ones that
   matter.
4. **Words**: every text the screen shows, exactly.

## 1.4 The chapters

| No. | Chapter | What it covers |
|---|---|---|
| 1 | Introduction | this chapter |
| 2 | [The frame](./frame.md) | the top bar and the rail on a computer, the phone's tabs, the avatar's menu |
| 3 | [Getting in](./start.md) | signing in, signing out, a sign-in that ran out, nothing to reach |
| 4 | [Machines](./machines.md) | home: the machines one may reach, the search, a machine's page |
| 5 | [Opening a machine](./opening.md) | the steps of a connection, typing a name and a password, a connection that fails |
| 6 | [A desktop](./desktop.md) | a session: its bar, keys, the clipboard, the picture's size, its end |
| 7 | [Programs](./programs.md) | programs of a Windows machine without its desktop, and the list of their windows that moves between them |
| 8 | [A shell](./shell.md) | a shell: its text, its look, scrolling back and searching |
| 9 | [Sessions](./sessions.md) | the open sessions, and moving between them |
| 10 | [On a phone](./phone.md) | a desktop and a shell by touch, the keys a phone lacks |
| 11 | [Settings](./settings.md) | how one's sessions open, the keyboard, the shell's look, the account, about Away |
| 12 | [Administration: machines](./admin-machines.md) | the machines and groups in force, a machine's page, testing a machine |
| 13 | [Administration: rules](./admin-rules.md) | who reaches what, the roles, what a person reaches |
| 14 | [Administration: sessions and records](./admin-records.md) | who is connected now, ending a session, the records |
| 15 | [Administration: status](./admin-status.md) | whether Away's parts are well, and the check of the setup |
| 16 | [When something goes wrong](./errors.md) | what a page says when it can't do its job |
| 17 | [Words](./words.md) | the words Away uses for its own things |

## 1.5 The figures, and what they draw

Each figure is listed in [screens.csv](./screens.csv) with the capabilities it
draws. The table below is written from it, as is the list of what the first
release holds for a person or an administrator and no figure shows.

<!-- generated: figures -->
| No. | Chapter | Figures | Capabilities they draw |
|---|---|---|---|
| 2 | [The frame](./frame.md) | 5 | 8 |
| 3 | [Getting in](./start.md) | 6 | 6 |
| 4 | [Machines](./machines.md) | 9 | 14 |
| 5 | [Opening a machine](./opening.md) | 7 | 8 |
| 6 | [A desktop](./desktop.md) | 10 | 14 |
| 7 | [Programs](./programs.md) | 6 | 7 |
| 8 | [A shell](./shell.md) | 8 | 10 |
| 9 | [Sessions](./sessions.md) | 4 | 6 |
| 10 | [On a phone](./phone.md) | 5 | 10 |
| 11 | [Settings](./settings.md) | 3 | 14 |
| 12 | [Administration: machines](./admin-machines.md) | 7 | 10 |
| 13 | [Administration: rules](./admin-rules.md) | 3 | 8 |
| 14 | [Administration: sessions and records](./admin-records.md) | 4 | 7 |
| 15 | [Administration: status](./admin-status.md) | 2 | 8 |
| 16 | [When something goes wrong](./errors.md) | 6 | 8 |
|  | all | 85 | 86 |

The first release holds 68 capabilities for a person or an administrator. A figure shows 62 of them.
<!-- end: figures -->

A capability with nothing to see is in no figure: typing in one's own keyboard
layout, a pointer that moves without delay. The chapters name those in their
rules.

<!-- generated: not-drawn -->
| Area | In a figure | In no figure |
|---|---|---|
| [Keyboard and pointer](../capabilities/session.md#keyboard-and-pointer) | 4 of 8 | Give every key to the machine while a session fills the screen `give-all-keys`; Use every mouse button and the wheel, in both directions `use-mouse-fully`; Type the clipboard's text into the machine key by key `type-clipboard`; Type in a language whose characters are composed of several keys `compose-characters` |
| [On a phone or a tablet](../capabilities/session.md#on-a-phone-or-a-tablet) | 3 of 4 | Work by touch: tap to click, hold for the other button, two fingers to scroll `work-by-touch` |
| [Signing in](../capabilities/app.md#signing-in) | 4 of 5 | Open a locked desktop again with nothing asked `unlock-without-password` |
<!-- end: not-drawn -->

## 1.6 The design system

Away's pages are built from the
[nalet design system](https://nalet.github.io/design-system/), the package
`@nalet/design-system` 0.4.0. The drawings use the package's own stylesheet and
draw each of its components as the package renders it, so a page built with the
package looks like its figure.

| A part of a screen | The design system's component |
|---|---|
| The one main action, the others, quiet ones such as cancel, and what ends something | button: emphasized, standard, quiet, danger; icon button |
| A choice of one from a list: a keyboard layout, a text size | select, drawn closed |
| Actions in a menu: a machine's menu, the avatar's menu, the menus of a session's bar | action menu |
| A name and a password, a search | field, input, text area |
| A setting that applies at once | switch |
| A group of settings or of facts | card |
| A question over the page | dialog |
| A problem with a page or with a part of it | banner, with its details |
| News that leaves by itself | toast |
| Nothing to show, couldn't load, no access, an ending | empty state |
| Waiting | loader |
| A state: applied, refused, well, failing | status |
| A list of machines, rules, sessions or records | data table, with its filter bar |
| The parts of the administration | tabs |
| A file's name, a key, an address | code |

The rest is Away's own, drawn on the design system's colours, faces and sizes:
the frame of [chapter 2](./frame.md), a machine's tile (4), the steps of a
connection (5), the session's bar and its panels (6), the list of windows of a
session with programs (7), the keys a phone lacks (10), and the stand-ins for
what a machine shows.

Where the design system has nothing yet, the drawings show how Away draws it
until it does: the top bar and its user menu, the rail of icons and the phone's
tab bar, a menu of one choice, and details on an empty state.

The general rules come from the design system too, and these chapters don't
repeat them:
[meaningful text](https://nalet.github.io/design-system/foundations/meaningful-text/),
[content and voice](https://nalet.github.io/design-system/foundations/content-voice/)
(lowercase in the monospace face for what one presses and for headings,
sentences in the body face for what one reads),
[empty states](https://nalet.github.io/design-system/patterns/empty-states/) and
[loading and progress](https://nalet.github.io/design-system/patterns/loading-and-progress/).

## 1.7 The drawings' sources

Every screen is drawn in one file per chapter, `img/ch06-desktop.html` for
chapter 6 and so on. They share one stylesheet,
[img/screens.css](./img/screens.css), which imports the design system's own,
[img/ds/nalet-0.4.0.css](./img/ds/nalet-0.4.0.css), unchanged. A drawing writes
a design system component as a short tag, such as
`<ds-button variant="primary">open again</ds-button>`, and
[img/ds.js](./img/ds.js) turns it into the markup the package's component
renders. [img/frame.js](./img/frame.js) draws the frame of chapter 2, the session's
bar of chapter 6 and the list of windows of chapter 7 around every screen and
places the pink marks, and
[img/remote.js](./img/remote.js) draws the stand-ins for what a machine shows.

```sh
(cd site && npm ci)                                  # once: the drawings take their fonts from the site's packages
npx -p playwright@1.63.0 node scripts/render-mockups.mjs          # draw every figure again
npx -p playwright@1.63.0 node scripts/render-mockups.mjs ch06     # one chapter's figures
```

Edit a drawing in its HTML file, never its picture. The kit of tags, the frame
and the notes come from the specification of
[zaentrum](https://zaentrum.github.io/spec/), which draws its screens the same
way.
