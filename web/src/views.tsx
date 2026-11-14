// The pages inside the frame that are neither home nor the administration: the sessions,
// of which this version opens none, the settings, and what Away says of itself.
import { Button, Card, Code, Divider, EmptyState, Input, Table } from '@nalet/design-system';
import { ArrowLeft, ChevronRight, Search } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';

import { ABOUT, addressOf, inPlace } from './place';
import { texts } from './texts';
import { askBuiltFrom } from './who';
import type { Part, Who } from './who';

export function Sessions() {
  return (
    <div className="fill">
      <h1 className="unseen">{texts.views.sessions}</h1>
      <EmptyState illustration="empty" size="lg" title={texts.sessions.title} />
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

// The settings: in this version, who is signed in, and the way to what Away says of
// itself.
export function SettingsPage({ who, go }: { who: Who; go: (to: string) => void }) {
  return (
    <div className="pg">
      <h1>{texts.views.settings}</h1>
      <section aria-labelledby="account">
        <Card header={<h2 id="account">{texts.settings.account}</h2>}>
          <div className="li">
            <span className="av" aria-hidden="true">
              {who.initials}
            </span>
            <div className="main">
              <span className="ttl">{who.name}</span>
              {who.address && <span className="sub">{who.address}</span>}
            </div>
          </div>
        </Card>
      </section>
      <section aria-labelledby="about">
        <Card header={<h2 id="about">{texts.settings.about}</h2>}>
          <a className="li to" href={ABOUT} onClick={inPlace(go, ABOUT)}>
            <div className="main">
              <span className="ttl">{texts.settings.aboutAway}</span>
              <span className="sub">{texts.settings.aboutWhat(who.version)}</span>
            </div>
            <ChevronRight size={16} aria-hidden="true" />
          </a>
        </Card>
      </section>
      {who.contact && <p className="turn-to">{texts.contact(who.contact)}</p>}
    </div>
  );
}

// A licence as people read it: the names as they are, the words between them small.
function written(licence: string): string {
  return licence.replaceAll(' OR ', ' or ').replaceAll(' AND ', ' and ');
}

// What Away says of itself: which version runs, where its source is, the licence it is
// under, and every part it is built from with that part's licence.
export function About({ who, go }: { who: Who; go: (to: string) => void }) {
  const [parts, setParts] = useState<Part[] | 'asking' | 'not said'>('asking');
  const [sought, setSought] = useState('');
  const ask = useCallback(() => {
    setParts('asking');
    askBuiltFrom().then(setParts, () => setParts('not said'));
  }, []);
  useEffect(ask, [ask]);

  const all = Array.isArray(parts) ? parts : [];
  const words = sought.trim().toLowerCase();
  const shown = words ? all.filter((part) => `${part.name} ${part.licence}`.toLowerCase().includes(words)) : all;
  // a commit is said by its first letters, as people say one
  const commit = /^[0-9a-f]{12,}$/.test(who.commit) ? who.commit.slice(0, 7) : null;
  const source = who.source.replace(/^https?:\/\//, '');

  return (
    <div className="pg ab">
      <div className="pghead">
        <a className="nc-iconbtn nc-iconbtn--ghost nc-iconbtn--md" href={addressOf.settings} aria-label={texts.about.back} title={texts.about.back} onClick={inPlace(go, addressOf.settings)}>
          <ArrowLeft size={16} aria-hidden="true" />
        </a>
        <h1>{texts.about.title}</h1>
      </div>
      <div className="cols">
        <section aria-labelledby="this-away">
          <Card header={<h2 id="this-away">{texts.about.thisAway}</h2>}>
            <dl className="facts">
              <dt>{texts.about.version}</dt>
              <dd>{who.version}</dd>
              <dt>{texts.about.built}</dt>
              <dd>
                {commit ? <Code>{commit}</Code> : texts.about.unnamed} {texts.about.of}{' '}
                <a href={who.source} rel="noreferrer">
                  {source}
                </a>
              </dd>
              <dt>{texts.about.licence}</dt>
              <dd>
                {texts.about.licences[who.licence] ?? who.licence} · <a href="/licence.txt">{texts.about.itsText}</a>
              </dd>
            </dl>
            <Divider />
            <p className="small">{texts.about.nobody}</p>
            <Divider />
            <p className="small">
              {texts.about.hole}{' '}
              <a href={`${who.source}/security/policy`} rel="noreferrer">
                {texts.about.report}
              </a>
              .
            </p>
          </Card>
        </section>
        <section aria-labelledby="built-from" aria-busy={parts === 'asking'}>
          <div className="listhead">
            <h2 id="built-from">{texts.about.builtFrom}</h2>
            {Array.isArray(parts) && (
              <span className="count" role="status">
                {words ? texts.about.partsOf(shown.length, all.length) : texts.about.parts(all.length)}
              </span>
            )}
            {Array.isArray(parts) && (
              <Input
                type="search"
                inputSize="sm"
                className="find"
                leading={<Search size={14} aria-hidden="true" />}
                placeholder={texts.about.find}
                aria-label={texts.about.find}
                value={sought}
                onChange={(event) => setSought(event.target.value)}
              />
            )}
          </div>
          {parts === 'not said' && (
            <div className="said">
              <p role="alert">{texts.about.notSaid}</p>
              <Button onClick={ask}>{texts.about.again}</Button>
            </div>
          )}
          {Array.isArray(parts) && (
            <Table
              dense
              aria-labelledby="built-from"
              rowKey={(part) => `${part.name} ${part.version}`}
              columns={[
                { key: 'name', header: texts.about.part },
                { key: 'version', header: texts.about.partVersion, width: 120, render: (part) => part.version || '—' },
                { key: 'licence', header: texts.about.partLicence, width: 220, render: (part) => written(part.licence) },
              ]}
              rows={shown}
              empty={texts.about.none}
            />
          )}
        </section>
      </div>
    </div>
  );
}
