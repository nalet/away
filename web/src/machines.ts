// The machines the server says whoever is signed in may reach, and what it says to
// those who administer Away about what is in force.
import { useCallback, useEffect, useState } from 'react';

export interface Way {
  port: number;
  over?: 'rdp' | 'vnc';
}

export interface Machine {
  name: string;
  kind: 'windows' | 'linux' | 'mac';
  address: string;
  groups: string[];
  desktop: Way | null;
  programs: string[];
  shell: Way | null;
  collection: string | null;
}

// A machine as the files say it, for those who administer.
export interface Written extends Machine {
  file: string;
  line: number;
  given_by: { rule: string; who: string[] }[];
  served_by: string[];
}

export interface Rule {
  name: string;
  who: string[];
  reaches: string[];
  file: string;
  line: number;
}

export interface InForce {
  has_files: boolean;
  since: number;
  machines: Written[];
  rules: Rule[];
  refused: { at: number; file: string; line: number; why: string } | null;
}

async function ask<Answer>(address: string): Promise<Answer> {
  const answer = await fetch(address, { headers: { accept: 'application/json' }, cache: 'no-store' });
  if (!answer.ok) throw new Error(`the server answered ${answer.status}`);
  return (await answer.json()) as Answer;
}

// What was asked of the server: still being asked, not answered, or the answer.
export type Asked<Answer> = 'asking' | 'unanswered' | Answer;

// Asks when the page opens, and again when its window is looked at again.
function useAsked<Answer>(address: string): [Asked<Answer>, () => void] {
  const [answer, setAnswer] = useState<Asked<Answer>>('asking');
  const again = useCallback(() => {
    ask<Answer>(address).then(setAnswer, () => setAnswer((before) => (typeof before === 'string' ? 'unanswered' : before)));
  }, [address]);
  useEffect(() => {
    again();
    window.addEventListener('focus', again);
    return () => window.removeEventListener('focus', again);
  }, [again]);
  return [answer, again];
}

export function useMachines(): [Asked<Machine[]>, () => void] {
  const [answer, again] = useAsked<{ machines: Machine[] }>('/api/machines');
  return [typeof answer === 'string' ? answer : answer.machines, again];
}

export function useInForce(): [Asked<InForce>, () => void] {
  return useAsked<InForce>('/api/register');
}

// Where a machine's own page is.
export function pageOf(name: string): string {
  return `/machines/${encodeURIComponent(name)}`;
}

// The machine a page's address names, where it names one.
export function namedBy(path: string): string | null {
  const name = /^\/machines\/([^/]+)$/.exec(path)?.[1];
  if (!name) return null;
  try {
    return decodeURIComponent(name);
  } catch {
    return null;
  }
}

// Whether a machine has what is looked for in its name, its address or one of its groups.
export function matches(machine: Machine, sought: string): boolean {
  const words = sought.trim().toLowerCase();
  return [machine.name, machine.address, ...machine.groups].some((text) => text.toLowerCase().includes(words));
}

// What people open: a machine, by its desktop or its shell, or one program of a machine.
export interface Entry {
  machine: Machine;
  program: string | null;
}

// Everything one may open on these machines, each as an entry of its own: the machines
// that have a desktop or a shell, and then every program. Both keep the order the
// administrator's files have them in. A machine that offers programs alone is seen as
// its programs.
export function entriesOf(machines: Machine[]): Entry[] {
  const whole = machines.filter((machine) => machine.desktop || machine.shell).map((machine) => ({ machine, program: null }));
  const programs = machines.flatMap((machine) => machine.programs.map((program) => ({ machine, program })));
  return [...whole, ...programs];
}

// Whether an entry is what is looked for: a program by its name, and anything by its
// machine's name, address or groups.
export function found(entry: Entry, sought: string): boolean {
  const words = sought.trim().toLowerCase();
  return (entry.program?.toLowerCase().includes(words) ?? false) || matches(entry.machine, sought);
}
