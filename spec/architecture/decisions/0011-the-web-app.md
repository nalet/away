# ADR-0011: The web app is built from the design system, and loads nothing from outside

**Status:** Accepted · 2026-11-14. Nothing of it is built.

## Context

- Away's pages take their look and their rules from the [nalet design system](https://nalet.github.io/design-system/) ([chapter 1](../../screens/README.md#16-the-design-system)). Its components are React components with a stylesheet of tokens.
- The frame of [chapter 2](../../screens/frame.md), the top bar, the rail and a phone's tab bar, is a draft in the design system and no component yet.
- One app serves a computer, a tablet and a phone (`use-on-phone`), with every page usable by the keyboard alone and by a screen reader (`use-without-mouse`), to the level the quality proposes.
- Away asks nobody outside for anything: no fonts, no scripts, no statistics, no checks for updates (`call-nobody-else`).
- The sign-in is in the server ([ADR-0006](./0006-the-sign-in-is-held-by-the-server.md)), so the page handles no token.
- A session's picture is drawn many times a second ([ADR-0004](./0004-the-session-and-its-picture.md)).

## Decision

1. **TypeScript and React, with the design system's components**, for every page around the sessions. A part of a screen that the design system has is its component, unchanged.
2. **What the design system lacks is built in Away from its tokens**, as the chapters draw it: the frame, a machine's tile, the steps of a connection, the session's bar, the list of windows, the keys a phone lacks. Each is offered to the design system once it has held up in use, and replaced by the design system's own when there is one.
3. **The session view stands apart from the pages.** It is plain TypeScript around a canvas, driven by the session's connection and not by the pages' drawing, so a picture is never held up by a page. The shell's screen runs inside it ([ADR-0005](./0005-the-shells-screen-in-the-browser.md)).
4. **One app for every size.** A phone gets the same pages laid out for it, as the chapters draw them. There is no second app.
5. **The server serves the app's files, and the page may load from Away's own address alone.** Fonts and icons are in the build. The rule the server sends with every page allows no script, style, font or picture from anywhere else, and a test on every change opens every page and fails if the browser asked anyone but Away.
6. **Every page and every machine has an address**, so a link or a bookmark opens it (`open-by-address`).
7. **Every text is in one place**, in English, ready for further languages (`choose-language`, later).
8. **Few libraries.** The pages need React, the design system and its icons. The page signs nobody in, so it needs nothing for that.

## Consequences

- A page looks like its figure, because both are made of the same components.
- The frame is Away's to build and to keep until the design system has it.
- Accessibility is tested where it is made: each component of Away's own with the keyboard and a screen reader, and every page by a check on each change.
- A person on a network with no way out, or behind a filter, loses nothing: there is nothing to block.
- React and the design system are the page's weight, and their new versions are work.

### What this rules out

- **No second set of components**, and no restyling of the design system's.
- **No file from another address**: no font service, no content network, no measuring script.
- **No installed app** (`use-installed-app`, not planned). Adding Away to a phone's home screen comes later.
- **No pages made by the server.** The server sends the app once and data after that.
