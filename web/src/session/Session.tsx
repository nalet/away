// A session: a machine's desktop in the browser, or one of its programs. While it
// opens, the steps of the connection show, and the machine's question for a name and a
// password. Once it is open the machine has the whole window, and Away shows nothing of
// its own but a handle. A program's windows show on Away's ground, where the machine
// has them on its desktop.
import { Button, Chevron, EmptyState, Field, IconButton, Input, Spinner } from '@nalet/design-system';
import { AppWindow, Check, ChevronUp, ClipboardList, Keyboard, Maximize, Minimize, Monitor, Unplug, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { FormEvent, KeyboardEvent as KeyOfReact, PointerEvent as PointerOfReact, ReactNode } from 'react';

import { desktopOver, pageOf, useMachines } from '../machines';
import type { Machine, Program } from '../machines';
import { addressOf } from '../place';
import { texts } from '../texts';
import { ClipboardPanel, useClipboard } from './Clipboard';
import type { Passes } from './Clipboard';
import { isHeld, layoutName, layoutOf, placeOf, symOf, symOfPlace } from './keys';
import { begins, credentials, did, leaves, readIcon, readRectangle, readSaid, readTrayPicture, seen, sized, start, trayDoes, windowDoes } from './wire';
import type { Did, Step, TrayOf, WindowOf } from './wire';

const STEPS: Step[] = ['reaching', 'checking', 'signing-in', 'first-picture'];

// A desktop whose connection dropped: how often Away has tried again, and whether Away
// itself is being updated.
interface Lost {
  tries: number;
  updating: boolean;
}

// How far a session has come.
type State =
  | { is: 'opening'; step: Step | null; asks: { again: boolean; instead?: boolean; only?: 'password' } | null }
  | { is: 'refused'; status: number }
  | { is: 'failed'; step: Step; why: string; details: string }
  | { is: 'open' }
  | { is: 'ended'; why: string };

// One session with a machine's desktop, or with one of its programs: from asking the
// server for it to its end.
function useSession(machine: string, program: string | null, method?: string) {
  const [state, setState] = useState<State>({ is: 'opening', step: null, asks: null });
  // the windows of the programs, from the front one back
  const [windows, setWindows] = useState<WindowOf[]>([]);
  const [inFront, setInFront] = useState<number | null>(null);
  const [attempt, setAttempt] = useState(0);
  // which ways the clipboard passes, and the text that was last copied on the machine
  const [passes, setPasses] = useState<Passes | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  // the icons of the windows, each as a picture the page holds
  const [icons, setIcons] = useState<Record<number, ImageBitmap>>({});
  // the icons of the tray, and their pictures, each by its window and its number
  const [tray, setTray] = useState<TrayOf[]>([]);
  const [trayPictures, setTrayPictures] = useState<Record<string, ImageBitmap>>({});
  // the keyboard's layout the machine was told of, and what became of a program the
  // person asked for in the session, the last of them
  const [layout, setLayout] = useState<number | null>(null);
  // whether the machine is told of keys by what they write, and not by their places
  const [written, setWritten] = useState(false);
  const [started, setStarted] = useState<{ program: string; ok: boolean; details?: string } | null>(null);
  // the picture is drawn on a canvas of the session's own, which a page shows once it is open
  const [canvas] = useState(() => document.createElement('canvas'));
  const socket = useRef<WebSocket | null>(null);

  // a desktop that was open and whose connection dropped: how often Away has tried
  // again since, and whether Away itself is being updated
  const [lost, setLost] = useState<Lost | null>(null);
  const comingBack = useRef<{ now: () => void; leave: () => void } | null>(null);

  useEffect(() => {
    let over = false;
    let connection: WebSocket | null = null;
    // what arrives is drawn in the order it arrived, and confirmed once it is drawn
    let drawing: Promise<void> = Promise.resolve();
    // whether the desktop has been open, how often it was tried again since it last
    // was, and the wait before the next try, while one is waited for
    let wasOpen = false;
    let tries = 0;
    let waits: number | undefined;
    setState({ is: 'opening', step: null, asks: null });
    setWindows([]);
    setPasses(null);
    setCopied(null);
    setLost(null);
    setStarted(null);
    // the pictures of the icons are the page's until the session ends
    const pictures = new Map<number, ImageBitmap>();
    const ofTheTray = new Map<string, ImageBitmap>();
    setIcons({});
    setTray([]);
    setTrayPictures({});

    // The desktop goes on at the machine: Away tries again, at once and then at longer
    // intervals, until it is back or the person leaves.
    const again = (updating: boolean) => {
      if (over) return;
      connection?.close();
      tries += 1;
      setLost({ tries, updating });
      const after = [0, 1000, 2000, 4000, 8000, 15000][tries - 1] ?? 30000;
      waits = window.setTimeout(() => {
        waits = undefined;
        begin(true).catch(() => again(updating));
      }, after);
    };
    const begin = async (anew: boolean) => {
      const [asked, layout] = await Promise.all([
        fetch(`/api/sessions?${new URLSearchParams({ machine, ...(program === null ? {} : { program }), ...(method ? { signs_in: method } : {}) })}`, { method: 'POST', headers: { 'x-away': '1' } }),
        layoutOf(),
      ]);
      if (over) return;
      setLayout(layout);
      if (!asked.ok) {
        // while Away itself is not there, as when it is being updated, it is tried again
        if (anew && asked.status >= 500) throw new Error('Away is not there');
        setLost(null);
        setState({ is: 'refused', status: asked.status });
        return;
      }
      const { ticket } = (await asked.json()) as { ticket: string };
      if (over) return;
      const opened = new WebSocket(`${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/session`);
      opened.binaryType = 'arraybuffer';
      connection = opened;
      socket.current = opened;
      // whether this connection said how it ends: one that closes without a word is lost
      let told = false;
      opened.onopen = () => opened.send(begins(ticket, window.innerWidth - (docked(program) ? RAIL : 0), window.innerHeight, layout, Math.round(window.devicePixelRatio * 100)));
      opened.onmessage = (event: MessageEvent<string | ArrayBuffer>) => {
        if (typeof event.data !== 'string') {
          const inTheTray = readTrayPicture(event.data);
          if (inTheTray) {
            createImageBitmap(new Blob([inTheTray.png], { type: 'image/png' }))
              .then((picture) => {
                if (over) {
                  picture.close();
                  return;
                }
                const key = `${inTheTray.window}:${inTheTray.icon}`;
                ofTheTray.get(key)?.close();
                ofTheTray.set(key, picture);
                setTrayPictures(Object.fromEntries(ofTheTray));
              })
              .catch(() => undefined);
            return;
          }
          const icon = readIcon(event.data);
          if (icon) {
            // read as the rectangles are, so that the page loads no picture from anywhere
            createImageBitmap(new Blob([icon.png], { type: 'image/png' }))
              .then((picture) => {
                if (over) {
                  picture.close();
                  return;
                }
                pictures.get(icon.window)?.close();
                pictures.set(icon.window, picture);
                setIcons(Object.fromEntries(pictures));
              })
              .catch(() => undefined);
            return;
          }
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
        if ('step' in said) {
          // a desktop that comes back says nothing of its steps: the card over it does
          if (!anew) setState({ is: 'opening', step: said.step, asks: null });
        } else if ('asks' in said) {
          // a machine where people type asks again when the desktop comes back
          setLost(null);
          setState({ is: 'opening', step: 'signing-in', asks: said.asks });
        } else if ('desktop' in said) {
          const { width, height, keys } = said.desktop;
          drawing = drawing.then(() => {
            canvas.width = width;
            canvas.height = height;
            if (over) return;
            setWritten(keys === 'written');
            wasOpen = true;
            tries = 0;
            setLost(null);
            setState({ is: 'open' });
          });
        } else if ('drawn' in said) {
          const number = said.drawn;
          drawing = drawing.then(() => {
            if (opened.readyState === WebSocket.OPEN) opened.send(seen(number));
          });
        } else if ('failed' in said) {
          told = true;
          // a machine that can't be reached at this moment is tried again
          if (anew && said.failed.step === 'reaching') again(false);
          else {
            setLost(null);
            setState({ is: 'failed', ...said.failed });
          }
        } else if ('ended' in said) {
          told = true;
          // Away is being updated: the desktop goes on at the machine, and is opened again
          if (said.ended === 'updating' && wasOpen) again(true);
          else {
            setLost(null);
            setState({ is: 'ended', why: said.ended });
          }
        } else if ('windows' in said) {
          setWindows(said.windows);
          setInFront(said.in_front);
        }
        else if ('clipboard' in said) setPasses(said.clipboard);
        else if ('copied' in said) setCopied(said.copied);
        else if ('started' in said) setStarted(said.started);
        else if ('tray' in said) setTray(said.tray);
      };
      opened.onclose = () => {
        if (over || told || connection !== opened) return;
        // a connection that ends without a word: a desktop that was open is opened
        // again, and one that never was open was never a session
        if (wasOpen) again(false);
        else setState((before) => (before.is === 'failed' || before.is === 'ended' ? before : { is: 'ended', why: 'no-session' }));
      };
    };
    comingBack.current = {
      // the next try doesn't wait: where one is under way already, that one goes on
      now: () => {
        if (over || waits === undefined) return;
        window.clearTimeout(waits);
        waits = undefined;
        begin(true).catch(() => again(false));
      },
      // the person gives up, and is back where the machine is
      leave: () => {
        over = true;
        window.clearTimeout(waits);
        connection?.close();
        setLost(null);
        setState({ is: 'ended', why: 'left' });
      },
    };
    begin(false).catch(() => {
      if (!over) setState({ is: 'refused', status: 0 });
    });
    return () => {
      over = true;
      window.clearTimeout(waits);
      comingBack.current = null;
      if (connection?.readyState === WebSocket.OPEN) connection.send(leaves());
      connection?.close();
      socket.current = null;
      pictures.forEach((picture) => picture.close());
      ofTheTray.forEach((picture) => picture.close());
    };
  }, [machine, program, method, attempt, canvas]);

  const say = useCallback((message: string | Uint8Array<ArrayBuffer>) => {
    if (socket.current?.readyState === WebSocket.OPEN) socket.current.send(message);
  }, []);
  return {
    state,
    windows,
    inFront,
    icons,
    tray,
    trayPictures,
    layout,
    written,
    started,
    passes,
    copied,
    canvas,
    say,
    lost,
    tryNow: () => comingBack.current?.now(),
    leave: () => comingBack.current?.leave(),
    again: () => setAttempt((before) => before + 1),
  };
}

interface SessionProps {
  method?: string;
  name: string;
  program: string | null;
  go: (to: string) => void;
  contact: string | null;
}

// A desktop bookmark follows the machine's current setting before opening a session.
export function Session({ showMachine, ...session }: SessionProps & { showMachine: () => ReactNode }) {
  const [machines] = useMachines();
  const machine = typeof machines === 'string' ? null : (machines.find((one) => one.name === session.name) ?? null);
  if (session.program === null && machines === 'asking') return <div className="fill" aria-busy="true" />;
  if (session.program === null && machine?.native_rdp) return showMachine();
  return <BrowserSession {...session} machine={machine} />;
}

// The page of a session with a machine's desktop, or with one of its programs.
function BrowserSession({ name, program, method, go, contact, machine }: SessionProps & { machine: Machine | null }) {
  const { state, windows, inFront, icons, tray, trayPictures, layout, written, started, passes, copied, canvas, say, lost, tryNow, leave, again } = useSession(name, program, method);
  const toMachines = () => go(machine ? pageOf(name) : addressOf.machines);

  useEffect(() => {
    document.title = `${program ?? name} · ${texts.name}`;
  }, [name, program]);
  // a person who left, or closed the program, is back where the machine is
  useEffect(() => {
    if (state.is === 'ended' && (state.why === 'left' || state.why === 'closed')) go(pageOf(name));
  }, [state, go, name]);

  if (state.is === 'open') {
    return (
      <Desktop
        name={name}
        program={program}
        windows={windows}
        inFront={inFront}
        icons={icons}
        tray={tray}
        trayPictures={trayPictures}
        programs={machine?.programs ?? []}
        layout={layout}
        written={written}
        kind={machine?.kind ?? null}
        started={started}
        passes={passes}
        copied={copied}
        canvas={canvas}
        say={say}
        lost={lost}
        tryNow={tryNow}
        leave={leave}
        otherMachines={() => go(addressOf.machines)}
      />
    );
  }
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
                  {state.is !== 'ended' ? t.tryAgain : 'bring' in says && says.bring ? t.bringHere : t.openAgain}
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
  // a desktop over VNC shows nothing of itself that could be checked, and has a password
  // of its own in place of names
  const overVnc = program === null && machine !== null && desktopOver(machine) === 'vnc';
  const steps = overVnc ? STEPS.filter((step) => step !== 'checking') : STEPS;
  const passwordAlone = asks?.only === 'password';
  const at = state.step === null ? -1 : steps.indexOf(state.step);
  const failed = state.is === 'failed';
  const filled = typed.password !== '' && (passwordAlone || typed.name !== '');
  const signIn = (event: FormEvent) => {
    event.preventDefault();
    if (!filled) return;
    setSent(true);
    say(credentials(passwordAlone ? '' : typed.name, typed.password));
    setTyped((before) => ({ ...before, password: '' }));
  };
  const why = failed ? t.failed(state.step, state.why, name, machine?.address ?? name, overVnc) : null;
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
          {steps.map((step, nth) => {
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
                        {passwordAlone
                          ? asks.again
                            ? t.asksPasswordAgain(name)
                            : t.asksPassword(name)
                          : asks.again
                            ? t.asksAgain(name)
                            : asks.instead
                              ? t.asksInstead(name)
                              : t.asks(name)}
                      </span>
                      <div className="form">
                        {!passwordAlone && (
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
                        )}
                        <Field label={t.password}>
                          <Input
                            type="password"
                            autoFocus={asks.again || passwordAlone}
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
                <Button type="submit" variant="primary" disabled={!filled}>
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
// How wide the always-on rail is, in CSS pixels (matches .srail in away.css): a
// program's picture is the space beside it, so its windows never slip under the rail.
const RAIL = 56;

// A phone, held upright or sideways: the frame's own breakpoints (away.css). There a
// program's rail is not docked: it waits at the left edge and is swiped in.
const PHONE = '(max-width: 640px) and (orientation: portrait), (max-width: 480px), (max-height: 480px) and (orientation: landscape)';

// Whether a program's rail stands beside its windows, taking its own width: on a
// computer it does; a desktop has no rail, and on a phone the rail lies over the picture.
function docked(program: string | null): boolean {
  return program !== null && !window.matchMedia(PHONE).matches;
}

// Whether the page is on a phone, following the device as it is turned.
function usePhone(): boolean {
  const [phone, setPhone] = useState(() => window.matchMedia(PHONE).matches);
  useEffect(() => {
    const query = window.matchMedia(PHONE);
    const changed = () => setPhone(query.matches);
    query.addEventListener('change', changed);
    return () => query.removeEventListener('change', changed);
  }, []);
  return phone;
}

// The time and the day on the person's own device, as a taskbar shows them: the hours
// of the whole day, so that the time is as short as it gets.
function timeNow(): { time: string; day: string; at: string } {
  const now = new Date();
  return {
    time: now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }),
    day: now.toLocaleDateString([], { day: '2-digit', month: '2-digit' }),
    at: now.toISOString(),
  };
}

// A window's icon, as the machine gave it: drawn on a canvas of its own.
function WindowIcon({ picture }: { picture: ImageBitmap }) {
  const drawn = useRef<HTMLCanvasElement | null>(null);
  useEffect(() => {
    const canvas = drawn.current;
    if (!canvas) return;
    canvas.width = picture.width;
    canvas.height = picture.height;
    const context = canvas.getContext('2d');
    context?.clearRect(0, 0, canvas.width, canvas.height);
    context?.drawImage(picture, 0, 0);
  }, [picture]);
  return <canvas ref={drawn} className="icon" aria-hidden="true" />;
}

// The key combinations a browser or a device keeps for itself, each as its keys in the
// order they go down: the session's bar sends one with a click.
const KEPT: { name: string; keys: string[] }[] = [
  { name: texts.session.kept.ctrlAltDel, keys: ['ControlLeft', 'AltLeft', 'Delete'] },
  { name: texts.session.kept.altTab, keys: ['AltLeft', 'Tab'] },
  { name: texts.session.kept.altF4, keys: ['AltLeft', 'F4'] },
  { name: texts.session.kept.windows, keys: ['MetaLeft'] },
  { name: texts.session.kept.ctrlW, keys: ['ControlLeft', 'KeyW'] },
];

// The same for a Mac's desktop: the combinations of its command key, most of which a
// browser keeps for itself.
const KEPT_ON_A_MAC: { name: string; keys: string[] }[] = [
  { name: texts.session.keptOnAMac.cmdTab, keys: ['MetaLeft', 'Tab'] },
  { name: texts.session.keptOnAMac.cmdSpace, keys: ['MetaLeft', 'Space'] },
  { name: texts.session.keptOnAMac.cmdW, keys: ['MetaLeft', 'KeyW'] },
  { name: texts.session.keptOnAMac.cmdQ, keys: ['MetaLeft', 'KeyQ'] },
  { name: texts.session.keptOnAMac.cmdOptEsc, keys: ['MetaLeft', 'AltLeft', 'Escape'] },
];

// Whether the person's own device is a Mac, whose browser tells of some keys in its own way.
const ON_A_MAC = /Mac|iPhone|iPad/.test(navigator.platform);

// What a browser that gives a page every key is asked through, where it has it.
type KeysOfBrowser = { lock?: () => Promise<void>; unlock?: () => void };

// click and the wheel go to the machine. Of a program, the windows show where the
// machine has them on its desktop, and what is around them is Away's ground.
function Desktop({
  name,
  program,
  windows,
  inFront,
  icons,
  tray,
  trayPictures,
  programs,
  layout,
  written,
  kind,
  started,
  passes,
  copied,
  canvas,
  say,
  lost,
  tryNow,
  leave,
  otherMachines,
}: {
  name: string;
  program: string | null;
  windows: WindowOf[];
  inFront: number | null;
  icons: Record<number, ImageBitmap>;
  tray: TrayOf[];
  trayPictures: Record<string, ImageBitmap>;
  programs: Program[];
  layout: number | null;
  written: boolean;
  kind: Machine['kind'] | null;
  started: { program: string; ok: boolean; details?: string } | null;
  passes: Passes | null;
  copied: string | null;
  canvas: HTMLCanvasElement;
  say: (message: string | Uint8Array<ArrayBuffer>) => void;
  lost: Lost | null;
  tryNow: () => void;
  leave: () => void;
  otherMachines: () => void;
}) {
  const t = texts.session;
  const [bar, setBar] = useState(false);
  const [filled, setFilled] = useState(Boolean(document.fullscreenElement));
  // whether the menu of the keys is open, and what holds its button and the menu
  const [keys, setKeys] = useState(false);
  const keysOf = useRef<HTMLSpanElement | null>(null);
  // whether the panel of the clipboard is open
  const [clip, setClip] = useState(false);
  // whether the machine's programs are listed, under the mark of a program's rail; the
  // program asked for that is on its way, and the last one that didn't start
  const [launch, setLaunch] = useState(false);
  const [starting, setStarting] = useState<string | null>(null);
  const [notStarted, setNotStarted] = useState<{ program: string; details: string } | null>(null);
  // whether the icons of the tray are shown, under the button at the foot of the rail
  const [trayOpen, setTrayOpen] = useState(false);
  // the windows in the order they came, as a taskbar keeps them: one brought to the front
  // keeps its place
  const cameIn = useRef<number[]>([]);
  const clipboard = useClipboard(canvas, passes, copied, say);
  // the clock at the foot of the rail, and the first control, focused when a desktop's
  // rail opens. For a program the rail is always there, as a taskbar
  const [clock, setClock] = useState(timeNow);
  const firstControl = useRef<HTMLButtonElement | null>(null);
  // a program's rail stands always on a computer; on a phone it is swiped in, and a
  // desktop has the bar across its top instead
  const phone = usePhone();
  const railAlways = program !== null && !phone;
  // where a swipe began, on the left edge or on the rail
  const swipe = useRef<number | null>(null);
  // the card that says the connection is lost shows after two seconds without the
  // machine: a short gap shows nothing
  const isLost = lost !== null;
  const [gone, setGone] = useState(false);
  useEffect(() => {
    if (!isLost) {
      setGone(false);
      return undefined;
    }
    const shows = window.setTimeout(() => setGone(true), 2000);
    return () => window.clearTimeout(shows);
  }, [isLost]);
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

  // the desktop follows the window: once the window has rested at another size, the
  // gateway is told, and a machine that follows its client's screen offers its desktop
  // anew at that size
  useEffect(() => {
    let rests: number | undefined;
    const changed = () => {
      window.clearTimeout(rests);
      rests = window.setTimeout(() => say(sized(window.innerWidth - (docked(program) ? RAIL : 0), window.innerHeight, Math.round(window.devicePixelRatio * 100))), 300);
    };
    window.addEventListener('resize', changed);
    return () => {
      window.clearTimeout(rests);
      window.removeEventListener('resize', changed);
    };
  }, [say]);

  // the menu's first key combination has the focus when it opens
  useEffect(() => {
    if (keys) keysOf.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [keys]);

  // what the machine said of the program that was asked for: one that started shows its
  // window, and the list goes away; one that didn't says so in the list
  useEffect(() => {
    if (started === null) return;
    setStarting(null);
    if (started.ok) {
      setLaunch(false);
      setNotStarted(null);
    } else setNotStarted({ program: started.program, details: started.details ?? '' });
  }, [started]);

  // the clock keeps the time
  useEffect(() => {
    const tick = window.setInterval(() => setClock(timeNow()), 15000);
    return () => window.clearInterval(tick);
  }, []);

  // a desktop's rail, opened from the handle, puts the focus on its first control
  useEffect(() => {
    if (program !== null && !railAlways && bar) firstControl.current?.focus();
  }, [program, railAlways, bar]);

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
    // what the wheel turned and was less than the machine is told at once: up and down, and sideways
    const rest = [0, 0];
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      // a browser counts in pixels, lines or pages: a notch of a wheel is 120 to a machine,
      // and stands for a hundred pixels. A trackpad turns by little at a time, and what
      // is less than one part adds up, so that a slow turn is as far as a fast one
      const scale = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 800 : 1.2;
      const place = at(event);
      const turned = (turn: number, sideways: boolean): Did[] => {
        const nth = sideways ? 1 : 0;
        // counted in tenths, which are whole numbers: nothing of a turn is lost to rounding
        const tenths = (rest[nth] ?? 0) + Math.round(turn * 10);
        const whole = Math.trunc(tenths / 10);
        rest[nth] = tenths - whole * 10;
        const told: Did[] = [];
        // more than one message holds goes out as several
        for (let left = whole; left !== 0; ) {
          const by = Math.max(-255, Math.min(255, left));
          told.push({ wheel: { by, sideways, ...place } });
          left -= by;
        }
        return told;
      };
      const events = [...turned(-event.deltaY * scale, false), ...turned(event.deltaX * scale, true)];
      if (events.length > 0) tell(events);
    };
    const onMenu = (event: Event) => event.preventDefault();

    // the keys that are down, so that none stays down on the machine when the window is left
    const down = new Map<string, { code: number; extended: boolean }>();
    let locksSaid = false;
    // Ctrl + Alt + Shift, pressed and let go with no other key, opens the session's bar
    let chord: Set<string> | null = new Set();
    // the same for a machine that is told of keys by what they write: each key that is
    // down, by its place, with what it wrote as it went down
    const writing = new Map<string, number>();
    const onWritten = (isDown: boolean, event: KeyboardEvent) => {
      const modifier = /^(Control|Alt|Shift)(Left|Right)$/.exec(event.code)?.[1];
      if (!isDown) {
        const sym = writing.get(event.code);
        if (sym === undefined) return;
        event.preventDefault();
        writing.delete(event.code);
        if (writing.size === 0 && chord?.size === 3) setBar((before) => !before);
        tell([{ written: { sym, down: false } }]);
        return;
      }
      const sym = symOf(event.key, event.code);
      if (sym === null) return;
      event.preventDefault();
      if (writing.size === 0) chord = new Set();
      if (modifier) chord?.add(modifier);
      else chord = null;
      const events: Did[] = [];
      // on a Mac the option key makes another character of a key: the machine is told the
      // character, and the option key comes up before it, or it would make yet another of it
      if (ON_A_MAC && !isHeld(sym) && sym < 0xff00 && !event.ctrlKey && !event.metaKey) {
        for (const [code, held] of writing) {
          if (held === 0xffe9 || held === 0xffea) {
            events.push({ written: { sym: held, down: false } });
            writing.delete(code);
          }
        }
      }
      events.push({ written: { sym, down: true } });
      // a Mac's browser doesn't say when a key comes up while the command key is held: such
      // a key comes up at once
      if (ON_A_MAC && event.metaKey && !isHeld(sym)) events.push({ written: { sym, down: false } });
      else writing.set(event.code, sym);
      tell(events);
    };
    const onKey = (isDown: boolean) => (event: KeyboardEvent) => {
      if (written) return onWritten(isDown, event);
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
      if (writing.size > 0) tell([...writing.values()].map((sym) => ({ written: { sym, down: false } })));
      writing.clear();
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
  }, [canvas, name, program, say, t, written]);

  const fill = () => {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => undefined);
    else document.documentElement.requestFullscreen().catch(() => undefined);
  };
  const close = () => {
    setKeys(false);
    setClip(false);
    setLaunch(false);
    setTrayOpen(false);
    setBar(false);
    canvas.focus();
  };
  // an icon of the tray, chosen or asked for its menu: what opens, opens on the machine,
  // where the keys and the pointer go again
  const touchTray = (icon: TrayOf, does: 'choose' | 'menu') => {
    say(trayDoes(icon.window, icon.icon, does));
    setTrayOpen(false);
    canvas.focus();
  };
  // another program of the machine, in this session: the machine opens its window here
  const open = (asked: string) => {
    setNotStarted(null);
    setStarting(asked);
    say(start(asked));
  };
  // the keys go down in their order and come up in the other, as under a hand; then
  // the machine has the window again
  const send = (codes: string[]) => {
    const keys: ((down: boolean) => Did)[] = written
      ? codes
          .map(symOfPlace)
          .filter((sym) => sym !== null)
          .map((sym) => (down: boolean) => ({ written: { sym, down } }))
      : codes
          .map(placeOf)
          .filter((place) => place !== null)
          .map((place) => (down: boolean) => ({ key: { ...place, down } }));
    say(did([...keys.map((key) => key(true)), ...[...keys].reverse().map((key) => key(false))]));
    close();
  };
  const kept = kind === 'mac' ? KEPT_ON_A_MAC : KEPT;
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
  const shown = windows.filter((window) => window.listed);
  cameIn.current = [...cameIn.current.filter((id) => shown.some((window) => window.id === id)), ...shown.map((window) => window.id).filter((id) => !cameIn.current.includes(id))];
  const listed = cameIn.current.map((id) => shown.find((window) => window.id === id)).filter((window) => window !== undefined);
  const keyboard = layout === null ? null : layoutName(layout);
  // the controls a session has: the same on a desktop's bar and on a program's rail
  const controls = (
    <>
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
          onClick={() => {
            setClip(false);
            setLaunch(false);
            setKeys(!keys);
          }}
          autoFocus={program === null}
        >
          <Keyboard size={16} aria-hidden="true" />
        </IconButton>
        {keys && (
          <div className="sessmenu" role="menu" aria-label={t.keys} onKeyDown={inKeysMenu}>
            {kept.map((one) => (
              <button key={one.name} type="button" role="menuitem" tabIndex={-1} className="nc-actionmenu__item" onClick={() => send(one.keys)}>
                <span className="nc-actionmenu__label">{one.name}</span>
              </button>
            ))}
          </div>
        )}
      </span>
      {passes !== null && (
        <span className="menuof">
          <IconButton
            variant="ghost"
            label={t.clipboard.name}
            title={t.clipboard.name}
            className={clip ? 'is-open' : undefined}
            aria-expanded={clip}
            onClick={() => {
              setKeys(false);
              setLaunch(false);
              setClip(!clip);
            }}
          >
            <ClipboardList size={16} aria-hidden="true" />
          </IconButton>
          {clip && <ClipboardPanel machine={program ?? name} passes={passes} byItself={clipboard.byItself} fromMachine={copied} send={clipboard.send} />}
        </span>
      )}
      <IconButton variant="ghost" label={filled ? t.unfill : t.fill} title={filled ? t.unfill : t.fill} onClick={fill}>
        {filled ? <Minimize size={16} aria-hidden="true" /> : <Maximize size={16} aria-hidden="true" />}
      </IconButton>
      <IconButton variant="ghost" label={t.end} title={t.end} onClick={() => say(leaves())}>
        <Unplug size={16} aria-hidden="true" />
      </IconButton>
    </>
  );
  // on a phone the rail is swiped in from the left edge, and swiped out again
  const swipeStart = (event: PointerOfReact<HTMLElement>, capture: boolean) => {
    swipe.current = event.clientX;
    if (capture) event.currentTarget.setPointerCapture(event.pointerId);
  };
  const swipeIn = (event: PointerOfReact<HTMLElement>) => {
    if (swipe.current !== null && event.clientX - swipe.current > 24) {
      swipe.current = null;
      setBar(true);
    }
  };
  const swipeOut = (event: PointerOfReact<HTMLElement>) => {
    if (swipe.current !== null && swipe.current - event.clientX > 24) {
      swipe.current = null;
      close();
    }
  };
  return (
    <div className={`${program === null ? 'sess' : 'sess prog'}${gone ? ' is-lost' : ''}`}>
      <h1 className="unseen asis">{program ?? name}</h1>
      {gone && lost !== null && (
        <div className="lostcard" role="alert">
          <h2>{t.lost.title}</h2>
          <p>
            {lost.updating ? `${t.lost.updating} ${t.lost.comesBack(program !== null)}` : `${t.lost.tryingAgain(lost.tries)} ${t.lost.running(name, program !== null)}`}
          </p>
          <div className="acts">
            <Button onClick={leave}>{t.lost.leave}</Button>
            <Button variant="primary" onClick={tryNow}>
              {t.lost.tryNow}
            </Button>
          </div>
        </div>
      )}
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
      {program === null ? (
        // a desktop: the bar across the top, behind its handle; the desktop has its own taskbar
        bar ? (
          <div
            className="sbar"
            role="toolbar"
            aria-label={t.bar}
            onKeyDown={(event) => {
              if (event.key === 'Escape') close();
            }}
          >
            <span className="on">
              <Monitor size={16} aria-hidden="true" />
              {name}
            </span>
            <span className="grow" />
            {controls}
            <IconButton variant="ghost" label={t.closeBar} title={t.closeBar} onClick={close}>
              <X size={16} aria-hidden="true" />
            </IconButton>
          </div>
        ) : (
          <button type="button" className="handle" aria-label={t.openBar} title={t.openBar} onClick={() => setBar(true)} />
        )
      ) : railAlways || bar ? (
        // a program: the rail down the left, as a taskbar; on a phone it lies over the picture
        <>
          <div
            className={`srail${railAlways ? '' : ' over'}`}
            role="toolbar"
            aria-label={t.bar}
            onKeyDown={(event) => {
              if (event.key === 'Escape' && !railAlways) close();
            }}
            onPointerDown={railAlways ? undefined : (event) => swipeStart(event, false)}
            onPointerMove={railAlways ? undefined : swipeOut}
            onPointerUp={() => (swipe.current = null)}
          >
            {/* the top of the rail is the Away mark: the machine's programs, to open
                another in this session, and the way to the other machines */}
            <span
              className="menuof launch"
              onBlur={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget)) setLaunch(false);
              }}
              onKeyDown={(event) => {
                if (event.key === 'Escape' && launch) {
                  event.stopPropagation();
                  setLaunch(false);
                  firstControl.current?.focus();
                }
              }}
            >
              <button
                type="button"
                ref={firstControl}
                className={`it mark${launch ? ' is-open' : ''}`}
                aria-label={t.launcher}
                title={t.launcher}
                aria-expanded={launch}
                onClick={() => {
                  setKeys(false);
                  setClip(false);
                  setLaunch(!launch);
                }}
              >
                <Chevron size={14} aria-hidden="true" />
                <b className="letter">a</b>
              </button>
              {launch && (
                <div className="sesspanel launcher" role="group" aria-label={t.programsOf(name)}>
                  <h2>{t.programsOf(name)}</h2>
                  {programs.length > 0 && (
                    <ul className="progs">
                      {programs.map((one) => (
                        <li key={one.name}>
                          <button
                            type="button"
                            className="prog"
                            aria-disabled={starting !== null}
                            onClick={() => {
                              // one program at a time: the button keeps the focus, and the list stays
                              if (starting === null) open(one.name);
                            }}
                          >
                            {one.icon ? <img src={one.icon} alt="" width={20} height={20} /> : <AppWindow size={20} aria-hidden="true" />}
                            <span className="grow">{one.name}</span>
                            {starting === one.name && <Spinner size={14} label={t.starting(one.name)} />}
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                  {/* one that didn't start says so here, with the machine's own reason behind "details" */}
                  {notStarted !== null && (
                    <div className="didnt">
                      <p className="off" role="alert">
                        {t.notStarted(notStarted.program, name)}
                      </p>
                      {notStarted.details && (
                        <details className="dtl">
                          <summary>{t.details}</summary>
                          <p>{notStarted.details}</p>
                        </details>
                      )}
                    </div>
                  )}
                  <button type="button" className="prog more" onClick={otherMachines}>
                    <Monitor size={20} aria-hidden="true" />
                    <span className="grow">{t.otherMachines}</span>
                  </button>
                </div>
              )}
            </span>
            {/* the program's open windows, each as its own icon: a click brings one to the front */}
            {listed.length > 0 && (
              <ul className="wins" aria-label={t.windows}>
                {listed.map((window) => (
                  <li key={window.id}>
                    <button
                      type="button"
                      className={`it win${window.id === inFront ? ' on' : ''}`}
                      aria-current={window.id === inFront ? 'true' : undefined}
                      aria-label={t.toFront(window.title)}
                      title={t.toFront(window.title)}
                      onClick={() => {
                        // the window comes to the front and has the keys, one made small back at its
                        // size first, as a taskbar does; its icon keeps its place
                        if (window.shown === 'minimized') say(windowDoes(window.id, 'restore'));
                        say(windowDoes(window.id, 'front'));
                        canvas.focus();
                      }}
                    >
                      {icons[window.id] ? <WindowIcon picture={icons[window.id]!} /> : <AppWindow size={20} aria-hidden="true" />}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <span className="gap" />
            {controls}
            {!railAlways && (
              <IconButton variant="ghost" label={t.closeBar} title={t.closeBar} onClick={close}>
                <X size={16} aria-hidden="true" />
              </IconButton>
            )}
            {/* the foot of the rail, as a taskbar has: the keyboard the machine types as, and the clock */}
            <div className="foot">
              {/* the icons the programs put in the tray, behind a button, as Windows keeps them */}
              {tray.length > 0 && (
                <span
                  className="menuof traymenu"
                  onBlur={(event) => {
                    if (!event.currentTarget.contains(event.relatedTarget)) setTrayOpen(false);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Escape' && trayOpen) {
                      event.stopPropagation();
                      setTrayOpen(false);
                      event.currentTarget.querySelector('button')?.focus();
                    }
                  }}
                >
                  <IconButton
                    variant="ghost"
                    label={t.tray}
                    title={t.tray}
                    className={trayOpen ? 'is-open' : undefined}
                    aria-expanded={trayOpen}
                    onClick={() => {
                      setKeys(false);
                      setClip(false);
                      setLaunch(false);
                      setTrayOpen(!trayOpen);
                    }}
                  >
                    <ChevronUp size={16} aria-hidden="true" />
                  </IconButton>
                  {trayOpen && (
                    <div className="sesspanel traypanel" role="group" aria-label={t.trayOf(name)}>
                      <ul className="traygrid">
                        {tray.map((icon) => {
                          const picture = trayPictures[`${icon.window}:${icon.icon}`];
                          return (
                            <li key={`${icon.window}:${icon.icon}`}>
                              <button
                                type="button"
                                className="ticon"
                                aria-label={icon.tip.split('\n')[0] || t.trayIcon}
                                title={icon.tip || t.trayIcon}
                                onClick={() => touchTray(icon, 'choose')}
                                onContextMenu={(event) => {
                                  event.preventDefault();
                                  touchTray(icon, 'menu');
                                }}
                                onKeyDown={(event) => {
                                  if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
                                    event.preventDefault();
                                    touchTray(icon, 'menu');
                                  }
                                }}
                              >
                                {picture ? <WindowIcon picture={picture} /> : <AppWindow size={18} aria-hidden="true" />}
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    </div>
                  )}
                </span>
              )}
              {keyboard && (
                <span className="lang" title={t.typesAs(keyboard.name)}>
                  <span aria-hidden="true">{keyboard.short}</span>
                  <span className="unseen">{t.typesAs(keyboard.name)}</span>
                </span>
              )}
              <time className="clock" dateTime={clock.at} aria-label={t.clockLabel}>
                <span>{clock.time}</span>
                <span className="day">{clock.day}</span>
              </time>
            </div>
          </div>
          {/* on a phone, a touch beside the rail puts it away again */}
          {!railAlways && <div className="scrim" aria-hidden="true" onPointerDown={close} />}
        </>
      ) : (
        // a program on a phone: the rail waits at the left edge, and is swiped in
        <button
          type="button"
          className="sedge"
          aria-label={t.openBar}
          title={t.openBar}
          onClick={() => setBar(true)}
          onPointerDown={(event) => swipeStart(event, true)}
          onPointerMove={swipeIn}
          onPointerUp={() => (swipe.current = null)}
        />
      )}
      <div className="holds" ref={screen} />
    </div>
  );
}
