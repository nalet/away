// Which page shows: the frame for someone who is signed in, and before that the way in.
import { useCallback, useEffect, useState } from 'react';

import { Frame } from './Frame';
import { Failed, Gate, NotSetUp, Unreached } from './Gate';
import { pageOf } from './machines';
import { sessionAt, usePlace } from './place';
import { Session } from './session/Session';
import { askWho, signInAt, signOut } from './who';
import type { Who } from './who';

// How often the page asks whether the sign-in still holds, in milliseconds.
const ASKS_EVERY = 30_000;

export function App() {
  const [who, setWho] = useState<Who | 'unreached' | null>(null);
  const [path, go] = usePlace();

  const ask = useCallback(() => {
    askWho().then(
      (answer) => setWho(answer),
      () => setWho((before) => (before && before !== 'unreached' ? before : 'unreached')),
    );
  }, []);

  useEffect(() => {
    ask();
    const again = window.setInterval(ask, ASKS_EVERY);
    window.addEventListener('focus', ask);
    return () => {
      window.clearInterval(again);
      window.removeEventListener('focus', ask);
    };
  }, [ask]);

  if (who === null) return null;
  if (who === 'unreached') return <Unreached again={ask} />;
  if (!who.set_up) return <NotSetUp />;

  const here = window.location.pathname + window.location.search;
  if (!who.signed_in) {
    const asked = new URLSearchParams(window.location.search);
    if (who.failed) return <Failed failed={who.failed} contact={who.contact} />;
    if (asked.has('signed-out')) return <Gate signedOut />;
    // someone the sign-in page sent back without a sign-in sees the way in; anybody
    // else is sent to sign in at once, and comes back to the page they asked for
    if (asked.has('sign-in')) return <Gate />;
    window.location.replace(signInAt(here));
    return null;
  }

  const leave = () => {
    signOut().then(
      () => window.location.assign('/?signed-out'),
      () => ask(),
    );
  };
  const frame = (at: string) => <Frame who={who} path={at} go={go} signOut={leave} signInAgain={() => window.location.assign(signInAt(here))} />;
  // a session has the whole window: no bar and no rail of Away's around it
  const session = sessionAt(path);
  if (session !== null && !who.ran_out) {
    return <Session key={path} name={session.machine} program={session.program} go={go} contact={who.contact} showMachine={() => frame(pageOf(session.machine))} />;
  }
  return frame(path);
}
