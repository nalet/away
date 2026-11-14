// What the server says of whoever asks: whether they are signed in, who they are, and
// what the installation is told. The sign-in itself stays in the server: the page holds
// no token, and the browser's cookie is one no script can read.

export interface Who {
  set_up: boolean;
  signed_in: boolean;
  ran_out?: boolean;
  name?: string;
  address?: string | null;
  initials?: string;
  administers?: boolean;
  reads_records?: boolean;
  failed?: { why: 'no-answer' | 'unusable'; details: string } | null;
  contact: string | null;
  version: string;
  commit: string;
  source: string;
}

export async function askWho(): Promise<Who> {
  const answer = await fetch('/api/who', { headers: { accept: 'application/json' }, cache: 'no-store' });
  if (!answer.ok) throw new Error(`the server answered ${answer.status}`);
  return (await answer.json()) as Who;
}

// The header says that one of Away's own pages asks: no other site can add it.
export async function signOut(): Promise<void> {
  const answer = await fetch('/sign-out', { method: 'POST', headers: { 'x-away': '1' } });
  if (!answer.ok) throw new Error(`the server answered ${answer.status}`);
}

// Where the browser goes to sign in, and comes back to afterwards.
export function signInAt(to: string): string {
  return `/sign-in?to=${encodeURIComponent(to)}`;
}
