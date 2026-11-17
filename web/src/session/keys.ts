// The keys of a keyboard, as a machine is told of them: each by its place, which a
// browser names and a Windows machine numbers. What a key writes is the machine's to
// say, by the layout the session is given: the one of the person's own keyboard.

// The keys by their places: the number of each in the first set of scan codes.
const PLACES: Record<string, number> = {
  Escape: 0x01, Digit1: 0x02, Digit2: 0x03, Digit3: 0x04, Digit4: 0x05, Digit5: 0x06, Digit6: 0x07, Digit7: 0x08, Digit8: 0x09,
  Digit9: 0x0a, Digit0: 0x0b, Minus: 0x0c, Equal: 0x0d, Backspace: 0x0e, Tab: 0x0f, KeyQ: 0x10, KeyW: 0x11, KeyE: 0x12, KeyR: 0x13,
  KeyT: 0x14, KeyY: 0x15, KeyU: 0x16, KeyI: 0x17, KeyO: 0x18, KeyP: 0x19, BracketLeft: 0x1a, BracketRight: 0x1b, Enter: 0x1c,
  ControlLeft: 0x1d, KeyA: 0x1e, KeyS: 0x1f, KeyD: 0x20, KeyF: 0x21, KeyG: 0x22, KeyH: 0x23, KeyJ: 0x24, KeyK: 0x25, KeyL: 0x26,
  Semicolon: 0x27, Quote: 0x28, Backquote: 0x29, ShiftLeft: 0x2a, Backslash: 0x2b, KeyZ: 0x2c, KeyX: 0x2d, KeyC: 0x2e, KeyV: 0x2f,
  KeyB: 0x30, KeyN: 0x31, KeyM: 0x32, Comma: 0x33, Period: 0x34, Slash: 0x35, ShiftRight: 0x36, NumpadMultiply: 0x37, AltLeft: 0x38,
  Space: 0x39, CapsLock: 0x3a, F1: 0x3b, F2: 0x3c, F3: 0x3d, F4: 0x3e, F5: 0x3f, F6: 0x40, F7: 0x41, F8: 0x42, F9: 0x43, F10: 0x44,
  NumLock: 0x45, ScrollLock: 0x46, Numpad7: 0x47, Numpad8: 0x48, Numpad9: 0x49, NumpadSubtract: 0x4a, Numpad4: 0x4b, Numpad5: 0x4c,
  Numpad6: 0x4d, NumpadAdd: 0x4e, Numpad1: 0x4f, Numpad2: 0x50, Numpad3: 0x51, Numpad0: 0x52, NumpadDecimal: 0x53,
  IntlBackslash: 0x56, F11: 0x57, F12: 0x58,
};

// The keys that were added to the keyboard later: the same numbers, marked as added.
const ADDED: Record<string, number> = {
  NumpadEnter: 0x1c, ControlRight: 0x1d, NumpadDivide: 0x35, PrintScreen: 0x37, AltRight: 0x38, Home: 0x47, ArrowUp: 0x48,
  PageUp: 0x49, ArrowLeft: 0x4b, ArrowRight: 0x4d, End: 0x4f, ArrowDown: 0x50, PageDown: 0x51, Insert: 0x52, Delete: 0x53,
  MetaLeft: 0x5b, MetaRight: 0x5c, ContextMenu: 0x5d,
};

// A key's number, and whether it is one of the added keys. Nothing for a key a machine
// has no number for.
export function placeOf(code: string): { code: number; extended: boolean } | null {
  const place = PLACES[code];
  if (place !== undefined) return { code: place, extended: false };
  const added = ADDED[code];
  return added !== undefined ? { code: added, extended: true } : null;
}

// The keys that do something and write nothing, as X11 numbers them: by the name a
// browser gives what a key does. A machine that speaks VNC is told of keys by what they
// write, since it has no layout of the person's to read places with.
const DOES: Record<string, number> = {
  Backspace: 0xff08, Tab: 0xff09, Enter: 0xff0d, Pause: 0xff13, ScrollLock: 0xff14, Escape: 0xff1b, Home: 0xff50, ArrowLeft: 0xff51,
  ArrowUp: 0xff52, ArrowRight: 0xff53, ArrowDown: 0xff54, PageUp: 0xff55, PageDown: 0xff56, End: 0xff57, PrintScreen: 0xff61,
  Insert: 0xff63, ContextMenu: 0xff67, NumLock: 0xff7f, CapsLock: 0xffe5, Delete: 0xffff, AltGraph: 0xfe03,
  F1: 0xffbe, F2: 0xffbf, F3: 0xffc0, F4: 0xffc1, F5: 0xffc2, F6: 0xffc3, F7: 0xffc4, F8: 0xffc5, F9: 0xffc6, F10: 0xffc7,
  F11: 0xffc8, F12: 0xffc9,
};
// The keys held with others: the left one of each, and the right one after it. The
// command key of a Mac and the Windows key are one key to such a machine.
const HELD: Record<string, [number, number]> = { Shift: [0xffe1, 0xffe2], Control: [0xffe3, 0xffe4], Alt: [0xffe9, 0xffea], Meta: [0xffeb, 0xffec], OS: [0xffeb, 0xffec] };

// The number of a character as X11 has what is written: the western alphabet's by its
// own number, and every other one above a mark.
function symOfCharacter(point: number): number | null {
  if (point < 0x20 || (point >= 0x7f && point < 0xa0)) return null;
  return point < 0x100 ? point : 0x01000000 + point;
}

// A key by what it writes or does, from what a browser says of it: what the key
// writes, and its place, which tells the left key from the right. Nothing for a key
// that neither writes one character nor is known.
export function symOf(key: string, code: string): number | null {
  const held = HELD[key];
  if (held) return held[code.endsWith('Right') ? 1 : 0];
  const does = DOES[key];
  if (does !== undefined) return does;
  const point = key.codePointAt(0);
  // one character, also one beyond the first sixty thousand
  if (point === undefined || key.length !== (point > 0xffff ? 2 : 1)) return null;
  return symOfCharacter(point);
}

// A key by its place, as it writes with no other key held on a keyboard of the United
// States: for the key combinations Away sends by itself.
export function symOfPlace(code: string): number | null {
  const letter = /^Key([A-Z])$/.exec(code)?.[1];
  if (letter) return letter.toLowerCase().charCodeAt(0);
  const digit = /^Digit([0-9])$/.exec(code)?.[1];
  if (digit) return digit.charCodeAt(0);
  if (code === 'Space') return 0x20;
  const held = /^(Shift|Control|Alt|Meta)(Left|Right)$/.exec(code);
  if (held) return HELD[held[1] ?? '']?.[held[2] === 'Right' ? 1 : 0] ?? null;
  return DOES[code] ?? null;
}

// Whether a key, by its number, is one that is held with others.
export function isHeld(sym: number): boolean {
  return (sym >= 0xffe1 && sym <= 0xffee) || sym === 0xfe03;
}

// The layouts Away tells apart, as Windows numbers them.
const US = 0x0409;
const BY_LANGUAGE: [string, number][] = [
  ['de-ch', 0x0807], ['fr-ch', 0x100c], ['it-ch', 0x0807], ['de', 0x0407], ['fr-be', 0x080c], ['fr-ca', 0x0c0c], ['fr', 0x040c],
  ['en-gb', 0x0809], ['en-ie', 0x0809], ['it', 0x0410], ['es', 0x040a], ['pt-br', 0x0416], ['pt', 0x0816], ['da', 0x0406],
  ['sv', 0x041d], ['nb', 0x0414], ['no', 0x0414], ['fi', 0x040b],
];

// The layouts Away tells apart by name: the language each writes, short as a taskbar
// shows it, and in full.
const NAMES: Record<number, [string, string]> = {
  0x0409: ['EN', 'English (United States)'],
  0x0809: ['EN', 'English (United Kingdom)'],
  0x0807: ['DE', 'German (Switzerland)'],
  0x0407: ['DE', 'German'],
  0x100c: ['FR', 'French (Switzerland)'],
  0x080c: ['FR', 'French (Belgium)'],
  0x0c0c: ['FR', 'French (Canada)'],
  0x040c: ['FR', 'French'],
  0x0410: ['IT', 'Italian'],
  0x040a: ['ES', 'Spanish'],
  0x0416: ['PT', 'Portuguese (Brazil)'],
  0x0816: ['PT', 'Portuguese'],
  0x0406: ['DA', 'Danish'],
  0x041d: ['SV', 'Swedish'],
  0x0414: ['NB', 'Norwegian'],
  0x040b: ['FI', 'Finnish'],
};

// A layout's language, short and in full.
export function layoutName(layout: number): { short: string; name: string } {
  const [short, name] = NAMES[layout] ?? ['EN', 'English (United States)'];
  return { short, name };
}

// The layout of the person's keyboard. Where the browser says what the keys write, that
// decides. Elsewhere it is taken from the language the browser is set to, and is the
// one of the United States where that says nothing.
export async function layoutOf(): Promise<number> {
  const keyboard = (navigator as { keyboard?: { getLayoutMap?: () => Promise<{ get: (code: string) => string | undefined }> } }).keyboard;
  try {
    const writes = await keyboard?.getLayoutMap?.();
    if (writes) {
      if (writes.get('KeyY') === 'z') {
        // the keyboards that have Z where others have Y
        if (writes.get('Minus') === 'ß') return 0x0407;
        return writes.get('BracketLeft') === 'è' ? 0x100c : 0x0807;
      }
      if (writes.get('KeyQ') === 'a') return writes.get('Digit1') === '&' && writes.get('Minus') === ')' ? 0x040c : 0x080c;
      if (writes.get('Backslash') === '#') return 0x0809;
      if (writes.get('KeyY') === 'y' && writes.get('Semicolon') === ';') return US;
    }
  } catch {
    // a browser that says nothing of the keyboard: its language is what there is
  }
  const language = navigator.language.toLowerCase();
  return BY_LANGUAGE.find(([begins]) => language === begins || language.startsWith(`${begins}-`))?.[1] ?? US;
}
