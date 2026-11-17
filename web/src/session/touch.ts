// Fingers on the picture, read as a trackpad: the screen is the pad, and the pointer is
// where it was left, not under the finger.
//
// One finger moves the pointer, faster the faster it moves. A tap presses the main
// button where the pointer is, and a finger that comes down again right after a tap
// holds that button, to drag. A finger that rests is the other button, and so is a tap
// with two fingers; a tap with three is the middle one. Two fingers that move together
// turn the wheel, and two that spread or close zoom the picture.

export type Point = { x: number; y: number };

// What the fingers did, in the window's pixels.
export type Does = {
  // The pointer moves by this much.
  moved: (by: Point) => void;
  // A button of the pointer goes down or comes up, where the pointer is: the main one
  // is 0, the middle one 1, the other one 2.
  button: (button: number, down: boolean) => void;
  // Two fingers moved together by this much.
  turned: (by: Point) => void;
  // Two fingers spread or closed by a factor, and what was between them at one place of
  // the window is between them at another now.
  zoomed: (by: number, from: Point, to: Point) => void;
};

// How long a tap lasts at most, and how long after it a finger that comes down again
// holds the button.
const A_TAP_WITHIN = 250;
// How far a finger moves at most during a tap.
const A_TAP_MOVES = 10;
// How long a finger rests before it is the other button.
const RESTS_FOR = 500;
// How far two fingers spread or close before they zoom, and how far they move together
// before they turn the wheel.
const SPREADS_BY = 16;
const MOVES_BY = 8;
// How many times faster the pointer moves than the finger at most.
const FASTEST = 4;

// A finger's touches as a pointer's doings. Each of the four takes what the browser
// says of a finger; `ends` lets go of whatever is held.
export function fingers(does: Does): {
  down: (event: PointerEvent) => void;
  move: (event: PointerEvent) => void;
  up: (event: PointerEvent) => void;
  ends: () => void;
} {
  const on = new Map<number, Point & { at: number }>();
  // the touch that is going on: when it began, how many fingers it had at most, how far
  // the first one moved, and what it turned out to be
  let began = 0;
  let most = 0;
  let travelled = 0;
  let is: 'nothing yet' | 'moving' | 'turning' | 'zooming' | 'rested' = 'nothing yet';
  // two fingers as they were when the second came down, and as they were last
  let between: { middle: Point; apart: number } | null = null;
  let last: { middle: Point; apart: number } | null = null;
  // the main button, down since a tap: let go a moment later, or held by a finger that
  // came down again in time
  let lets = 0;
  let pressed = false;
  let rests = 0;

  const two = (): { middle: Point; apart: number } | null => {
    const [first, second] = [...on.values()];
    if (!first || !second) return null;
    return { middle: { x: (first.x + second.x) / 2, y: (first.y + second.y) / 2 }, apart: Math.hypot(first.x - second.x, first.y - second.y) };
  };
  const letGo = () => {
    window.clearTimeout(lets);
    lets = 0;
    if (pressed) does.button(0, false);
    pressed = false;
  };

  return {
    down(event) {
      on.set(event.pointerId, { x: event.clientX, y: event.clientY, at: event.timeStamp });
      window.clearTimeout(rests);
      if (on.size === 1) {
        began = event.timeStamp;
        most = 1;
        travelled = 0;
        is = 'nothing yet';
        between = null;
        // a finger that comes down again right after a tap holds the button the tap pressed
        if (lets) {
          window.clearTimeout(lets);
          lets = 0;
        } else {
          rests = window.setTimeout(() => {
            if (on.size !== 1 || is !== 'nothing yet') return;
            is = 'rested';
            does.button(2, true);
            does.button(2, false);
          }, RESTS_FOR);
        }
        return;
      }
      most = Math.max(most, on.size);
      between = two();
      last = between;
    },
    move(event) {
      const finger = on.get(event.pointerId);
      if (!finger) return;
      const by = { x: event.clientX - finger.x, y: event.clientY - finger.y };
      const took = Math.max(1, event.timeStamp - finger.at);
      on.set(event.pointerId, { x: event.clientX, y: event.clientY, at: event.timeStamp });
      if (on.size === 1 && most === 1) {
        travelled += Math.abs(by.x) + Math.abs(by.y);
        if (is === 'nothing yet' && travelled >= A_TAP_MOVES) {
          is = 'moving';
          window.clearTimeout(rests);
        }
        if (is !== 'moving') return;
        // a finger that moves fast carries the pointer further than one that moves slowly
        const faster = Math.min(FASTEST, 1 + Math.hypot(by.x, by.y) / took);
        does.moved({ x: by.x * faster, y: by.y * faster });
        return;
      }
      const now = two();
      if (on.size !== 2 || !now || !between || !last) return;
      if (is !== 'turning' && is !== 'zooming') {
        if (Math.abs(now.apart - between.apart) >= SPREADS_BY) is = 'zooming';
        else if (Math.hypot(now.middle.x - between.middle.x, now.middle.y - between.middle.y) >= MOVES_BY) is = 'turning';
        else return;
        last = now;
        return;
      }
      if (is === 'zooming') does.zoomed(now.apart / Math.max(1, last.apart), last.middle, now.middle);
      else does.turned({ x: now.middle.x - last.middle.x, y: now.middle.y - last.middle.y });
      last = now;
    },
    up(event) {
      if (!on.delete(event.pointerId)) return;
      window.clearTimeout(rests);
      if (on.size > 0) return;
      // the last finger left: a button that a finger held comes up
      const held = pressed;
      if (held) letGo();
      const tapped = is === 'nothing yet' && travelled < A_TAP_MOVES && event.timeStamp - began <= A_TAP_WITHIN && event.type !== 'pointercancel';
      if (!tapped) return;
      if (most === 1) {
        // the button goes down now and comes up a moment later, unless a finger comes
        // down again before: then it stays down under that finger
        does.button(0, true);
        pressed = true;
        lets = window.setTimeout(letGo, A_TAP_WITHIN);
      } else {
        const button = most === 2 ? 2 : 1;
        does.button(button, true);
        does.button(button, false);
      }
    },
    ends() {
      window.clearTimeout(rests);
      on.clear();
      letGo();
    },
  };
}
