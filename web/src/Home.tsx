// Home: the machines one may reach, in the groups an administrator made, and a page for
// each machine. This version shows them and opens none yet.
import { Banner, Button, Card, Chevron, Code, EmptyState, Input } from '@nalet/design-system';
import { AppWindow, ArrowLeft, Monitor, Search, SquareTerminal } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { useState } from 'react';

import { matches, pageOf, useMachines } from './machines';
import type { Machine } from './machines';
import { inPlace } from './place';
import { texts } from './texts';

// What a machine is, in one line: its desktop, its programs, its shell.
export function whatItIs(machine: Machine): string {
  const kind = texts.kindNames[machine.kind];
  const parts: string[] = [];
  if (machine.desktop) parts.push(`${kind} desktop`);
  if (machine.programs.length > 0) parts.push(machine.desktop ? texts.programCount(machine.programs.length) : `${texts.programCount(machine.programs.length)} on ${kind}`);
  if (machine.shell) parts.push('shell');
  return parts.join(', ');
}

export function iconOf(machine: Machine): LucideIcon {
  return machine.desktop ? Monitor : machine.programs.length > 0 ? AppWindow : SquareTerminal;
}

// The machines by their groups, in the order of the groups' names, and last those in none.
function grouped(machines: Machine[]): { name: string | null; machines: Machine[] }[] {
  const names = [...new Set(machines.flatMap((machine) => machine.groups))].sort((one, other) => one.localeCompare(other, undefined, { sensitivity: 'base' }));
  const groups: { name: string | null; machines: Machine[] }[] = names.map((name) => ({ name, machines: machines.filter((machine) => machine.groups.includes(name)) }));
  const others = machines.filter((machine) => machine.groups.length === 0);
  if (others.length > 0) groups.push({ name: null, machines: others });
  return groups;
}

export function Home({ go, contact }: { go: (to: string) => void; contact: string | null }) {
  const [machines, again] = useMachines();
  const [sought, setSought] = useState('');

  if (machines === 'asking') return <div className="fill" aria-busy="true" />;
  if (machines === 'unanswered') {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views.home}</h1>
        <EmptyState illustration="error" size="lg" headingLevel={2} title={texts.home.unanswered} actions={<Button variant="primary" onClick={again}>{texts.home.again}</Button>} />
      </div>
    );
  }
  if (machines.length === 0) {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views.home}</h1>
        <EmptyState illustration="empty" size="lg" headingLevel={2} title={texts.home.none} />
        {contact && <p className="turn-to">{texts.contact(contact)}</p>}
      </div>
    );
  }

  const found = sought.trim() ? machines.filter((machine) => matches(machine, sought)) : machines;
  const groups = grouped(found);
  const named = groups.some((group) => group.name !== null);
  return (
    <div className="pg wide">
      <div className="homehead">
        <h1 className="unseen">{texts.views.home}</h1>
        <Input
          type="search"
          className="find"
          leading={<Search size={14} aria-hidden="true" />}
          placeholder={texts.home.find}
          aria-label={texts.home.find}
          value={sought}
          onChange={(event) => setSought(event.target.value)}
        />
      </div>
      {found.length === 0 && (
        <div className="nomatch" role="status">
          <p>{texts.home.noMatch(sought.trim())}</p>
          <p className="small">{texts.home.looksAt}</p>
        </div>
      )}
      {groups.map((group) => {
        const title = group.name ?? (named ? texts.home.others : texts.home.machines);
        const id = `group-${group.name ?? ''}`.replace(/[^a-zA-Z0-9-]/g, '_');
        return (
          <section className="grp" key={id} aria-labelledby={id}>
            <h2 className="gh" id={id}>
              <Chevron size={14} aria-hidden="true" />
              {title} <span className="n">{group.machines.length}</span>
            </h2>
            <ul className="mgrid">
              {group.machines.map((machine) => {
                const Icon = iconOf(machine);
                return (
                  <li key={machine.name}>
                    <a className="m" href={pageOf(machine.name)} onClick={inPlace(go, pageOf(machine.name))}>
                      <Icon size={22} aria-hidden="true" />
                      <span className="main">
                        <span className="name">{machine.name}</span>
                        <span className="sub">{whatItIs(machine)}</span>
                      </span>
                    </a>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </div>
  );
}

// One machine: what it offers, and what it is.
export function MachinePage({ name, go }: { name: string; go: (to: string) => void }) {
  const [machines, again] = useMachines();
  if (machines === 'asking') return <div className="fill" aria-busy="true" />;
  if (machines === 'unanswered') {
    return (
      <div className="fill">
        <h1 className="unseen asis">{name}</h1>
        <EmptyState illustration="error" size="lg" headingLevel={2} title={texts.home.unanswered} actions={<Button variant="primary" onClick={again}>{texts.home.again}</Button>} />
      </div>
    );
  }
  const machine = machines.find((one) => one.name === name);
  if (!machine) {
    return (
      <div className="fill">
        <h1 className="unseen asis">{name}</h1>
        <EmptyState
          illustration="unknown"
          size="lg"
          headingLevel={2}
          title={texts.machine.notHere}
          description={texts.machine.notHereWhy}
          actions={
            <a className="nc-btn nc-btn--default nc-btn--md" href="/" onClick={inPlace(go, '/')}>
              <span className="nc-btn__label">{texts.machine.home}</span>
            </a>
          }
        />
      </div>
    );
  }
  const Icon = iconOf(machine);
  return (
    <div className="pg ab">
      <div className="pghead">
        <a className="nc-iconbtn nc-iconbtn--ghost nc-iconbtn--md" href="/" aria-label={texts.machine.back} title={texts.machine.back} onClick={inPlace(go, '/')}>
          <ArrowLeft size={16} aria-hidden="true" />
        </a>
        <Icon size={22} className="kind" aria-hidden="true" />
        <h1 className="asis">{machine.name}</h1>
      </div>
      <Banner state="information">{texts.machine.notYet}</Banner>
      <div className="cols rev">
        <div>
          {machine.desktop && (
            <section className="grp" aria-labelledby="way-desktop">
              <h2 className="gh" id="way-desktop">
                <Chevron size={14} aria-hidden="true" />
                {texts.machine.desktop}
              </h2>
              <div className="m still">
                <Monitor size={22} aria-hidden="true" />
                <span className="main">
                  <span className="name">{texts.machine.desktopOf[machine.kind]}</span>
                </span>
              </div>
            </section>
          )}
          {machine.programs.length > 0 && (
            <section className="grp" aria-labelledby="way-programs">
              <h2 className="gh" id="way-programs">
                <Chevron size={14} aria-hidden="true" />
                {texts.machine.programs} <span className="n">{machine.programs.length}</span>
              </h2>
              <ul className="mgrid">
                {machine.programs.map((program) => (
                  <li key={program} className="m prog still">
                    <AppWindow size={22} aria-hidden="true" />
                    <span className="main">
                      <span className="name">{program}</span>
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {machine.shell && (
            <section className="grp" aria-labelledby="way-shell">
              <h2 className="gh" id="way-shell">
                <Chevron size={14} aria-hidden="true" />
                {texts.machine.shell}
              </h2>
              <div className="m still">
                <SquareTerminal size={22} aria-hidden="true" />
                <span className="main">
                  <span className="name">{texts.machine.shellOf}</span>
                </span>
              </div>
            </section>
          )}
        </div>
        <section aria-labelledby="about-machine">
          <Card header={<h2 id="about-machine">{texts.machine.about}</h2>}>
            <dl className="facts">
              <dt>{texts.machine.whatItIs}</dt>
              <dd>{texts.machine.kinds[machine.kind]}</dd>
              {machine.collection && (
                <>
                  <dt>{texts.machine.collection}</dt>
                  <dd>{machine.collection}</dd>
                </>
              )}
              <dt>{texts.machine.address}</dt>
              <dd>
                <Code>{machine.address}</Code>
              </dd>
              <dt>{texts.machine.groups}</dt>
              <dd>{machine.groups.length > 0 ? machine.groups.join(', ') : texts.machine.inNone}</dd>
            </dl>
          </Card>
        </section>
      </div>
    </div>
  );
}
