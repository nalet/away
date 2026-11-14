// The avatar and its menu: who is signed in, the settings, and signing out. The design
// system plans a menu of this kind; until it has one, this is built from its tokens and
// from the classes of its menu of actions.
import { LogOut, Settings } from 'lucide-react';
import { useEffect, useId, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';

import { addressOf } from './place';
import { texts } from './texts';
import type { Who } from './who';

export function UserMenu({ who, go, signOut }: { who: Who; go: (to: string) => void; signOut: () => void }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const id = useId();

  const items = () => Array.from(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
  const close = (back: boolean) => {
    setOpen(false);
    if (back) button.current?.focus();
  };

  // opening moves into the menu; a click or a tap elsewhere closes it
  useEffect(() => {
    if (!open) return;
    items()[0]?.focus();
    const elsewhere = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', elsewhere);
    return () => document.removeEventListener('pointerdown', elsewhere);
  }, [open]);

  const keys = (event: KeyboardEvent<HTMLDivElement>) => {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    const to = (index: number) => {
      event.preventDefault();
      all[(index + all.length) % all.length]?.focus();
    };
    if (event.key === 'ArrowDown') to(at + 1);
    else if (event.key === 'ArrowUp') to(at - 1);
    else if (event.key === 'Home') to(0);
    else if (event.key === 'End') to(all.length - 1);
    else if (event.key === 'Escape') {
      event.preventDefault();
      close(true);
    } else if (event.key === 'Tab') setOpen(false);
  };

  const name = who.name ?? '';
  return (
    <div className="user" ref={root}>
      <button
        ref={button}
        type="button"
        className="avatar"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        aria-label={texts.menu.account(name)}
        onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' && !open) {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span className="av" aria-hidden="true">
          {who.initials}
        </span>
      </button>
      {open && (
        <div className="usermenu" id={id} ref={menu} role="menu" aria-label={texts.menu.account(name)} onKeyDown={keys}>
          <div className="who" role="presentation">
            <span className="who-name">{name}</span>
            {who.address && <span className="who-address">{who.address}</span>}
          </div>
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            className="nc-actionmenu__item"
            onClick={() => {
              close(false);
              go(addressOf.settings);
            }}
          >
            <span className="nc-actionmenu__icon" aria-hidden="true">
              <Settings size={16} />
            </span>
            <span className="nc-actionmenu__label">{texts.menu.settings}</span>
          </button>
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            className="nc-actionmenu__item"
            onClick={() => {
              close(false);
              signOut();
            }}
          >
            <span className="nc-actionmenu__icon" aria-hidden="true">
              <LogOut size={16} />
            </span>
            <span className="nc-actionmenu__label">{texts.menu.signOut}</span>
          </button>
        </div>
      )}
    </div>
  );
}
