// The pages inside the frame. This version signs people in and knows no machines, so
// home, the sessions and the administration say that there is nothing yet.
import { EmptyState } from '@nalet/design-system';

import { inPlace } from './place';
import { texts } from './texts';
import type { Who } from './who';

export function Home() {
  return (
    <div className="fill">
      <h1 className="unseen">{texts.views.home}</h1>
      <EmptyState illustration="empty" size="lg" title={texts.home.title} description={texts.home.why} />
    </div>
  );
}

export function Sessions() {
  return (
    <div className="fill">
      <h1 className="unseen">{texts.views.sessions}</h1>
      <EmptyState illustration="empty" size="lg" title={texts.sessions.title} />
    </div>
  );
}

export function Administration() {
  return (
    <div className="fill">
      <h1 className="unseen">{texts.views.administration}</h1>
      <EmptyState illustration="empty" size="lg" title={texts.administration.title} description={texts.administration.why} />
    </div>
  );
}

export function Nowhere({ go }: { go: (to: string) => void }) {
  return (
    <div className="fill">
      <EmptyState
        illustration="unknown"
        size="lg"
        headingLevel={2}
        title={texts.nowhere.title}
        actions={
          <a className="nc-btn nc-btn--default nc-btn--md" href="/" onClick={inPlace(go, '/')}>
            <span className="nc-btn__label">{texts.nowhere.home}</span>
          </a>
        }
      />
    </div>
  );
}

// The settings: in this version, who is signed in and what runs.
export function SettingsPage({ who }: { who: Who }) {
  return (
    <div className="pg">
      <h1>{texts.views.settings}</h1>
      <section aria-labelledby="about">
        <h2 id="about">{texts.settings.about}</h2>
        <dl className="facts">
          <dt>{texts.settings.you}</dt>
          <dd>
            {who.name}
            {who.address ? `, ${who.address}` : ''}
          </dd>
          <dt>{texts.settings.version}</dt>
          <dd className="mono">{who.version}</dd>
          <dt>{texts.settings.built}</dt>
          <dd className="mono">{who.commit}</dd>
          <dt>{texts.settings.source}</dt>
          <dd>
            <a href={who.source} rel="noreferrer">
              {who.source}
            </a>
          </dd>
        </dl>
        {who.contact && <p className="turn-to">{texts.contact(who.contact)}</p>}
      </section>
    </div>
  );
}

