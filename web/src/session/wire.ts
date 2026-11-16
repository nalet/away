// The session's messages between the browser and the gateway, as the pages read and
// write them. The gateway's side is crates/away-wire, and both are tried against the
// examples in crates/away-wire/fixtures/messages.json.

export type Step = 'reaching' | 'checking' | 'signing-in' | 'first-picture';

// What the gateway says as text.
export type Said =
  | { step: Step }
  | { asks: { again: boolean; instead?: boolean } }
  | { desktop: { width: number; height: number } }
  | { drawn: number }
  | { failed: { step: Step; why: string; details: string } }
  | { ended: string }
  | { windows: WindowOf[]; in_front: number | null }
  | { clipboard: { to: boolean; from: boolean } }
  | { copied: string }
  | { started: { program: string; ok: boolean } };

// A window of a program: what names it, its title, how it shows, whether it is a window
// of its own that a list names, and the parts of it that can be seen, each by its left,
// top, right and bottom edge on the desktop.
export interface WindowOf {
  id: number;
  title: string;
  shown: 'as-it-is' | 'maximized' | 'minimized' | 'hidden';
  listed: boolean;
  seen: [number, number, number, number][];
}

// A rectangle of the picture: where it goes, and its pixels as a PNG picture.
export interface Rectangle {
  left: number;
  top: number;
  width: number;
  height: number;
  png: Uint8Array<ArrayBuffer>;
}

export function readSaid(text: string): Said | null {
  try {
    const said: unknown = JSON.parse(text);
    return typeof said === 'object' && said !== null ? (said as Said) : null;
  } catch {
    return null;
  }
}

export function readRectangle(bytes: ArrayBuffer): Rectangle | null {
  const view = new DataView(bytes);
  // a rectangle, with its pixels as a PNG picture
  if (bytes.byteLength < 10 || view.getUint8(0) !== 1 || view.getUint8(9) !== 1) return null;
  return { left: view.getUint16(1), top: view.getUint16(3), width: view.getUint16(5), height: view.getUint16(7), png: new Uint8Array(bytes, 10) };
}

// The icon of a window of a program: the window, and the icon as a PNG picture.
export interface IconOf {
  window: number;
  png: Uint8Array<ArrayBuffer>;
}

export function readIcon(bytes: ArrayBuffer): IconOf | null {
  const view = new DataView(bytes);
  if (bytes.byteLength < 6 || view.getUint8(0) !== 2) return null;
  return { window: view.getUint32(1), png: new Uint8Array(bytes, 5) };
}

// The person asks for another program of the machine, in the session that runs: by the
// name Away gives it.
export function start(program: string): string {
  return JSON.stringify({ start: program });
}

// The window's size as the page measures it, and how many dots the device shows for a
// hundred of the page's: the machine's desktop has the device's dots.
export function begins(ticket: string, width: number, height: number, layout: number, scale: number): string {
  return JSON.stringify({ ticket, width, height, layout, scale });
}

// The window has another size now, or is on another screen: as the page measures it,
// and how many dots the device shows for a hundred of the page's.
export function sized(width: number, height: number, scale: number): string {
  return JSON.stringify({ sized: { width, height, scale } });
}

// The person copied this text on their own device.
export function copied(text: string): string {
  return JSON.stringify({ copied: text });
}

export function credentials(name: string, password: string): string {
  return JSON.stringify({ name, password });
}

export function seen(number: number): string {
  return JSON.stringify({ seen: number });
}

export function leaves(): string {
  return JSON.stringify({ leaves: true });
}

// What a person does with a window of a program as a whole.
export function windowDoes(id: number, does: 'front' | 'minimize' | 'maximize' | 'restore' | 'close'): string {
  return JSON.stringify({ window: id, does });
}

// One thing a person did with the keys or the pointer. A button is numbered as a
// browser numbers it: 0 the main one, 1 the middle, 2 the second, 3 back, 4 forward.
export type Did =
  | { moved: { x: number; y: number } }
  | { button: { button: number; down: boolean; x: number; y: number } }
  | { wheel: { by: number; sideways: boolean; x: number; y: number } }
  | { key: { code: number; extended: boolean; down: boolean } }
  | { character: { unit: number; down: boolean } }
  | { locks: { scroll: boolean; num: boolean; caps: boolean } };

export function did(events: Did[]): Uint8Array<ArrayBuffer> {
  const bytes: number[] = [];
  const pair = (number: number) => bytes.push((number >> 8) & 0xff, number & 0xff);
  for (const event of events) {
    if ('moved' in event) {
      bytes.push(1);
      pair(event.moved.x);
      pair(event.moved.y);
    } else if ('button' in event) {
      bytes.push(2, event.button.button, event.button.down ? 1 : 0);
      pair(event.button.x);
      pair(event.button.y);
    } else if ('wheel' in event) {
      bytes.push(3);
      pair(event.wheel.by & 0xffff);
      bytes.push(event.wheel.sideways ? 1 : 0);
      pair(event.wheel.x);
      pair(event.wheel.y);
    } else if ('key' in event) {
      bytes.push(4, event.key.code, event.key.extended ? 1 : 0, event.key.down ? 1 : 0);
    } else if ('character' in event) {
      bytes.push(5);
      pair(event.character.unit);
      bytes.push(event.character.down ? 1 : 0);
    } else {
      bytes.push(6, (event.locks.scroll ? 1 : 0) | (event.locks.num ? 2 : 0) | (event.locks.caps ? 4 : 0));
    }
  }
  return new Uint8Array(bytes);
}
