// The clipboard of a session: text that is copied on one side can be pasted on the
// other, each way where the installation lets it.
//
// A browser that lets a page read its clipboard passes it on by itself: what the person
// copied is told to the machine when the machine's screen gets the focus, and what is
// copied on the machine is written to the person's clipboard. A browser that doesn't is
// given a panel, with a field for text to the machine and one for text from it.

import { Button, Field, Textarea } from '@nalet/design-system';
import { useCallback, useEffect, useRef, useState } from 'react';

import { texts } from '../texts';
import { copied as copiedToMachine } from './wire';

// Which ways the clipboard passes, as the gateway says.
export interface Passes {
  to: boolean;
  from: boolean;
}

// The session's clipboard: whether this browser passes it on by itself, and what the
// panel sends and shows where it doesn't.
export function useClipboard(
  canvas: HTMLCanvasElement,
  passes: Passes | null,
  fromMachine: string | null,
  say: (message: string) => void,
): { byItself: boolean; send: (text: string) => void } {
  // a browser passes its clipboard on once it lets the page read it, which it says
  // itself where it can be asked; a read that it refuses says the opposite
  const [byItself, setByItself] = useState(false);
  // what was last told to the machine and what last came from it: neither is told again
  const told = useRef<string | null>(null);
  const came = useRef<string | null>(null);

  useEffect(() => {
    let over = false;
    const asked = navigator.permissions?.query?.({ name: 'clipboard-read' as PermissionName });
    if (!navigator.clipboard?.readText || !asked) return undefined;
    asked
      .then((status) => {
        const said = () => {
          if (!over) setByItself(status.state !== 'denied');
        };
        said();
        status.onchange = said;
      })
      .catch(() => undefined);
    return () => {
      over = true;
    };
  }, []);

  const send = useCallback(
    (text: string) => {
      if (!passes?.to || text === '' || text === told.current) return;
      told.current = text;
      say(copiedToMachine(text));
    },
    [passes, say],
  );

  // what the person copied is told to the machine when its screen gets the focus, and
  // when the window comes back to the front
  useEffect(() => {
    if (!passes?.to || !byItself) return undefined;
    const read = () => {
      if (!document.hasFocus()) return;
      navigator.clipboard
        .readText()
        .then((text) => {
          if (text !== came.current) send(text);
        })
        .catch((error: unknown) => {
          // the person said no, or the browser asks each time: the panel is the way
          if (error instanceof DOMException && error.name === 'NotAllowedError') setByItself(false);
        });
    };
    read();
    canvas.addEventListener('focus', read);
    window.addEventListener('focus', read);
    return () => {
      canvas.removeEventListener('focus', read);
      window.removeEventListener('focus', read);
    };
  }, [canvas, passes, byItself, send]);

  // what was copied on the machine goes to the person's clipboard
  useEffect(() => {
    if (!passes?.from || fromMachine === null || fromMachine === came.current) return;
    came.current = fromMachine;
    if (byItself) navigator.clipboard.writeText(fromMachine).catch(() => undefined);
  }, [passes, fromMachine, byItself]);

  return { byItself, send };
}

// The panel under the bar's clipboard button.
export function ClipboardPanel({
  machine,
  passes,
  byItself,
  fromMachine,
  send,
}: {
  machine: string;
  passes: Passes;
  byItself: boolean;
  fromMachine: string | null;
  send: (text: string) => void;
}) {
  const t = texts.session.clipboard;
  const [text, setText] = useState('');
  const [taken, setTaken] = useState(false);
  const off = (
    <>
      {!passes.to && <p className="off">{t.noPasting(machine)}</p>}
      {!passes.from && <p className="off">{t.noCopying(machine)}</p>}
    </>
  );
  if (byItself) {
    return (
      <div className="sesspanel" role="group" aria-label={t.name}>
        {(passes.to || passes.from) && <p>{passes.to && passes.from ? t.passes : passes.to ? t.passesTo(machine) : t.passesFrom(machine)}</p>}
        {off}
      </div>
    );
  }
  return (
    <div className="sesspanel" role="group" aria-label={t.name}>
      {(passes.to || passes.from) && (
        <p>
          {t.notByItself}
          {passes.to ? ` ${t.pasteHere(machine)}` : ''}
        </p>
      )}
      {passes.to && (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            send(text);
          }}
        >
          <Field label={t.toMachine}>
            <Textarea rows={3} value={text} onChange={(event) => setText(event.target.value)} />
          </Field>
          <Button type="submit" variant="primary" size="sm" disabled={text === ''}>
            {t.send}
          </Button>
        </form>
      )}
      {passes.from && (
        <div>
          <Field label={t.fromMachine}>
            <Textarea rows={3} readOnly value={fromMachine ?? ''} />
          </Field>
          <Button
            size="sm"
            disabled={fromMachine === null}
            onClick={() => {
              if (fromMachine !== null) {
                navigator.clipboard
                  ?.writeText(fromMachine)
                  .then(() => setTaken(true))
                  .catch(() => undefined);
              }
            }}
          >
            {t.copy}
          </Button>
          <span className="said" role="status">
            {taken ? t.copiedHere : ''}
          </span>
        </div>
      )}
      {off}
    </div>
  );
}
