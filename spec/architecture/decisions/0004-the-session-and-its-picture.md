# ADR-0004: One connection a session, and the gateway keeps the picture

**Status:** Accepted · 2026-11-14. Nothing of it is built. The targets for the picture are measured before the design is taken as settled (see [Validation](#validation)).

## Context

A machine's picture has to reach a browser, and keys and the pointer have to go back, under these conditions from the model:

- Away answers on one address, *behind the reverse proxy that is there* (`set-address`). A person's device needs a browser and nothing else, from any network.
- The picture is exact once it rests, gives up detail while it moves or the line is slow, and follows the window's size (`exact-at-rest`, `adapt-to-line`, `fit-to-window`). The pointer moves without delay (`pointer-without-delay`).
- A session comes back by itself after a drop, and moves from one browser tab to another ([chapter 9](../../screens/sessions.md)).
- It works in the current and the previous version of four browsers, on a computer, a tablet and a phone.
- Later: two people in one session, recordings, a machine's video played smoothly.

Two questions follow. What carries a session between the browser and Away? And where is a machine's picture decoded: in the gateway, or in the browser?

## Decision

1. **One connection a session: a WebSocket on Away's own address.** It carries messages of Away's own format in both directions: rectangles of the picture, the pointer's shape, the clipboard, the steps of a connection and how it ended one way; keys, the pointer, the window's size and the clipboard the other. Any reverse proxy that passes WebSockets passes it, with no further port to open.
2. **The gateway keeps each desktop's picture as it is on the machine.** It decodes whatever the machine sends into that picture, in whichever of the protocol's formats, and the browser never sees the machine's own bytes.
3. **The browser gets rectangles it can decode itself.** A rectangle is new pixels, or a copy from elsewhere in the picture where the machine reports a move. New pixels travel in two formats that every browser decodes without help: one that gives up detail, for what is moving, and one that is exact, for what has come to rest. The browser draws them on a canvas. The page holds no decoder of its own.
4. **The gateway sends only as fast as the browser confirms.** When the line is slow it leaves out states in between and lowers the detail of what moves. When the picture rests, it sends it again exactly.
5. **The pointer's shape travels on its own**, and the browser shows it as its own pointer.
6. **The gateway never makes video.** A machine's own video stream is passed through to the browser's video decoder, with `play-video-smoothly`, later.
7. **A session outlives its connection for a while.** When the connection drops, the gateway holds the session for a time the operator sets. A browser that comes back, in the same tab or another, is sent the whole picture once and then what changes. For a shell the gateway keeps the bytes that were written meanwhile ([ADR-0005](./0005-the-shells-screen-in-the-browser.md)).

### Validation

The first desktop, over VNC ([package 9](../../plan/README.md)), is measured against the proposed targets: 30 pictures a second on a local network and 15 on a line of 10 Mbit/s while a window is dragged at 1920 by 1080, office work on a line of 2 Mbit/s, a screen at rest exact within a second, and a delay of under 50 ms added by Away.

The design holds if they are met with two picture formats and copies. If moving pictures cost more than the line has, the first thing to change is the format for what moves; the gateway keeping the picture stays.

## Consequences

- Any browser will do: a canvas, pictures and one connection.
- A page that is loaded again, or a session that moves to another tab, has its picture at once.
- The small pictures of the open sessions, a second person in a session and a recording all have one source later.
- What a hostile machine sends is read in the gateway and ends there. The browser reads Away's format only.
- The gateway pays for it: memory for each desktop's picture (8 MB at 1920 by 1080, four times that at twice the size) and work for every rectangle it packs.
- A moving picture costs more of the line than a video format would. A video on the machine stutters on a slow line until its own stream is passed through.
- One connection is one queue: a lost packet holds back everything behind it until it is sent again.

### What this rules out

- **No path beside the address Away answers on**: no further port, no connection straight from the browser to anything but Away.
- **No decoding of a machine's own formats in the browser.**
- **No video encoder in the gateway.**
- **No session without the gateway's copy of its picture.**
