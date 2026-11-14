// The frame: what is around every page of Away. A bar across the top with the mark and
// the avatar, and the views as icons alone: in a rail down the left on a wide screen and
// on a phone held sideways, in a bar at the foot on a phone held upright.
//
// The design system has no frame yet, so this one is built from its tokens.
import { Button, Chevron } from '@nalet/design-system';
import { AppWindow, House, Layers, Monitor, Settings, Shield } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { useEffect, useRef } from 'react';

import { ABOUT, addressOf, inPlace, viewAt } from './place';
import type { View } from './place';
import { texts } from './texts';
import { UserMenu } from './UserMenu';
import { Administration } from './Administration';
import { MachinePage, Opens } from './Home';
import { namedBy } from './machines';
import { About, Nowhere, Sessions, SettingsPage } from './views';
import type { Who } from './who';

const ICONS: Record<View, LucideIcon> = { home: House, programs: AppWindow, machines: Monitor, sessions: Layers, administration: Shield, settings: Settings };

interface FrameProps {
  who: Who;
  path: string;
  go: (to: string) => void;
  signOut: () => void;
  signInAgain: () => void;
}

export function Frame({ who, path, go, signOut, signInAgain }: FrameProps) {
  const view = viewAt(path);
  const machine = namedBy(path);
  const page = useRef<HTMLElement>(null);
  // the administration holds what administrators see; the records come with a later version
  const administers = Boolean(who.administers);
  // the views, in their order; administration only for those who have a role there
  const views: View[] = administers ? ['home', 'programs', 'machines', 'sessions', 'administration'] : ['home', 'programs', 'machines', 'sessions'];

  useEffect(() => {
    const title = path === ABOUT ? texts.about.title : (namedBy(path) ?? (view ? texts.views[view] : texts.nowhere.title));
    document.title = `${title} · ${texts.name}`;
  }, [view, path]);

  const link = (to: View, where: 'rail' | 'tabs') => {
    const Icon = ICONS[to];
    const current = to === view;
    const anchor = (
      <a
        key={to}
        href={addressOf[to]}
        className={`it it-${to}${current ? ' on' : ''}${where === 'rail' ? ' nc-tooltip__trigger' : ''}`}
        aria-label={texts.views[to]}
        aria-current={current ? 'page' : undefined}
        onClick={inPlace(go, addressOf[to])}
      >
        <Icon size={24} aria-hidden="true" />
      </a>
    );
    if (where === 'tabs') return anchor;
    // under the pointer and under the keyboard's focus a view says its name. The design
    // system's tooltip makes a stop of its own for the Tab key, which a link doesn't
    // need, so its classes are used around the link itself
    return (
      <span key={to} className="nc-tooltip">
        {anchor}
        <span className="nc-tooltip__bubble nc-tooltip__bubble--right" aria-hidden="true">
          {texts.views[to]}
        </span>
      </span>
    );
  };

  return (
    <div className="frame">
      <a
        className="skip"
        href="#page"
        onClick={(event) => {
          event.preventDefault();
          page.current?.focus();
        }}
      >
        {texts.skip}
      </a>
      <header className="bar">
        <a className="mark" href="/" aria-label={`${texts.name}, ${texts.views.home}`} onClick={inPlace(go, '/')}>
          <Chevron size={16} aria-hidden="true" />
          <b className="letter">a</b>
          <span className="word">way</span>
        </a>
        <div className="end">
          <UserMenu who={who} go={go} signOut={signOut} />
        </div>
      </header>
      <nav className="rail" aria-label="views">
        {views.map((to) => link(to, 'rail'))}
        <span className="gap" />
        {link('settings', 'rail')}
      </nav>
      <main className="page" id="page" tabIndex={-1} ref={page}>
        {view === 'home' && <Opens which="home" go={go} contact={who.contact} key="home" />}
        {view === 'programs' && <Opens which="programs" go={go} contact={who.contact} key="programs" />}
        {view === 'machines' && (machine ? <MachinePage name={machine} go={go} key={machine} /> : <Opens which="machines" go={go} contact={who.contact} key="machines" />)}
        {view === 'sessions' && <Sessions />}
        {view === 'administration' && (administers ? <Administration path={path} go={go} /> : <Nowhere go={go} />)}
        {view === 'settings' && (path === ABOUT ? <About who={who} go={go} /> : <SettingsPage who={who} go={go} />)}
        {view === null && <Nowhere go={go} />}
      </main>
      <nav className="tabs" aria-label="views">
        {views.map((to) => link(to, 'tabs'))}
      </nav>
      {who.ran_out && (
        <div className="over" role="alertdialog" aria-modal="true" aria-labelledby="ran-out" aria-describedby="ran-out-why">
          <div className="card">
            <h2 id="ran-out">{texts.ranOut.title}</h2>
            <p id="ran-out-why">{texts.ranOut.why}</p>
            <div className="foot">
              <Button variant="primary" autoFocus onClick={signInAgain}>
                {texts.ranOut.signIn}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
