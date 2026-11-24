// The passwords Away keeps for machines, as those who administer it see them: by their
// names, with the machines that name them, and whether one is kept. A password never
// comes back from the server. The page seals it to the issuer's key before it is sent,
// so that the server and the database hold only what the issuer alone opens.
import { useAsked } from './machines';
import type { Asked } from './machines';

export interface KeptSecret {
  name: string;
  // the machines whose entry names it
  machines: string[];
  kept: boolean;
  // since when it is kept as it is, in seconds since 1970, and by whom
  since: number | null;
  by: string | null;
}

export interface KeptSecrets {
  // whether Away keeps passwords here at all: it needs a database and the issuer's key
  keeps: boolean;
  // the issuer's key a password is sealed to: its name, and its public half in Base64
  key: { name: string; public: string } | null;
  secrets: KeptSecret[];
}

export function useKept(): [Asked<KeptSecrets>, () => void] {
  return useAsked<KeptSecrets>('/api/kept');
}

function fromBase64(letters: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(letters), (letter) => letter.charCodeAt(0));
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

// A password sealed as the issuer opens it: a key made for it alone, encrypted to the
// issuer's key (RSA-OAEP with SHA-256), and the password encrypted with that key
// (AES-256-GCM), bound to the name it is kept under, so that it opens under no other.
export async function sealed(key: { name: string; public: string }, name: string, password: string): Promise<Uint8Array<ArrayBuffer>> {
  const publicHalf = fromBase64(key.public);
  // the key is the one the server named: its name is made from its public half
  const named = new Uint8Array(await crypto.subtle.digest('SHA-256', publicHalf)).slice(0, 16);
  if (hex(named) !== key.name) throw new Error('the key is not the one it is said to be');
  const issuer = await crypto.subtle.importKey('spki', publicHalf, { name: 'RSA-OAEP', hash: 'SHA-256' }, false, ['encrypt']);
  const own = crypto.getRandomValues(new Uint8Array(32));
  const once = crypto.getRandomValues(new Uint8Array(12));
  const sealing = await crypto.subtle.importKey('raw', own, 'AES-GCM', false, ['encrypt']);
  const bound = new TextEncoder().encode(`away kept secret\n${name}`);
  const secret = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv: once, additionalData: bound }, sealing, new TextEncoder().encode(password)));
  const wrapped = new Uint8Array(await crypto.subtle.encrypt({ name: 'RSA-OAEP' }, issuer, own));
  own.fill(0);
  const whole = new Uint8Array(4 + 16 + 2 + wrapped.length + 12 + secret.length);
  whole.set(new TextEncoder().encode('AKS1'), 0);
  whole.set(named, 4);
  whole.set([wrapped.length >> 8, wrapped.length & 0xff], 20);
  whole.set(wrapped, 22);
  whole.set(once, 22 + wrapped.length);
  whole.set(secret, 34 + wrapped.length);
  return whole;
}

// Keeps a password for the machines that name it, in place of one kept before. Gives the
// server's status: 409 where the issuer's key changed meanwhile.
export async function keep(key: { name: string; public: string }, name: string, password: string): Promise<number> {
  const body = await sealed(key, name, password);
  const answer = await fetch(`/api/kept?${new URLSearchParams({ name })}`, {
    method: 'PUT',
    headers: { 'x-away': '1', 'content-type': 'application/octet-stream' },
    body,
  });
  return answer.status;
}

// Keeps the password of that name no more.
export async function forget(name: string): Promise<number> {
  const answer = await fetch(`/api/kept?${new URLSearchParams({ name })}`, { method: 'DELETE', headers: { 'x-away': '1' } });
  return answer.status;
}
