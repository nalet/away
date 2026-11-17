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
import { LARGEST_FILE, Zip, nameOf, save, sizeOf } from './files';
import type { FileOf } from './files';
import { copied as copiedToMachine, copiedPicture as pictureToMachine } from './wire';

// Which ways the clipboard passes, as the gateway says, and whether pictures pass with
// the text.
export interface Passes {
  to: boolean;
  from: boolean;
  pictures?: boolean;
}

// What a clipboard holds for the other side: text, a picture as a PNG picture, or the
// files that were copied on the machine, of which the person has the ones they choose.
export type Held = { text: string } | { picture: Uint8Array<ArrayBuffer> } | { files: FileOf[] };

// Who takes a file that is had from the machine: its pieces in their order, and then
// whether all of it came.
export type Takes = { piece: (piece: Uint8Array<ArrayBuffer>) => void; ended: (whole: boolean) => void };

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
    // files reach no clipboard a browser has: the panel lists them, to download
    if ('files' in fromMachine) return;
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

// The files that were copied on the machine: each to download, and all of them as one
// ZIP file. One is had at a time, in pieces, and the browser saves it once it is whole.
function FilesFromMachine({
  machine,
  files,
  have,
  giveUp,
}: {
  machine: string;
  files: FileOf[];
  have: (at: number, takes: Takes) => void;
  giveUp: () => void;
}) {
  const t = texts.session.clipboard;
  // the file that is being had, how much of it came, and what became of the last one
  const [having, setHaving] = useState<{ name: string; came: number; of: number } | null>(null);
  const [said, setSaid] = useState('');
  useEffect(() => {
    setHaving(null);
    setSaid('');
  }, [files]);
  // a file, whole, as its pieces: nothing where the machine gave no more of it
  const had = (at: number, file: FileOf, before: number, of: number) =>
    new Promise<Uint8Array<ArrayBuffer>[] | null>((whole) => {
      const pieces: Uint8Array<ArrayBuffer>[] = [];
      let came = 0;
      have(at, {
        piece: (piece) => {
          pieces.push(piece);
          came += piece.byteLength;
          setHaving({ name: nameOf(file), came: before + came, of });
        },
        ended: (all) => whole(all ? pieces : null),
      });
    });
  const one = async (at: number, file: FileOf) => {
    setSaid('');
    setHaving({ name: nameOf(file), came: 0, of: file.size ?? 0 });
    const pieces = await had(at, file, 0, file.size ?? 0);
    setHaving(null);
    if (pieces === null) setSaid(t.noMore(machine, nameOf(file)));
    else {
      save(new Blob(pieces), nameOf(file));
      setSaid(t.saved(nameOf(file)));
    }
  };
  const total = files.reduce((sum, file) => sum + (file.size ?? 0), 0);
  const all = async () => {
    setSaid('');
    const zip = new Zip();
    let came = 0;
    for (const [at, file] of files.entries()) {
      if (file.folder) {
        zip.add(file, []);
        continue;
      }
      setHaving({ name: nameOf(file), came, of: total });
      const pieces = await had(at, file, came, total);
      if (pieces === null) {
        setHaving(null);
        setSaid(t.noMore(machine, nameOf(file)));
        return;
      }
      came += pieces.reduce((sum, piece) => sum + piece.byteLength, 0);
      zip.add(file, pieces);
    }
    setHaving(null);
    const name = t.zipName(machine);
    save(zip.whole(), name);
    setSaid(t.saved(name));
  };
  const busy = having !== null;
  return (
    <div className="held">
      <span className="nc-field__label" id="files-from-the-machine">
        {t.fromMachine}
      </span>
      <ul className="files" aria-labelledby="files-from-the-machine">
        {files.map((file, at) => (
          <li key={file.name}>
            <span className="grow">{file.folder ? `${file.name}/` : file.name}</span>
            {!file.folder && <span className="size">{sizeOf(file.size ?? 0)}</span>}
            {!file.folder && (
              <Button size="sm" disabled={busy || (file.size ?? 0) > LARGEST_FILE} aria-label={t.download(nameOf(file))} onClick={() => void one(at, file)}>
                {t.downloadShort}
              </Button>
            )}
          </li>
        ))}
      </ul>
      {files.length > 1 && (
        <Button size="sm" disabled={busy || total > LARGEST_FILE} onClick={() => void all()}>
          {t.downloadAll}
        </Button>
      )}
      {(total > LARGEST_FILE || files.some((file) => (file.size ?? 0) > LARGEST_FILE)) && <p className="off">{t.tooLargeFile}</p>}
      {busy && (
        <Button size="sm" onClick={giveUp}>
          {t.stop}
        </Button>
      )}
      <span className="said" role="status">
        {having ? t.having(having.name, having.of > 0 ? Math.min(100, Math.floor((having.came * 100) / having.of)) : null) : said}
      </span>
    </div>
  );
}

// The files the person gives for the machine to paste: chosen with a button, pasted
// into the panel, or dropped on the session. What is in them stays on the person's
// device until the machine pastes them.
function FilesToMachine({ machine, gave, give }: { machine: string; gave: { name: string; size: number }[]; give: (files: File[]) => void }) {
  const t = texts.session.clipboard;
  const chosen = useRef<HTMLInputElement | null>(null);
  return (
    <div className="held">
      <input
        ref={chosen}
        type="file"
        multiple
        hidden
        onChange={(event) => {
          give(Array.from(event.target.files ?? []));
          event.target.value = '';
        }}
      />
      <Button size="sm" onClick={() => chosen.current?.click()}>
        {t.addFiles}
      </Button>
      {gave.length > 0 && (
        <>
          <p role="status">{t.filesGiven(gave.length, machine)}</p>
          <ul className="files" aria-label={t.filesFor(machine)}>
            {gave.map((file, at) => (
              <li key={`${at} ${file.name}`}>
                <span className="grow">{file.name}</span>
                <span className="size">{sizeOf(file.size)}</span>
              </li>
            ))}
          </ul>
          <Button size="sm" onClick={() => give([])}>
            {t.takeBack}
          </Button>
        </>
      )}
    </div>
  );
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
  have,
  giveUp,
  gave,
  give,
}: {
  machine: string;
  passes: Passes;
  byItself: boolean;
  fromMachine: Held | null;
  send: (text: string) => void;
  sendPicture: (picture: Blob) => Promise<'sent' | 'too large' | 'no picture'>;
  screen: HTMLCanvasElement;
  have: (at: number, takes: Takes) => void;
  giveUp: () => void;
  gave: { name: string; size: number }[];
  give: (files: File[]) => void;
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
  // the files that were copied on the machine are listed whatever the browser does
  // with its clipboard: no browser's clipboard takes a file from a page
  const files = passes.from && fromMachine !== null && 'files' in fromMachine && (
    <FilesFromMachine machine={machine} files={fromMachine.files} have={have} giveUp={giveUp} />
  );
  // and the person's own files are given whatever the browser does with it: none
  // lets a page read a file off it either, except as it is pasted
  const theirs = passes.to && pictures && <FilesToMachine machine={machine} gave={gave} give={give} />;
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
        {theirs}
        {files}
        {takesAShot}
        {off}
      </div>
    );
  }
  // a picture that is pasted into the field goes to the machine at once
  const pasted = (event: ClipboardEvent<HTMLElement>) => {
    const carried = pictures ? Array.from(event.clipboardData.files) : [];
    // one picture goes as a picture; anything else that is files goes as files
    const picture = carried.length === 1 ? carried.find((file) => file.type.startsWith('image/')) : undefined;
    if (!picture && carried.length > 0) {
      event.preventDefault();
      give(carried);
      return;
    }
    if (!picture) return;
    event.preventDefault();
    setSent(null);
    sendPicture(picture)
      .then((how) => setSent(how === 'no picture' ? null : how))
      .catch(() => undefined);
  };
  const copy = () => {
    if (fromMachine === null || 'files' in fromMachine) return;
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
      {theirs}
      {files}
      {passes.from && !files && (
        <div>
          {fromMachine !== null && 'picture' in fromMachine ? (
            <div className="held">
              <span className="nc-field__label">{t.fromMachine}</span>
              <PictureFromMachine png={fromMachine.picture} label={t.pictureOf} />
            </div>
          ) : (
            <Field label={t.fromMachine}>
              <Textarea rows={3} readOnly value={fromMachine !== null && 'text' in fromMachine ? fromMachine.text : ''} />
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
