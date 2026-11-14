// Where in Away the browser is. Every view has an address of its own, so a link, a
// reload and the back button lead to it.
import { useCallback, useEffect, useState } from 'react';
import type { MouseEvent } from 'react';

export type View = 'home' | 'programs' | 'machines' | 'sessions' | 'administration' | 'settings';

export const addressOf: Record<View, string> = {
  home: '/',
  programs: '/programs',
  machines: '/machines',
  sessions: '/sessions',
  administration: '/administration',
  settings: '/settings',
};

// What Away says about itself: a page of the settings, with an address of its own.
export const ABOUT = '/settings/about';

export function viewAt(path: string): View | null {
  if (path === ABOUT) return 'settings';
  // a machine's page is a page of the machines, and the rules are a page of the administration
  if (/^\/machines\/[^/]+$/.test(path)) return 'machines';
  if (path === '/administration/rules' || path === '/administration/farms') return 'administration';
  const found = (Object.keys(addressOf) as View[]).find((view) => addressOf[view] === path);
  return found ?? null;
}

// Where a machine's desktop is open.
export function desktopOf(name: string): string {
  return `/machines/${encodeURIComponent(name)}/desktop`;
}

// Where a program of a machine is open.
export function programOf(machine: string, program: string): string {
  return `/machines/${encodeURIComponent(machine)}/programs/${encodeURIComponent(program)}`;
}

// What a page's address opens, where it opens something: a machine's desktop, or one of
// its programs.
export function sessionAt(path: string): { machine: string; program: string | null } | null {
  const [, machine, program] = /^\/machines\/([^/]+)\/(?:desktop|programs\/([^/]+))$/.exec(path) ?? [];
  if (!machine) return null;
  try {
    return { machine: decodeURIComponent(machine), program: program ? decodeURIComponent(program) : null };
  } catch {
    return null;
  }
}

export function usePlace(): [string, (to: string) => void] {
  const [path, setPath] = useState(window.location.pathname);
  useEffect(() => {
    const moved = () => setPath(window.location.pathname);
    window.addEventListener('popstate', moved);
    return () => window.removeEventListener('popstate', moved);
  }, []);
  const go = useCallback((to: string) => {
    window.history.pushState(null, '', to);
    setPath(window.location.pathname);
  }, []);
  return [path, go];
}

// A link inside Away changes the view without loading the page anew. A click that asks
// for a new tab or a new window is left to the browser.
export function inPlace(go: (to: string) => void, to: string) {
  return (event: MouseEvent<HTMLAnchorElement>) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    go(to);
  };
}
