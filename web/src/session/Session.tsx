// A session: a machine's desktop in the browser, or one of its programs. While it
// opens, the steps of the connection show, and the machine's question for a name and a
// password. Once it is open the machine has the whole window, and Away shows nothing of
// its own but a handle. A program's windows show on Away's ground, where the machine
// has them on its desktop.
import { Button, Chevron, EmptyState, Field, IconButton, Input, Spinner } from '@nalet/design-system';
import { AppWindow, Check, Keyboard, Maximize, Minimize, Monitor, Power, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { FormEvent, KeyboardEvent as KeyOfReact } from 'react';

import { pageOf, useMachines } from '../machines';
import type { Machine } from '../machines';
import { addressOf } from '../place';
import { texts } from '../texts';
import { layoutOf, placeOf } from './keys';
import { begins, credentials, did, leaves, readRectangle, readSaid, seen, windowDoes } from './wire';
import type { Did, Step, WindowOf } from './wire';

const STEPS: Step[] = ['reaching', 'checking', 'signing-in', 'first-picture'];

// How far a session has come.
type State =
  | { is: 'opening'; step: Step | null; asks: { again: boolean } | null }
  | { is: 'refused'; status: number }
  | { is: 'failed'; step: Step; why: string; details: string }
  | { is: 'open' }
  | { is: 'ended'; why: string };

// One session with a machine's desktop, or with one of its programs: from asking the
// server for it to its end.
function useSession(machine: string, program: string | null) {
  const [state, setState] = useState<State>({ is: 'opening', step: null, asks: null });
  // the windows of the programs, from the front one back
  const [windows, setWindows] = useState<WindowOf[]>([]);
  const [attempt, setAttempt] = useState(0);
  // the picture is drawn on a canvas of the session's own, which a page shows once it is open
  const [canvas] = useState(() => document.createElement('canvas'));
  const socket = useRef<WebSocket | null>(null);

  useEffect(() => {
    let over = false;
    let connection: WebSocket | null = null;
    // what arrives is drawn in the order it arrived, and confirmed once it is drawn
    let drawing: Promise<void> = Promise.resolve();
    setState({ is: 'opening', step: null, asks: null });
    setWindows([]);
    const begin = async () => {
      const [asked, layout] = await Promise.all([
        fetch(`/api/sessions?${new URLSearchParams(program === null ? { machine } : { machine, program })}`, { method: 'POST', headers: { 'x-away': '1' } }),
        layoutOf(),
      ]);
      if (over) return;
      if (!asked.ok) {
        setState({ is: 'refused', status: asked.status });
        return;
      }
      const { ticket } = (await asked.json()) as { ticket: string };
      if (over) return;
      const opened = new WebSocket(`${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/session`);
      opened.binaryType = 'arraybuffer';
      connection = opened;
      socket.current = opened;
      opened.onopen = () => opened.send(begins(ticket, window.innerWidth, window.innerHeight, layout, Math.round(window.devicePixelRatio * 100)));
      opened.onmessage = (event: MessageEvent<string | ArrayBuffer>) => {
        if (typeof event.data !== 'string') {
          const rectangle = readRectangle(event.data);
          if (!rectangle) return;
          drawing = drawing
            .then(async () => {
              const picture = await createImageBitmap(new Blob([rectangle.png], { type: 'image/png' }));
              canvas.getContext('2d')?.drawImage(picture, rectangle.left, rectangle.top);
              picture.close();
            })
            .catch(() => undefined);
          return;
        }
        const said = readSaid(event.data);
        if (!said) return;
        if ('step' in said) setState({ is: 'opening', step: said.step, asks: null });
        else if ('asks' in said) setState({ is: 'opening', step: 'signing-in', asks: said.asks });
        else if ('desktop' in said) {
          const { width, height } = said.desktop;
          drawing = drawing.then(() => {
            canvas.width = width;
            canvas.height = height;
            if (!over) setState({ is: 'open' });
          });
        } else if ('drawn' in said) {
          const number = said.drawn;
          drawing = drawing.then(() => {
            if (opened.readyState === WebSocket.OPEN) opened.send(seen(number));
          });
        } else if ('failed' in said) setState({ is: 'failed', ...said.failed });
        else if ('ended' in said) setState({ is: 'ended', why: said.ended });
        else if ('windows' in said) setWindows(said.windows);
      };
      opened.onclose = () => {
        if (over) return;
        // a connection that ends without a word: lost, or never a session
        setState((before) => (before.is === 'failed' || before.is === 'ended' ? before : { is: 'ended', why: before.is === 'open' ? 'lost' : 'no-session' }));
      };
    };
    begin().catch(() => {
      if (!over) setState({ is: 'refused', status: 0 });
    });
    return () => {
      over = true;
      if (connection?.readyState === WebSocket.OPEN) connection.send(leaves());
      connection?.close();
      socket.current = null;
    };
  }, [machine, program, attempt, canvas]);

  const say = useCallback((message: string | Uint8Array<ArrayBuffer>) => {
    if (socket.current?.readyState === WebSocket.OPEN) socket.current.send(message);
  }, []);
  return { state, windows, canvas, say, again: () => setAttempt((before) => before + 1) };
}

// The page of a session with a machine's desktop, or with one of its programs.
export function Session({ name, program, go, contact }: { name: string; program: string | null; go: (to: string) => void; contact: string | null }) {
  const [machines] = useMachines();
  const { state, windows, canvas, say, again } = useSession(name, program);
  const machine = typeof machines === 'string' ? null : (machines.find((one) => one.name === name) ?? null);
  const toMachines = () => go(machine ? pageOf(name) : addressOf.machines);

  useEffect(() => {
    document.title = `${program ?? name} · ${texts.name}`;
  }, [name, program]);
  // a person who left, or closed the program, is back where the machine is
  useEffect(() => {
    if (state.is === 'ended' && (state.why === 'left' || state.why === 'closed')) go(pageOf(name));
  }, [state, go, name]);

  if (state.is === 'open') return <Desktop name={name} program={program} windows={windows} canvas={canvas} say={say} />;
  const t = texts.session;
  if (state.is === 'refused' || state.is === 'ended') {
    const says = state.is === 'refused' ? t.refused(state.status, name) : t.ended(state.why, name);
    return (
      <div className="stage">
        <h1 className="unseen asis">{program ?? name}</h1>
        <EmptyState
          illustration={state.is === 'refused' && state.status === 404 ? 'unknown' : 'error'}
          size="lg"
          headingLevel={2}
          title={says.title}
          description={says.why}
          actions={
            <>
              <Button variant="default" onClick={toMachines}>
                {t.machines}
              </Button>
              {says.again && (
                <Button variant="primary" onClick={again}>
                  {state.is === 'ended' ? t.openAgain : t.tryAgain}
                </Button>
              )}
            </>
          }
        />
        {contact && <p className="turn-to">{texts.contact(contact)}</p>}
      </div>
    );
  }
  return <Opening name={name} program={program} machine={machine} state={state} say={say} again={again} toMachines={toMachines} contact={contact} />;
}

// The steps of a connection: which are done, which runs, which failed and why, and what
// the machine asks for.
function Opening({
  name,
  program,
  machine,
  state,
  say,
  again,
  toMachines,
  contact,
}: {
  name: string;
  program: string | null;
  machine: Machine | null;
  state: Extract<State, { is: 'opening' | 'failed' }>;
  say: (message: string) => void;
  again: () => void;
  toMachines: () => void;
  contact: string | null;
}) {
  const t = texts.session;
  const [typed, setTyped] = useState({ name: '', password: '' });
  const [sent, setSent] = useState(false);
  const asks = state.is === 'opening' ? state.asks : null;
  // a new question takes a new answer
  useEffect(() => setSent(false), [asks]);
  const at = state.step === null ? -1 : STEPS.indexOf(state.step);
  const failed = state.is === 'failed';
  const signIn = (event: FormEvent) => {
    event.preventDefault();
    if (!typed.name || !typed.password) return;
    setSent(true);
    say(credentials(typed.name, typed.password));
    setTyped((before) => ({ ...before, password: '' }));
  };
  const why = failed ? t.failed(state.step, state.why, name, machine?.address ?? name) : null;
  return (
    <form className="stage" onSubmit={signIn}>
      <div className="box">
        <div className="who">
          {program === null ? <Monitor size={28} aria-hidden="true" /> : <AppWindow size={28} aria-hidden="true" />}
          <div className="col">
            <h1 className="name asis">{program ?? name}</h1>
            <span className="label">
              {program !== null ? texts.programOn(name) : machine ? texts.machine.desktopOf[machine.kind] : texts.machine.desktop}
            </span>
          </div>
        </div>
        <ol className="steps" aria-label={t.stepsOf(name)}>
          {STEPS.map((step, nth) => {
            const is = nth < at ? 'done' : nth > at ? 'todo' : failed ? 'failed' : 'now';
            const asked = is === 'now' && step === 'signing-in' && asks !== null && !sent;
            return (
              <li key={step} className={asked ? 'now ask' : is} aria-current={is === 'now' ? 'step' : undefined}>
                <span className="g">
                  {is === 'done' && <Check size={16} aria-label={t.done} />}
                  {is === 'failed' && <X size={16} aria-label={t.failedAt} />}
                  {is === 'now' && (asked ? <Chevron size={14} aria-hidden="true" /> : <Spinner size={16} label={t.running} />)}
                </span>
                <div className="grow">
                  {t.steps[step](name)}
                  {is === 'failed' && why && (
                    <span className="why" role="alert">
                      {why.says}
                    </span>
                  )}
                  {asked && (
                    <>
                      <span className="why" role={asks.again ? 'alert' : undefined}>
                        {asks.again ? t.asksAgain(name) : t.asks(name)}
                      </span>
                      <div className="form">
                        <Field label={t.name}>
                          <Input
                            autoFocus={!asks.again}
                            autoComplete="username"
                            autoCapitalize="none"
                            spellCheck={false}
                            value={typed.name}
                            onChange={(event) => setTyped((before) => ({ ...before, name: event.target.value }))}
                          />
                        </Field>
                        <Field label={t.password}>
                          <Input
                            type="password"
                            autoFocus={asks.again}
                            autoComplete="off"
                            value={typed.password}
                            onChange={(event) => setTyped((before) => ({ ...before, password: event.target.value }))}
                          />
                        </Field>
                      </div>
                    </>
                  )}
                </div>
              </li>
            );
          })}
        </ol>
        {failed && state.details && (
          <details className="dtl">
            <summary>{t.details}</summary>
            <p>{state.details}</p>
          </details>
        )}
        {failed && contact && <p className="turn-to">{texts.contact(contact)}</p>}
        <div className="ft">
          {failed ? (
            <>
              <Button type="button" variant={why?.again ? 'default' : 'primary'} onClick={toMachines}>
                {t.machines}
              </Button>
              {why?.again && (
                <Button type="button" variant="primary" onClick={again}>
                  {t.tryAgain}
                </Button>
              )}
            </>
          ) : (
            <>
              <Button type="button" variant="ghost" onClick={toMachines}>
                {t.cancel}
              </Button>
              {asks !== null && !sent && (
                <Button type="submit" variant="primary" disabled={!typed.name || !typed.password}>
                  {t.signIn}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
    </form>
  );
}

// A desktop that is open: the machine's screen has the window, and every key, every
// The key combinations a browser or a device keeps for itself, each as its keys in the
// order they go down: the session's bar sends one with a click.
const KEPT: { name: string; keys: string[] }[] = [
  { name: texts.session.kept.ctrlAltDel, keys: ['ControlLeft', 'AltLeft', 'Delete'] },
  { name: texts.session.kept.altTab, keys: ['AltLeft', 'Tab'] },
  { name: texts.session.kept.altF4, keys: ['AltLeft', 'F4'] },
  { name: texts.session.kept.windows, keys: ['MetaLeft'] },
  { name: texts.session.kept.ctrlW, keys: ['ControlLeft', 'KeyW'] },
];

// What a browser that gives a page every key is asked through, where it has it.
type KeysOfBrowser = { lock?: () => Promise<void>; unlock?: () => void };

// click and the wheel go to the machine. Of a program, the windows show where the
// machine has them on its desktop, and what is around them is Away's ground.
function Desktop({
  name,
  program,
  windows,
  canvas,
  say,
}: {
  name: string;
  program: string | null;
  windows: WindowOf[];
  canvas: HTMLCanvasElement;
  say: (message: string | Uint8Array<ArrayBuffer>) => void;
}) {
  const t = texts.session;
  const [bar, setBar] = useState(false);
  const [filled, setFilled] = useState(Boolean(document.fullscreenElement));
  // whether the menu of the keys is open, and what holds its button and the menu
  const [keys, setKeys] = useState(false);
  const keysOf = useRef<HTMLSpanElement | null>(null);
  const screen = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    // while the session fills the screen, a browser that allows it gives every key to
    // the page, and so to the machine: the keys it keeps for itself otherwise, too
    const keyboard = (navigator as { keyboard?: KeysOfBrowser }).keyboard;
    const changed = () => {
      const fills = Boolean(document.fullscreenElement);
      setFilled(fills);
      if (fills) keyboard?.lock?.().catch(() => undefined);
      else keyboard?.unlock?.();
    };
    document.addEventListener('fullscreenchange', changed);
    return () => {
      document.removeEventListener('fullscreenchange', changed);
      keyboard?.unlock?.();
    };
  }, []);

  // the menu's first key combination has the focus when it opens
  useEffect(() => {
    if (keys) keysOf.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [keys]);

  useEffect(() => {
    const holds = screen.current;
    if (!holds) return undefined;
    canvas.tabIndex = 0;
    canvas.className = 'screen';
    canvas.setAttribute('role', 'application');
    canvas.setAttribute('aria-label', program === null ? t.screenOf(name) : t.windowsOf(program));
    // of a program only the windows show: the parts of them that can be seen
    canvas.style.clipPath = program === null ? '' : 'url(#seen-parts)';
    holds.replaceChildren(canvas);
    canvas.focus();

    const tell = (events: Did[]) => say(did(events));
    // where the pointer is, in the desktop's own pixels
    const at = (event: MouseEvent) => {
      const shown = canvas.getBoundingClientRect();
      const within = (value: number, most: number) => Math.max(0, Math.min(most - 1, Math.round(value)));
      return {
        x: within(((event.clientX - shown.left) * canvas.width) / Math.max(1, shown.width), canvas.width),
        y: within(((event.clientY - shown.top) * canvas.height) / Math.max(1, shown.height), canvas.height),
      };
    };
    // the pointer's way is told once a frame: where it is, not every place it was
    let moved: { x: number; y: number } | null = null;
    let waiting = 0;
    const flush = () => {
      waiting = 0;
      if (moved) tell([{ moved }]);
      moved = null;
    };
    const onMove = (event: PointerEvent) => {
      moved = at(event);
      if (!waiting) waiting = window.requestAnimationFrame(flush);
    };
    const onButton = (down: boolean) => (event: PointerEvent) => {
      event.preventDefault();
      if (down) {
        setBar(false);
        canvas.focus();
        canvas.setPointerCapture(event.pointerId);
      }
      const place = at(event);
      moved = null;
      tell([{ moved: place }, { button: { button: event.button, down, ...place } }]);
    };
    const onDown = onButton(true);
    const onUp = onButton(false);
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      // a browser counts in pixels, lines or pages: a notch of a wheel is 120 to a machine
      const scale = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 800 : 1.2;
      const place = at(event);
      const turned = (by: number, sideways: boolean): Did[] =>
        by === 0 ? [] : [{ wheel: { by: Math.max(-255, Math.min(255, Math.round(by))), sideways, ...place } }];
      const events = [...turned(-event.deltaY * scale, false), ...turned(event.deltaX * scale, true)];
      if (events.length > 0) tell(events);
    };
    const onMenu = (event: Event) => event.preventDefault();

    // the keys that are down, so that none stays down on the machine when the window is left
    const down = new Map<string, { code: number; extended: boolean }>();
    let locksSaid = false;
    // Ctrl + Alt + Shift, pressed and let go with no other key, opens the session's bar
    let chord: Set<string> | null = new Set();
    const onKey = (isDown: boolean) => (event: KeyboardEvent) => {
      const place = placeOf(event.code);
      if (!place) return;
      event.preventDefault();
      const events: Did[] = [];
      if (isDown && !locksSaid) {
        locksSaid = true;
        events.push({ locks: { scroll: event.getModifierState('ScrollLock'), num: event.getModifierState('NumLock'), caps: event.getModifierState('CapsLock') } });
      }
      const modifier = /^(Control|Alt|Shift)(Left|Right)$/.exec(event.code)?.[1];
      if (isDown) {
        if (down.size === 0) chord = new Set();
        if (modifier) chord?.add(modifier);
        else chord = null;
        down.set(event.code, place);
      } else {
        down.delete(event.code);
        if (down.size === 0 && chord?.size === 3) setBar((before) => !before);
      }
      events.push({ key: { ...place, down: isDown } });
      tell(events);
    };
    const onKeyDown = onKey(true);
    const onKeyUp = onKey(false);
    const onBlur = () => {
      if (down.size > 0) tell([...down.values()].map((place) => ({ key: { ...place, down: false } })));
      down.clear();
      locksSaid = false;
      chord = null;
    };

    canvas.addEventListener('pointermove', onMove);
    canvas.addEventListener('pointerdown', onDown);
    canvas.addEventListener('pointerup', onUp);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    canvas.addEventListener('contextmenu', onMenu);
    canvas.addEventListener('keydown', onKeyDown);
    canvas.addEventListener('keyup', onKeyUp);
    canvas.addEventListener('blur', onBlur);
    return () => {
      onBlur();
      if (waiting) window.cancelAnimationFrame(waiting);
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('wheel', onWheel);
      canvas.removeEventListener('contextmenu', onMenu);
      canvas.removeEventListener('keydown', onKeyDown);
      canvas.removeEventListener('keyup', onKeyUp);
      canvas.removeEventListener('blur', onBlur);
    };
  }, [canvas, name, program, say, t]);

  const fill = () => {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => undefined);
    else document.documentElement.requestFullscreen().catch(() => undefined);
  };
  const close = () => {
    setKeys(false);
    setBar(false);
    canvas.focus();
  };
  // the keys go down in their order and come up in the other, as under a hand; then
  // the machine has the window again
  const send = (codes: string[]) => {
    const places = codes.map(placeOf).filter((place) => place !== null);
    const goDown: Did[] = places.map((place) => ({ key: { ...place, down: true } }));
    const comeUp: Did[] = [...places].reverse().map((place) => ({ key: { ...place, down: false } }));
    say(did([...goDown, ...comeUp]));
    close();
  };
  const inKeysMenu = (event: KeyOfReact<HTMLDivElement>) => {
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]'));
    const at = items.indexOf(document.activeElement as HTMLElement);
    const to = { ArrowDown: (at + 1) % items.length, ArrowUp: (at + items.length - 1) % items.length, Home: 0, End: items.length - 1 }[event.key];
    if (to !== undefined) {
      event.preventDefault();
      items[to]?.focus();
    } else if (event.key === 'Escape') {
      // the menu closes, and the bar stays
      event.preventDefault();
      event.stopPropagation();
      setKeys(false);
      keysOf.current?.querySelector('button')?.focus();
    } else if (event.key === 'Tab') setKeys(false);
  };
  // the parts of the windows that can be seen, as shares of the desktop's width and height
  const parts = windows.flatMap((window) => window.seen.map((part) => ({ of: window.id, part })));
  const listed = windows.filter((window) => window.listed);
  return (
    <div className={program === null ? 'sess' : 'sess prog'}>
      <h1 className="unseen asis">{program ?? name}</h1>
      {program !== null && (
        <svg className="unseen" width="0" height="0" aria-hidden="true">
          <clipPath id="seen-parts" clipPathUnits="objectBoundingBox">
            {parts.map(({ of, part: [left, top, right, bottom] }, nth) => (
              <rect
                key={`${of} ${nth}`}
                x={left / Math.max(1, canvas.width)}
                y={top / Math.max(1, canvas.height)}
                width={(right - left) / Math.max(1, canvas.width)}
                height={(bottom - top) / Math.max(1, canvas.height)}
              />
            ))}
          </clipPath>
        </svg>
      )}
      {bar ? (
        <div
          className="sbar"
          role="toolbar"
          aria-label={t.bar}
          onKeyDown={(event) => {
            if (event.key === 'Escape') close();
          }}
        >
          <span className="on">
            {program === null ? <Monitor size={16} aria-hidden="true" /> : <AppWindow size={16} aria-hidden="true" />}
            {program ?? name}
          </span>
          {program !== null && (
            <ul className="wins" aria-label={t.windows}>
              {listed.map((window) => (
                <li key={window.id}>
                  <button type="button" className="win" title={t.toFront(window.title)} onClick={() => say(windowDoes(window.id, 'front'))}>
                    {window.title}
                  </button>
                  <IconButton variant="ghost" size="sm" label={t.closeWindow(window.title)} title={t.closeWindow(window.title)} onClick={() => say(windowDoes(window.id, 'close'))}>
                    <X size={14} aria-hidden="true" />
                  </IconButton>
                </li>
              ))}
            </ul>
          )}
          <span className="grow" />
          <span
            className="menuof"
            ref={keysOf}
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget)) setKeys(false);
            }}
          >
            <IconButton
              variant="ghost"
              label={t.keys}
              title={t.keys}
              className={keys ? 'is-open' : undefined}
              aria-haspopup="menu"
              aria-expanded={keys}
              onClick={() => setKeys(!keys)}
              autoFocus
            >
              <Keyboard size={16} aria-hidden="true" />
            </IconButton>
            {keys && (
              <div className="sessmenu" role="menu" aria-label={t.keys} onKeyDown={inKeysMenu}>
                {KEPT.map((kept) => (
                  <button key={kept.name} type="button" role="menuitem" tabIndex={-1} className="nc-actionmenu__item" onClick={() => send(kept.keys)}>
                    <span className="nc-actionmenu__label">{kept.name}</span>
                  </button>
                ))}
              </div>
            )}
          </span>
          <IconButton variant="ghost" label={filled ? t.unfill : t.fill} title={filled ? t.unfill : t.fill} onClick={fill}>
            {filled ? <Minimize size={16} aria-hidden="true" /> : <Maximize size={16} aria-hidden="true" />}
          </IconButton>
          <IconButton variant="ghost" label={t.end} title={t.end} onClick={() => say(leaves())}>
            <Power size={16} aria-hidden="true" />
          </IconButton>
          <IconButton variant="ghost" label={t.closeBar} title={t.closeBar} onClick={close}>
            <X size={16} aria-hidden="true" />
          </IconButton>
        </div>
      ) : (
        <button type="button" className="handle" aria-label={t.openBar} title={t.openBar} onClick={() => setBar(true)} />
      )}
      <div className="holds" ref={screen} />
    </div>
  );
}
