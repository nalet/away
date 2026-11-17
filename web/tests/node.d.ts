// The two things of Node that a test of the pages uses, to change a file of machines
// while the server runs, and to read a file that a page downloaded. They are declared
// here so that the pages need no library that only describes Node.
declare module 'node:fs' {
  export function readFileSync(path: string, encoding: 'utf8'): string;
  export function readFileSync(path: string): Uint8Array;
  export function writeFileSync(path: string, text: string): void;
}
