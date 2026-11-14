# ADR-0005: A shell's screen is worked out in the browser

**Status:** Proposed · 2026-11-14. Nothing of it is built.

## Context

A shell sends no picture. It sends text mixed with instructions: move here, colour this, clear that line, switch to a second screen for an editor. Code has to turn that stream into what the shell shows, and to turn keys and the mouse back into what the shell expects. That code is usually called a terminal emulator; the screens call what it produces the shell.

It can run in the gateway, which would then send the browser a finished screen, or in the browser, which would then be sent the shell's own bytes. The model bears on the choice:

- What scrolled by can be scrolled back through and searched (`scroll-back`), and text is selected and copied (`select-shell-text`).
- A shell stays usable on a line of 50 kbit/s (*It stays usable on a slow line*).
- A screen reader can read a shell (*People who see or hear less can use it*).
- When the connection drops, Away holds the shell and *what it wrote in between is there when the connection is back* ([chapter 8](../../screens/shell.md)).
- Holding a shell while the browser is closed (`keep-while-away`) and recording one as text come later.
- No library that speaks a remote protocol ([ADR-0001](./0001-own-protocol-code.md)), and little of anyone else's in the page ([ADR-0011](./0011-the-web-app.md)).

## Decision

1. **The shell's screen is a library of Away's own**, with no input or output of its own, like the protocol libraries. Bytes go in; a grid of characters with their colours, the lines that scrolled by, the cursor and the shell's title come out. Keys and the mouse go in; the bytes a shell expects come out.
2. **In the first release it runs in the browser**, compiled to WebAssembly. The gateway passes a shell's bytes on untouched, in both directions.
3. **The session view draws the grid** and keeps the lines on the screen as text beside it, for a screen reader and for selecting.
4. **The gateway keeps the last of what a shell wrote**, up to a size the operator sets. A page that comes back after a drop, or is loaded again, is sent what it missed and is as far as the shell is.
5. **It understands what the programs people use send**: the instructions an xterm understands, as far as editors, pagers, monitors and programs that split the screen need them; characters of any script, wide and combined ones included; 256 and 16 million colours; the second screen; the mouse; pasting marked as pasting; a change of size.
6. **Later the same library runs in the gateway too**, to hold a shell while no browser is there and to record one as text. The connection carries the same bytes then as now.

## Consequences

- Scrolling back and searching happen in the page: at once, and at no cost to the line.
- A shell costs the gateway almost nothing: it passes bytes on.
- One piece of code for the browser now and for the gateway later, so a shell looks the same wherever its screen is worked out.
- What a hostile machine writes into a shell is read by this code inside the browser's own walls, in a language that allows no reading past a buffer.
- The page holds WebAssembly, so the rule about what the page may run has to allow it, and the build has to make it.
- Getting every program to look right is Away's work. A mistake shows as a garbled editor, so every change is run against a set of programs and compared with a terminal of reference ([ADR-0013](./0013-built-tested-signed-by-the-pipelines.md)).
- What scrolled by lives in the page. A page that is loaded again has as much of it as the gateway kept.

### What this rules out

- **No terminal library of someone else's in the page.**
- **No finished screen sent by the gateway**, and none kept there, in the first release.
- **No shell that lasts through a closed browser or an update of Away** until the library runs in the gateway too.
