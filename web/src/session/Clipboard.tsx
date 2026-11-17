// The clipboard of a session: text that is copied on one side can be pasted on the
// other, and a picture too where the machine carries pictures, each way where the
// installation lets it.
//
// A browser that lets a page read its clipboard passes it on by itself: what the person
// copied is told to the machine when the machine's screen gets the focus, and what is
// copied on the machine is written to the person's clipboard. A browser that doesn't is
// given a panel, with a field for what goes to the machine and one for what came from it.

import { Button, Field, Textarea } from '@nalet/design-system';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ClipboardEvent } from 'react';

import { texts } from '../texts';
import { copied as copiedToMachine, copiedPicture as pictureToMachine } from './wire';

// Which ways the clipboard passes, as the gateway says, and whether pictures pass with
// the text.
export interface Passes {
  to: boolean;
  from: boolean;
  pictures?: boolean;
}

// What a clipboard holds for the other side: text, or a picture as a PNG picture.
export type Held = { text: string } | { picture: Uint8Array<ArrayBuffer> };

// The largest picture that is handed to the gateway, as the PNG picture it is.
const LARGEST = 16 * 1024 * 1024;

// A picture by what it shows: its size and a sum over its pixels. A browser writes a
// picture anew when it takes one into its clipboard, so the same picture is not the
// same bytes twice.
async function shows(png: Uint8Array<ArrayBuffer>): Promise<string | null> {
  try {
    const picture = await createImageBitmap(new Blob([png], { type: 'image/png' }));
    const drawn = new OffscreenCanvas(picture.width, picture.height).getContext('2d');
    if (!drawn) return null;
    drawn.drawImage(picture, 0, 0);
    const pixels = new Uint32Array(drawn.getImageData(0, 0, picture.width, picture.height).data.buffer);
    picture.close();
    let sum = 0x811c9dc5;
    for (const pixel of pixels) sum = Math.imul(sum ^ pixel, 0x01000193);
    return `${picture.width} by ${picture.height}: ${sum >>> 0}`;
  } catch {
    return null;
  }
}

// A picture of any kind a browser shows, as a PNG picture.
async function asPng(file: Blob): Promise<Uint8Array<ArrayBuffer> | null> {
  try {
    if (file.type === 'image/png') return new Uint8Array(await file.arrayBuffer());
    const picture = await createImageBitmap(file);
    const canvas = new OffscreenCanvas(picture.width, picture.height);
    canvas.getContext('2d')?.drawImage(picture, 0, 0);
    picture.close();
    return new Uint8Array(await (await canvas.convertToBlob({ type: 'image/png' })).arrayBuffer());
  } catch {
    return null;
  }
}

// The session's clipboard: whether this browser passes it on by itself, and what the
// panel sends where it doesn't.
export function useClipboard(
  canvas: HTMLCanvasElement,
  passes: Passes | null,
  fromMachine: Held | null,
  say: (message: string | Uint8Array<ArrayBuffer>) => void,
): { byItself: boolean; send: (text: string) => void; sendPicture: (picture: Blob) => Promise<'sent' | 'too large' | 'no picture'> } {
  // a browser passes its clipboard on once it lets the page read it, which it says
  // itself where it can be asked; a read that it refuses says the opposite
  const [byItself, setByItself] = useState(false);
  // what was last told to the machine and what last came from it: neither is told again.
  // Text is known by itself, and a picture by what it shows
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
      if (!passes?.to || text === '' || `text ${text}` === told.current) return;
      told.current = `text ${text}`;
      say(copiedToMachine(text));
    },
    [passes, say],
  );

  const sendPicture = useCallback(
    async (picture: Blob) => {
      if (!passes?.to || !passes.pictures) return 'no picture';
      const png = await asPng(picture);
      const what = png && (await shows(png));
      if (!png || !what) return 'no picture';
      if (png.byteLength > LARGEST) return 'too large';
      // the picture the machine itself copied is not handed back to it
      if (what !== told.current && what !== came.current) {
        told.current = what;
        for (const piece of pictureToMachine(png)) say(piece);
      }
      return 'sent';
    },
    [passes, say],
  );

  // what the person copied is told to the machine when its screen gets the focus, and
  // when the window comes back to the front: its text, or its picture where it holds
  // no text
  useEffect(() => {
    if (!passes?.to || !byItself) return undefined;
    const refused = (error: unknown) => {
      // the person said no, or the browser asks each time: the panel is the way
      if (error instanceof DOMException && error.name === 'NotAllowedError') setByItself(false);
    };
    const read = () => {
      if (!document.hasFocus()) return;
      if (!passes.pictures || !navigator.clipboard.read) {
        navigator.clipboard
          .readText()
          .then((text) => {
            if (`text ${text}` !== came.current) send(text);
          })
          .catch(refused);
        return;
      }
      navigator.clipboard
        .read()
        .then(async (items) => {
          const text = items.find((item) => item.types.includes('text/plain'));
          const picture = items.find((item) => item.types.includes('image/png'));
          if (text) {
            const held = await (await text.getType('text/plain')).text();
            if (`text ${held}` !== came.current) send(held);
          } else if (picture) await sendPicture(await picture.getType('image/png'));
        })
        .catch(refused);
    };
    read();
    canvas.addEventListener('focus', read);
    window.addEventListener('focus', read);
    return () => {
      canvas.removeEventListener('focus', read);
      window.removeEventListener('focus', read);
    };
  }, [canvas, passes, byItself, send, sendPicture]);

  // what was copied on the machine goes to the person's clipboard
  useEffect(() => {
    if (!passes?.from || fromMachine === null) return;
    if ('text' in fromMachine) {
      if (`text ${fromMachine.text}` === came.current) return;
      came.current = `text ${fromMachine.text}`;
      if (byItself) navigator.clipboard.writeText(fromMachine.text).catch(() => undefined);
      return;
    }
    const png = fromMachine.picture;
    shows(png)
      .then((what) => {
        if (what === null || what === came.current) return;
        came.current = what;
        if (byItself) navigator.clipboard.write([new ClipboardItem({ 'image/png': new Blob([png], { type: 'image/png' }) })]).catch(() => undefined);
      })
      .catch(() => undefined);
  }, [passes, fromMachine, byItself]);

  return { byItself, send, sendPicture };
}

// A picture that came from the machine, drawn for the person to see what it is.
function PictureFromMachine({ png, label }: { png: Uint8Array<ArrayBuffer>; label: (width: number, height: number) => string }) {
  const drawn = useRef<HTMLCanvasElement | null>(null);
  const [size, setSize] = useState<[number, number] | null>(null);
  useEffect(() => {
    let over = false;
    createImageBitmap(new Blob([png], { type: 'image/png' }))
      .then((picture) => {
        const canvas = drawn.current;
        if (over || !canvas) return;
        canvas.width = picture.width;
        canvas.height = picture.height;
        canvas.getContext('2d')?.drawImage(picture, 0, 0);
        setSize([picture.width, picture.height]);
        picture.close();
      })
      .catch(() => undefined);
    return () => {
      over = true;
    };
  }, [png]);
  return <canvas ref={drawn} className="copied" role="img" aria-label={size ? label(size[0], size[1]) : undefined} />;
}

// The panel under the bar's clipboard button.
export function ClipboardPanel({
  machine,
  passes,
  byItself,
  fromMachine,
  send,
  sendPicture,
  screen,
}: {
  machine: string;
  passes: Passes;
  byItself: boolean;
  fromMachine: Held | null;
  send: (text: string) => void;
  sendPicture: (picture: Blob) => Promise<'sent' | 'too large' | 'no picture'>;
  screen: HTMLCanvasElement;
}) {
  const t = texts.session.clipboard;
  const pictures = passes.pictures === true;
  const [text, setText] = useState('');
  const [taken, setTaken] = useState(false);
  // a picture of what the session shows, taken by Away itself: the machine has no part
  // in it, so it is there for every kind of machine, where copying from it is let
  const [shot, setShot] = useState(false);
  const takesAShot = passes.from && typeof ClipboardItem !== 'undefined' && (
    <div>
      <Button
        size="sm"
        onClick={() => {
          setShot(false);
          screen.toBlob((picture) => {
            if (!picture) return;
            navigator.clipboard
              ?.write([new ClipboardItem({ 'image/png': picture })])
              .then(() => setShot(true))
              .catch(() => undefined);
          }, 'image/png');
        }}
      >
        {t.shot}
      </Button>
      <span className="said" role="status">
        {shot ? t.shotTaken : ''}
      </span>
    </div>
  );
  // what became of a picture that was pasted into the field
  const [sent, setSent] = useState<'sent' | 'too large' | null>(null);
  useEffect(() => setTaken(false), [fromMachine]);
  const off = (
    <>
      {!passes.to && <p className="off">{t.noPasting(machine)}</p>}
      {!passes.from && <p className="off">{t.noCopying(machine)}</p>}
    </>
  );
  if (byItself) {
    return (
      <div className="sesspanel" role="group" aria-label={t.name}>
        {(passes.to || passes.from) && (
          <p>{passes.to && passes.from ? t.passes(pictures) : passes.to ? t.passesTo(machine, pictures) : t.passesFrom(machine, pictures)}</p>
        )}
        {takesAShot}
        {off}
      </div>
    );
  }
  // a picture that is pasted into the field goes to the machine at once
  const pasted = (event: ClipboardEvent<HTMLElement>) => {
    const picture = pictures ? Array.from(event.clipboardData.files).find((file) => file.type.startsWith('image/')) : undefined;
    if (!picture) return;
    event.preventDefault();
    setSent(null);
    sendPicture(picture)
      .then((how) => setSent(how === 'no picture' ? null : how))
      .catch(() => undefined);
  };
  const copy = () => {
    if (fromMachine === null) return;
    const written =
      'text' in fromMachine
        ? navigator.clipboard?.writeText(fromMachine.text)
        : navigator.clipboard?.write([new ClipboardItem({ 'image/png': new Blob([fromMachine.picture], { type: 'image/png' }) })]);
    written?.then(() => setTaken(true)).catch(() => undefined);
  };
  return (
    <div className="sesspanel" role="group" aria-label={t.name}>
      {(passes.to || passes.from) && (
        <p>
          {t.notByItself}
          {passes.to ? ` ${t.pasteHere(machine, pictures)}` : ''}
        </p>
      )}
      {passes.to && (
        <form
          onPaste={pasted}
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
          <span className="said" role="status">
            {sent === 'sent' ? t.pictureSent(machine) : sent === 'too large' ? t.tooLarge : ''}
          </span>
        </form>
      )}
      {passes.from && (
        <div>
          {fromMachine !== null && 'picture' in fromMachine ? (
            <div className="held">
              <span className="nc-field__label">{t.fromMachine}</span>
              <PictureFromMachine png={fromMachine.picture} label={t.pictureOf} />
            </div>
          ) : (
            <Field label={t.fromMachine}>
              <Textarea rows={3} readOnly value={fromMachine?.text ?? ''} />
            </Field>
          )}
          <Button size="sm" disabled={fromMachine === null} onClick={copy}>
            {t.copy}
          </Button>
          <span className="said" role="status">
            {taken ? t.copiedHere : ''}
          </span>
        </div>
      )}
      {takesAShot}
      {off}
    </div>
  );
}
