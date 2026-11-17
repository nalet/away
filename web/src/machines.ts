// The machines the server says whoever is signed in may reach, and what it says to
// those who administer Away about what is in force.
import { useCallback, useEffect, useState } from 'react';

export interface Way {
  port: number;
  over?: 'rdp' | 'vnc';
}

// A program of a Windows machine: its name, and where its icon is, where it has one.
export interface Program {
  name: string;
  icon: string | null;
}

export interface Machine {
  name: string;
  kind: 'windows' | 'linux' | 'mac';
  address: string;
  groups: string[];
  desktop: Way | null;
  programs: Program[];
  shell: Way | null;
  collection: string | null;
  native_rdp?: string;
  sign_in_methods?: ('restricted-admin-sso' | 'themselves')[];
}

// A machine as the files say it, for those who administer.
export interface Written extends Machine {
  file: string;
  line: number;
  given_by: { rule: string; who: string[] }[];
  served_by: string[];
  farm: string | null;
}

// A farm as the files have it, and how its feed was last read.
export interface FarmState {
  name: string;
  feed: string;
  groups: string[];
  read_at: number | null;
  failed: string | null;
  desktops: number;
  programs: number;
  left_out: string[];
  file: string;
  line: number;
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
  farms: FarmState[];
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

// Something a person starred: a machine, or a program of one.
export interface Favourite {
  machine: string;
  program: string | null;
}

// What a person starred, and the way to star something or to take its star away.
// Where Away keeps nothing, because it is given no database or doesn't reach it,
// nothing can be starred, and the pages show no stars.
export interface Stars {
  // still being asked
  asking: boolean;
  kept: boolean;
  favourites: Favourite[];
  has: (machine: string, program: string | null) => boolean;
  star: (machine: string, program: string | null, starred: boolean) => void;
}

export function useFavourites(): Stars {
  const [answer, again] = useAsked<{ kept: boolean; favourites: Favourite[] }>('/api/favourites');
  // what the person just did shows at once, and is put right by the server's answer
  const [done, setDone] = useState<Favourite[] | null>(null);
  useEffect(() => setDone(null), [answer]);
  const kept = typeof answer !== 'string' && answer.kept;
  const favourites = done ?? (typeof answer === 'string' ? [] : answer.favourites);
  const same = (one: Favourite, machine: string, program: string | null) => one.machine === machine && one.program === program;
  const star = (machine: string, program: string | null, starred: boolean) => {
    const without = favourites.filter((one) => !same(one, machine, program));
    setDone(starred ? [...without, { machine, program }] : without);
    const named = new URLSearchParams(program === null ? { machine } : { machine, program });
    fetch(`/api/favourites?${named}`, { method: starred ? 'PUT' : 'DELETE', headers: { 'x-away': '1' } }).then(again, again);
  };
  return { asking: answer === 'asking', kept, favourites, has: (machine, program) => favourites.some((one) => same(one, machine, program)), star };
}

// What a machine's desktop is reached over: what its files say, and otherwise RDP on a
// Windows machine and VNC on every other.
export function desktopOver(machine: Machine): 'rdp' | 'vnc' {
  return machine.desktop?.over ?? (machine.kind === 'windows' ? 'rdp' : 'vnc');
}

// Whether this version of Away opens a machine's desktop: it opens every desktop, over
// RDP and over VNC. A farm's desktop is one of them: its broker says which machine
// serves it.
export function opensDesktop(machine: Machine): boolean {
  return machine.desktop !== null;
}

// Whether this version of Away opens a machine's programs: those of a Windows machine,
// each in windows of its own.
export function opensPrograms(machine: Machine): boolean {
  return machine.kind === 'windows';
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
  program: Program | null;
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
  return (entry.program?.name.toLowerCase().includes(words) ?? false) || matches(entry.machine, sought);
}
