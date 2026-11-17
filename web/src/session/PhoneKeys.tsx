// The keyboard of a phone, for a machine: the phone's own keyboard comes up for a field
// that nobody sees, and what is typed there goes to the machine. Over it lies a row of
// the keys such a keyboard lacks: Esc, Tab, Ctrl, Alt, the arrows, and behind fn the
// function keys and the keys that move through a page.
//
// A phone's keyboard says little of the keys that go down: it changes the field's text.
// So the field always holds a few characters, and what is typed is read off how its
// text changed: what went is taken back on the machine, and what came is typed there.
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { ReactNode, RefObject } from 'react';

import { texts } from '../texts';
import { placeOf, symOf, symOfPlace } from './keys';
import { did } from './wire';
import type { Did } from './wire';

// What the field holds while nothing is typed: something to take back, so that the key
// that takes back says so.
const HOLDS = String.fromCharCode(0xa0).repeat(8);

// A key that is held for the next one, by the place it has on a keyboard.
const HELD = { ctrl: 'ControlLeft', alt: 'AltLeft', cmd: 'MetaLeft' } as const;
type Held = keyof typeof HELD;

// A key by its place, going down or coming up, as this machine is told of keys: by
// what they write, or by where they are.
function key(written: boolean, code: string, down: boolean): Did[] {
  if (written) {
    const sym = symOfPlace(code);
    return sym === null ? [] : [{ written: { sym, down } }];
  }
  const place = placeOf(code);
  return place === null ? [] : [{ key: { ...place, down } }];
}

// A character, typed: down and up again. A machine that is told of keys by their places
// is told the character itself, whatever its own layout is.
function character(written: boolean, one: string): Did[] {
  if (written) {
    const sym = symOf(one, '');
    return sym === null ? [] : [{ written: { sym, down: true } }, { written: { sym, down: false } }];
  }
  return Array.from({ length: one.length }, (_, nth) => one.charCodeAt(nth)).flatMap((unit) => [
    { character: { unit, down: true } },
    { character: { unit, down: false } },
  ]);
}

// The key that writes a letter or a digit on a keyboard, by its place: with Ctrl, Alt or
// the command key held, a machine wants the key and not what it writes.
function placeOfCharacter(one: string): string | null {
  if (/^[a-z]$/i.test(one)) return `Key${one.toUpperCase()}`;
  if (/^[0-9]$/.test(one)) return `Digit${one}`;
  return one === ' ' ? 'Space' : null;
}

export function PhoneKeys({
  shown,
  machine,
  mac,
  written,
  say,
  field,
  onGone,
}: {
  shown: boolean;
  machine: string;
  mac: boolean;
  written: boolean;
  say: (message: Uint8Array<ArrayBuffer>) => void;
  field: RefObject<HTMLTextAreaElement | null>;
  onGone: () => void;
}) {
  const t = texts.session.phoneKeys;
  const [held, setHeld] = useState<Held[]>([]);
  const [further, setFurther] = useState(false);
  const row = useRef<HTMLDivElement | null>(null);
  // what the field held before it changed
  const before = useRef(HOLDS);

  // the field as it rests: it holds what there is to take back, and what is typed
  // comes after it
  const rests = (typed: HTMLTextAreaElement) => {
    typed.value = HOLDS;
    typed.setSelectionRange(HOLDS.length, HOLDS.length);
    before.current = HOLDS;
  };
  // a key with the held ones around it: they go down before it and come up after it,
  // and are let go
  const withHeld = (events: Did[], around: Held[]) => {
    const down = around.flatMap((one) => key(written, HELD[one], true));
    const up = [...around].reverse().flatMap((one) => key(written, HELD[one], false));
    if (events.length > 0) say(did([...down, ...events, ...up]));
    setHeld([]);
  };
  const tapped = (code: string) => [...key(written, code, true), ...key(written, code, false)];
  const tap = (code: string) => withHeld(tapped(code), held);
  // what was typed, a character at a time: the held keys are around the first one alone
  const type = (text: string) => {
    let around = held;
    for (const one of text) {
      const place = around.length > 0 ? placeOfCharacter(one) : null;
      withHeld(one === '\n' ? tapped('Enter') : place !== null ? tapped(place) : character(written, one), around);
      around = [];
    }
  };

  // the phone's keyboard covers the foot of the window: the row lies over it, and the
  // picture has the room above
  useEffect(() => {
    const session = row.current?.closest<HTMLElement>('.sess');
    const window_ = window.visualViewport;
    if (!shown || !session) return undefined;
    const covered = () => {
      const below = window_ ? Math.max(0, window.innerHeight - window_.height - window_.offsetTop) : 0;
      session.style.setProperty('--kb', `${Math.round(below)}px`);
    };
    covered();
    session.classList.add('typing');
    window_?.addEventListener('resize', covered);
    window_?.addEventListener('scroll', covered);
    return () => {
      window_?.removeEventListener('resize', covered);
      window_?.removeEventListener('scroll', covered);
      session.classList.remove('typing');
      session.style.removeProperty('--kb');
    };
  }, [shown]);

  // a key of the row is tapped without the field losing the keyboard
  const of = (name: string, does: () => void, label?: ReactNode, pressed?: boolean) => (
    <button key={name} type="button" aria-label={label ? name : undefined} aria-pressed={pressed} onPointerDown={(event) => event.preventDefault()} onClick={does}>
      {label ?? name}
    </button>
  );
  const holds = (one: Held, name: string) =>
    of(name, () => setHeld((now) => (now.includes(one) ? now.filter((other) => other !== one) : [...now, one])), undefined, held.includes(one));

  return (
    <>
      <textarea
        ref={field}
        className="styped"
        aria-label={t.typed(machine)}
        aria-hidden={shown ? undefined : true}
        tabIndex={-1}
        autoCapitalize="off"
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
        defaultValue={HOLDS}
        onFocus={(event) => rests(event.currentTarget)}
        onBlur={() => {
          if (shown) onGone();
        }}
        onKeyDown={(event) => {
          // a key that writes nothing changes no text: one of a keyboard that is plugged in
          if (['Escape', 'Tab', 'ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown', 'Delete'].includes(event.key) || /^F\d+$/.test(event.key)) {
            event.preventDefault();
            tap(event.key);
          }
        }}
        onInput={(event) => {
          // what the text has in common with what it was, at its beginning and at its
          // end: between the two, something went and something came
          const [now, was] = [event.currentTarget.value, before.current];
          let first = 0;
          while (first < now.length && first < was.length && now[first] === was[first]) first += 1;
          let last = 0;
          while (last < now.length - first && last < was.length - first && now[now.length - 1 - last] === was[was.length - 1 - last]) last += 1;
          for (let gone = was.length - first - last; gone > 0; gone -= 1) tap('Backspace');
          type(now.slice(first, now.length - last));
          // a word that is still being made stays, for the keyboard to change it again
          if ((event.nativeEvent as InputEvent).isComposing) before.current = now;
          else rests(event.currentTarget);
        }}
        onCompositionEnd={(event) => rests(event.currentTarget)}
      />
      {shown && (
        <div className="skeys" role="group" aria-label={t.row} ref={row}>
          {further
            ? [
                ...Array.from({ length: 12 }, (_, nth) => of(`f${nth + 1}`, () => tap(`F${nth + 1}`))),
                of(t.home, () => tap('Home')),
                of(t.end, () => tap('End')),
                of(t.pageUp, () => tap('PageUp')),
                of(t.pageDown, () => tap('PageDown')),
              ]
            : [
                of(t.esc, () => tap('Escape')),
                of(t.tab, () => tap('Tab')),
                holds('ctrl', t.ctrl),
                holds('alt', mac ? t.opt : t.alt),
                ...(mac ? [holds('cmd', t.cmd)] : []),
                of(t.left, () => tap('ArrowLeft'), <ArrowLeft size={16} aria-hidden="true" />),
                of(t.up, () => tap('ArrowUp'), <ArrowUp size={16} aria-hidden="true" />),
                of(t.down, () => tap('ArrowDown'), <ArrowDown size={16} aria-hidden="true" />),
                of(t.right, () => tap('ArrowRight'), <ArrowRight size={16} aria-hidden="true" />),
              ]}
          {of(t.fn, () => setFurther(!further), undefined, further)}
        </div>
      )}
    </>
  );
}
