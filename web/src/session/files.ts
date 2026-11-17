// The files that were copied on a machine, on their way to the person's device: what is
// in one comes in pieces, one file at a time, and several are put into one ZIP file, as
// they are, with the folders they lie in.

// A file as the gateway lists it: its name after the folders it lies in, each followed
// by a slash, its size, or that it is a folder.
export type FileOf = { name: string; size?: number; folder?: boolean };

// The largest file, and the most of several in one ZIP file, that pass through the
// browser, which holds them whole until they are saved.
export const LARGEST_FILE = 1024 * 1024 * 1024;

// A size as people read one.
export function sizeOf(bytes: number): string {
  if (bytes < 1000) return `${bytes} bytes`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let size = bytes / 1000;
  let unit = 0;
  while (size >= 1000 && unit < units.length - 1) {
    size /= 1000;
    unit += 1;
  }
  return `${size >= 100 ? Math.round(size) : size.toFixed(1)} ${units[unit]}`;
}

// A file's own name, without the folders it lies in.
export function nameOf(file: FileOf): string {
  return file.name.split('/').pop() ?? file.name;
}

// The sum a ZIP file keeps of each file, to tell whether it is whole.
const SUMS = Array.from({ length: 256 }, (_, nth) => {
  let sum = nth;
  for (let bit = 0; bit < 8; bit += 1) sum = sum & 1 ? (sum >>> 1) ^ 0xedb88320 : sum >>> 1;
  return sum >>> 0;
});

export function summed(sum: number, bytes: Uint8Array): number {
  let now = sum ^ 0xffffffff;
  for (const byte of bytes) now = (SUMS[(now ^ byte) & 0xff] ?? 0) ^ (now >>> 8);
  return (now ^ 0xffffffff) >>> 0;
}

// A ZIP file that is written as its files come: each as it is, not packed, under its
// name with its folders.
export class Zip {
  private parts: BlobPart[] = [];
  private listed: Uint8Array<ArrayBuffer>[] = [];
  private at = 0;
  private count = 0;
  private readonly when: [number, number];

  constructor(now = new Date()) {
    // the time and the day as such a file writes them: two seconds to a step, and years from 1980
    this.when = [
      (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1),
      ((Math.max(1980, now.getFullYear()) - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate(),
    ];
  }

  // Adds a file with what is in it, or a folder, which holds nothing.
  add(file: FileOf, pieces: Uint8Array<ArrayBuffer>[]): void {
    const name = new TextEncoder().encode(file.folder ? `${file.name}/` : file.name);
    const size = pieces.reduce((sum, piece) => sum + piece.byteLength, 0);
    const sum = pieces.reduce((before, piece) => summed(before, piece), 0);
    // what both the head before the file and its line in the list at the end say of it
    const said = (bytes: DataView, from: number) => {
      bytes.setUint16(from, 20, true);
      // names are written in the characters of every alphabet
      bytes.setUint16(from + 2, 0x0800, true);
      bytes.setUint16(from + 4, 0, true);
      bytes.setUint16(from + 6, this.when[0], true);
      bytes.setUint16(from + 8, this.when[1], true);
      bytes.setUint32(from + 10, sum, true);
      bytes.setUint32(from + 14, size, true);
      bytes.setUint32(from + 18, size, true);
      bytes.setUint16(from + 22, name.byteLength, true);
    };
    const head = new Uint8Array(30 + name.byteLength);
    const before = new DataView(head.buffer);
    before.setUint32(0, 0x04034b50, true);
    said(before, 4);
    head.set(name, 30);
    const line = new Uint8Array(46 + name.byteLength);
    const listed = new DataView(line.buffer);
    listed.setUint32(0, 0x02014b50, true);
    listed.setUint16(4, 20, true);
    said(listed, 6);
    listed.setUint32(38, file.folder ? 0x10 : 0, true);
    listed.setUint32(42, this.at, true);
    line.set(name, 46);
    this.parts.push(head, ...pieces);
    this.listed.push(line);
    this.at += head.byteLength + size;
    this.count += 1;
  }

  // The ZIP file, whole: the files, the list of them, and where the list is.
  whole(): Blob {
    const list = this.listed.reduce((sum, line) => sum + line.byteLength, 0);
    const end = new Uint8Array(22);
    const says = new DataView(end.buffer);
    says.setUint32(0, 0x06054b50, true);
    says.setUint16(8, this.count, true);
    says.setUint16(10, this.count, true);
    says.setUint32(12, list, true);
    says.setUint32(16, this.at, true);
    return new Blob([...this.parts, ...this.listed, end], { type: 'application/zip' });
  }
}

// Hands a file to the browser to save, under a name: it goes where the browser keeps
// what is downloaded.
export function save(file: Blob, name: string): void {
  const at = URL.createObjectURL(file);
  const link = document.createElement('a');
  link.href = at;
  link.download = name;
  link.hidden = true;
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(at), 60_000);
}
