// The pages of someone who is not in: the way in, and what is said when it fails.
import { Button, Chevron, EmptyState } from '@nalet/design-system';
import { useEffect } from 'react';
import type { ReactNode } from 'react';

import { texts } from './texts';
import { signInAt } from './who';
import type { Who } from './who';

function titled(title: string) {
  document.title = title === texts.name ? title : `${title} · ${texts.name}`;
}

function Whole({ children }: { children: ReactNode }) {
  return <main className="whole">{children}</main>;
}

// The page of someone who is not signed in: the name, one sentence, one button.
export function Gate({ signedOut = false }: { signedOut?: boolean }) {
  useEffect(() => titled(texts.name), []);
  return (
    <Whole>
      <h1 className="name">
        <Chevron size={26} aria-hidden="true" />
        {texts.name}
      </h1>
      {signedOut && (
        <p className="said" role="status">
          {texts.gate.signedOut}
        </p>
      )}
      <p className="how">{texts.gate.how}</p>
      <Button variant="primary" size="lg" onClick={() => window.location.assign(signInAt('/'))}>
        {texts.gate.signIn}
      </Button>
    </Whole>
  );
}

// Signing in failed: what happened, why, a way to try again, the technical reason, and
// whom to turn to.
export function Failed({ failed, contact }: { failed: NonNullable<Who['failed']>; contact: string | null }) {
  useEffect(() => titled(texts.failed.title), []);
  return (
    <Whole>
      <EmptyState
        illustration="error"
        size="lg"
        title={texts.failed.title}
        description={texts.failed[failed.why]}
        actions={
          <Button variant="primary" onClick={() => window.location.assign(signInAt('/'))}>
            {texts.failed.again}
          </Button>
        }
      />
      <details className="details">
        <summary>{texts.failed.details}</summary>
        <p>{failed.details}</p>
      </details>
      {contact && <p className="turn-to">{texts.contact(contact)}</p>}
    </Whole>
  );
}

// An installation that is told of no sign-in service.
export function NotSetUp() {
  useEffect(() => titled(texts.notSetUp.title), []);
  return (
    <Whole>
      <EmptyState illustration="unknown" size="lg" title={texts.notSetUp.title} description={texts.notSetUp.why} />
    </Whole>
  );
}

// The page couldn't reach the server.
export function Unreached({ again }: { again: () => void }) {
  useEffect(() => titled(texts.unreached.title), []);
  return (
    <Whole>
      <EmptyState
        illustration="error"
        size="lg"
        title={texts.unreached.title}
        description={texts.unreached.why}
        actions={
          <Button variant="primary" onClick={again}>
            {texts.unreached.again}
          </Button>
        }
      />
    </Whole>
  );
}
