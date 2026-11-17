# ADR-0016: A picture that moves is measured, then shown whole, then taken as the machine composes it

**Status:** Proposed · 2026-11-17. Nothing of it is built. The owner asked for it to be kept for later: the order in which a moving picture is made smooth after the first release, up to a video that plays on the machine. The format that gives up detail is the first release's own ([ADR-0004](./0004-the-session-and-its-picture.md)) and is not built either.

## Context

On the first installation a desktop at rest is sharp and answers at once. What moves on a Windows machine does not keep up: an animation of a window or a menu stutters, and a video is a row of stills. A browser that is handed the machine's own video stream shows the same desktop smoothly.

How a moving picture travels today, as [ADR-0004](./0004-the-session-and-its-picture.md) is built:

- **The machine sends pieces.** Away asks a Windows machine for pictures of what changed, and for nothing that has to be drawn from instructions. The machine sends the changed pieces as they come, and never says where one state of its screen ends and the next begins.
- **The gateway packs every piece exactly.** Each changed square of 64 pixels is packed in the exact format. For a large area that moves, that is much work and many bytes.
- **States in between are left out.** At most two batches are on their way before the browser confirms one. While much moves, few pictures a second arrive.
- **The page draws piece by piece.** It unpacks one rectangle after the other and draws each when it is ready, so that one state of the screen appears in parts.
- **Every effect travels.** Away asks the machine to keep its effects (`desktop-as-at-machine`), so each animation of a menu and a window is sent.

None of this is measured. The quality *A moving picture stays smooth* has its target (30 pictures a second on a local network, 15 on a line of 10 Mbit/s, while a window is dragged at 1920 by 1080), and ADR-0004 says the design is measured against it before it is taken as settled.

A Windows machine offers more than pictures of what changed. In the part of RDP that its specification calls the graphics pipeline, the machine composes whole pictures itself, says where each begins and ends, is told which one was shown and keeps its pace by that. It packs what moves as a video stream and the rest in formats that lose nothing, or that get sharper step by step. A machine with a graphics card renders and packs with it. Every current browser unpacks that kind of video with the device's own decoder.

Two questions follow. In which order is the picture made faster? And what of ADR-0004 stays?

## Decision

1. **The picture is measured before it is changed.** A session gives its pictures a second and the time each stage takes: from the machine to the gateway, unpacking, packing, the line, unpacking in the page, drawing. The test program gives the same without a browser. Each step below is taken in the order the measures give, and stays only where the measure shows its gain.
2. **A change is shown whole.** The gateway sends what belongs to one state of the screen together and says where it ends. The page unpacks its rectangles side by side and draws them in one step (`show-whole-pictures`).
3. **What moves gives up detail**, as ADR-0004 decided, and is exact again once it rests (`adapt-to-line`). It is the first release's, and comes before everything below.
4. **A person can have a machine leave its effects out**: the animations of menus and windows, a window's content while it is dragged, the background (`leave-out-effects`). What is not sent cannot stutter.
5. **Then Away takes a Windows machine's picture as the machine composes it** (`machine-made-pictures`): whole pictures, each confirmed once it is shown, so that the machine keeps the browser's pace. The gateway unpacks the exact formats into its own picture, as it does today, and the browser gets rectangles as before.
6. **What the machine packs as video is passed through** to the browser's own video decoder and drawn where it plays (`play-video-smoothly`). The gateway makes no video and unpacks none. Away asks for video only for the areas the machine itself finds to be moving, and never for the whole screen with colours left out: text stays exact at rest.
7. **The gateway's picture is not current where a video plays.** A browser that comes back, a second person and a recording get that area once it rests, or when the gateway asks the machine for it anew. Whether the gateway unpacks video after all is decided with the proof, and takes a record of its own.

### Validation

A proof comes before step 5, as for RDP itself ([ADR-0001](./0001-own-protocol-code.md)): the test program asks a Windows 11 and a Windows Server 2025 for the graphics pipeline, saves what arrives, and counts pictures a second while a window is dragged and while a video plays. It shows which formats the machine chooses when Away offers video for moving areas alone, and how much of the pipeline has to be Away's own code before a desktop is whole.

The steps hold if the quality's target is met after step 3 for a dragged window, and if a video of 1920 by 1080 plays at 24 pictures a second or more on a local network after step 6.

## Consequences

- Steps 2 to 4 keep ADR-0004 as it is, and a desktop over VNC gains from them too.
- Steps 5 and 6 are for Windows machines alone. A desktop over VNC has neither whole pictures of the machine's making nor a video stream.
- The graphics pipeline has several formats, and each is Away's own code ([ADR-0001](./0001-own-protocol-code.md)): the exact ones, the one that gets sharper step by step, and what the machine keeps for the browser to draw again. It is the larger part of the work, and all of it is code that a hostile machine speaks to, read and fuzzed with the rest.
- A video stream is the first thing of a machine's own that reaches the browser. It goes to the browser's video decoder and nowhere else, which is built to take video from anywhere.
- A browser without a decoder for the machine's video keeps the rectangles.
- A machine without a graphics card renders and packs with its processor. How many sessions that bears, and the machine's own settings for its pace and its video, are the operator's, and the guide says which they are.

### What this rules out

- **No change to how the picture travels before it is measured.**
- **No video made by the gateway**, as in ADR-0004.
- **No whole screen as video with colours left out.** A screen at rest stays exact.
- **No decoder of a machine's formats in the page**, beyond the browser's own decoder for video.
