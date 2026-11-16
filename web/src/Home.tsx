// What one may open: on home everything, on a page of their own the programs, and on
// another the machines in the groups an administrator made; and a page for each
// machine. A click opens a Windows desktop or a program.
import { Banner, Button, Card, Chevron, Code, EmptyState, Input, Textarea } from '@nalet/design-system';
import { AppWindow, ArrowLeft, Monitor, Search, SquareTerminal, Star } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { useRef, useState } from 'react';

import { entriesOf, found, opensDesktop, opensPrograms, pageOf, useFavourites, useMachines } from './machines';
import type { Entry, Machine, Stars } from './machines';
import { addressOf, desktopOf, inPlace, programOf } from './place';
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

// What one may open, by the groups of the machines, in the order the administrator's
// files first name each group, and last what is in no group.
function grouped(entries: Entry[]): { name: string | null; entries: Entry[] }[] {
  const names = [...new Set(entries.flatMap((entry) => entry.machine.groups))];
  const groups: { name: string | null; entries: Entry[] }[] = names.map((name) => ({ name, entries: entries.filter((entry) => entry.machine.groups.includes(name)) }));
  const others = entries.filter((entry) => entry.machine.groups.length === 0);
  if (others.length > 0) groups.push({ name: null, entries: others });
  return groups;
}

// Which of what one may open a page shows: everything on home, the programs alone, or
// the machines alone.
export type Which = 'home' | 'programs' | 'machines';

function entriesFor(which: Which, machines: Machine[]): Entry[] {
  if (which === 'home') return entriesOf(machines);
  if (which === 'programs') return entriesOf(machines).filter((entry) => entry.program !== null);
  // every machine, also one that offers programs alone: its page lists them
  return machines.map((machine) => ({ machine, program: null }));
}

// The star of a machine or of a program: it marks a favourite, and takes the mark away.
function StarOf({ stars, machine, program }: { stars: Stars; machine: string; program: string | null }) {
  if (!stars.kept) return null;
  const on = stars.has(machine, program);
  const what = on ? texts.stars.remove : texts.stars.add;
  return (
    <button
      type="button"
      className={on ? 'star on' : 'star'}
      aria-pressed={on}
      aria-label={texts.stars.named(what, program ?? machine)}
      title={what}
      onClick={() => stars.star(machine, program, !on)}
    >
      <Star size={16} aria-hidden="true" />
    </button>
  );
}

// What a person starred, as entries, of what they may open: in the order they starred it.
function starred(stars: Stars, machines: Machine[]): Entry[] {
  return stars.favourites.flatMap((favourite): Entry[] => {
    const machine = machines.find((one) => one.name === favourite.machine);
    if (!machine) return [];
    if (favourite.program === null) return [{ machine, program: null }];
    const program = machine.programs.find((one) => one.name === favourite.program);
    return program ? [{ machine, program }] : [];
  });
}

// A page of what one may open: home, the programs, or the machines.
export function Opens({ which, go, contact }: { which: Which; go: (to: string) => void; contact: string | null }) {
  const [machines, again] = useMachines();
  const stars = useFavourites();
  const [sought, setSought] = useState('');
  const t = texts.opens[which];

  if (machines === 'asking' || stars.asking) return <div className="fill" aria-busy="true" />;
  if (machines === 'unanswered') {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views[which]}</h1>
        <EmptyState illustration="error" size="lg" headingLevel={2} title={texts.home.unanswered} actions={<Button variant="primary" onClick={again}>{texts.home.again}</Button>} />
      </div>
    );
  }
  // where Away keeps what is starred, home is the place for it
  const favourites = which === 'home' && stars.kept;
  if (favourites && entriesFor(which, machines).length > 0 && starred(stars, machines).length === 0) {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views[which]}</h1>
        <EmptyState
          illustration="empty"
          size="lg"
          headingLevel={2}
          title={texts.stars.none}
          description={texts.stars.noneWhy}
          actions={
            <>
              <a className="nc-btn nc-btn--primary nc-btn--md" href={addressOf.programs} onClick={inPlace(go, addressOf.programs)}>
                <span className="nc-btn__label">{texts.views.programs}</span>
              </a>
              <a className="nc-btn nc-btn--default nc-btn--md" href={addressOf.machines} onClick={inPlace(go, addressOf.machines)}>
                <span className="nc-btn__label">{texts.views.machines}</span>
              </a>
            </>
          }
        />
      </div>
    );
  }
  const all = favourites ? starred(stars, machines) : entriesFor(which, machines);
  if (all.length === 0) {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views[which]}</h1>
        <EmptyState illustration="empty" size="lg" headingLevel={2} title={t.none} />
        {contact && <p className="turn-to">{texts.contact(contact)}</p>}
      </div>
    );
  }

  const shown = sought.trim() ? all.filter((entry) => found(entry, sought)) : all;
  // the programs are one grid, and so is what one starred; machines are in the groups of the files
  const one = which === 'programs' || favourites;
  const groups = one ? (shown.length > 0 ? [{ name: null, entries: shown }] : []) : grouped(shown);
  const named = groups.some((group) => group.name !== null);
  return (
    <div className="pg wide">
      <div className="homehead">
        <h1 className="unseen">{texts.views[which]}</h1>
        <Input
          type="search"
          className="find"
          leading={<Search size={14} aria-hidden="true" />}
          placeholder={t.find}
          aria-label={t.find}
          value={sought}
          onChange={(event) => setSought(event.target.value)}
        />
      </div>
      {shown.length === 0 && (
        <div className="nomatch" role="status">
          <p>{t.noMatch(sought.trim())}</p>
          <p className="small">{t.looksAt}</p>
        </div>
      )}
      {groups.map((group) => {
        const title = favourites ? texts.stars.favourites : which === 'programs' ? texts.views.programs : (group.name ?? (named ? texts.home.others : texts.home.machines));
        const id = `group-${group.name ?? ''}`.replace(/[^a-zA-Z0-9-]/g, '_');
        return (
          <section className="grp" key={id} aria-labelledby={id}>
            <h2 className="gh" id={id}>
              <Chevron size={14} aria-hidden="true" />
              {title} <span className="n">{group.entries.length}</span>
            </h2>
            <ul className="mgrid">
              {group.entries.map(({ machine, program }) => {
                // a program is an entry of its own, and a click on it opens it. A machine that
                // offers its desktop and nothing else, where Away opens it, is opened by a
                // click on it; one that offers more leads to its page, where each way in is
                const Icon = program === null ? iconOf(machine) : AppWindow;
                const only = program === null && !machine.native_rdp && opensDesktop(machine) && machine.programs.length === 0 && !machine.shell;
                const opens = program !== null && opensPrograms(machine);
                const to = only ? desktopOf(machine.name) : opens ? programOf(machine.name, program.name) : pageOf(machine.name);
                return (
                  <li key={`${machine.name} ${program?.name ?? ''}`} className={program === null && machine.sign_in_methods ? 'with-methods' : undefined}>
                    <a className="m" href={to} onClick={inPlace(go, to)}>
                      {program?.icon ? <img className="icon" src={program.icon} alt="" width={24} height={24} /> : <Icon size={22} aria-hidden="true" />}
                      <span className="main">
                        <span className="name">{program?.name ?? machine.name}</span>
                        <span className="sub">{program === null ? whatItIs(machine) : texts.programOn(machine.name)}</span>
                      </span>
                    </a>
                    {program === null && machine.sign_in_methods?.includes('themselves') && (
                      <a className="sign-in-choice" href={desktopOf(machine.name, 'themselves')} onClick={inPlace(go, desktopOf(machine.name, 'themselves'))} aria-label={`Open with certificate SSO on ${machine.name}`}>
                        Open with certificate SSO
                      </a>
                    )}
                    <StarOf stars={stars} machine={machine.name} program={program?.name ?? null} />
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

function NativeRdp({ command }: { command: string }) {
  const field = useRef<HTMLTextAreaElement>(null);
  const [status, setStatus] = useState('');
  const t = texts.machine.nativeRdp;
  const copy = async () => {
    try {
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(command);
        setStatus(t.copied);
        return;
      }
    } catch {
      // Some browsers block clipboard writes. The command can still be selected.
    }
    field.current?.focus();
    field.current?.select();
    setStatus(t.select);
  };
  return (
    <Card header={<strong>{t.title}</strong>}>
      <div className="native-rdp">
        <p>{t.how}</p>
        <Textarea ref={field} rows={2} aria-label={t.command} readOnly value={command} />
        <div className="native-rdp-actions">
          <Button size="sm" onClick={copy}>{t.copy}</Button>
          <span role="status">{status}</span>
        </div>
      </div>
    </Card>
  );
}

// One machine: what it offers, and what it is.
export function MachinePage({ name, go }: { name: string; go: (to: string) => void }) {
  const [machines, again] = useMachines();
  const stars = useFavourites();
  if (machines === 'asking' || stars.asking) return <div className="fill" aria-busy="true" />;
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
        <a className="nc-iconbtn nc-iconbtn--ghost nc-iconbtn--md" href={addressOf.machines} aria-label={texts.machine.back} title={texts.machine.back} onClick={inPlace(go, addressOf.machines)}>
          <ArrowLeft size={16} aria-hidden="true" />
        </a>
        <Icon size={22} className="kind" aria-hidden="true" />
        <h1 className="asis">{machine.name}</h1>
        <StarOf stars={stars} machine={machine.name} program={null} />
      </div>
      {(machine.shell !== null || (machine.desktop !== null && !opensDesktop(machine)) || (machine.programs.length > 0 && !opensPrograms(machine))) && (
        <Banner state="information">{texts.machine.notYet}</Banner>
      )}
      <div className="cols rev">
        <div>
          {machine.desktop && (
            <section className="grp" aria-labelledby="way-desktop">
              <h2 className="gh" id="way-desktop">
                <Chevron size={14} aria-hidden="true" />
                {texts.machine.desktop}
              </h2>
              {machine.native_rdp ? (
                <NativeRdp key={machine.native_rdp} command={machine.native_rdp} />
              ) : opensDesktop(machine) ? (
                <>
                <a className="m still" href={desktopOf(machine.name)} onClick={inPlace(go, desktopOf(machine.name))}>
                  <Monitor size={22} aria-hidden="true" />
                  <span className="main">
                    <span className="name">{machine.sign_in_methods ? 'Open with Restricted Admin' : texts.machine.desktopOf[machine.kind]}</span>
                  </span>
                </a>
                {machine.sign_in_methods?.includes('themselves') && (
                  <a className="m still" href={desktopOf(machine.name, 'themselves')} onClick={inPlace(go, desktopOf(machine.name, 'themselves'))}>
                    <Monitor size={22} aria-hidden="true" />
                    <span className="main"><span className="name">Open with certificate SSO</span></span>
                  </a>
                )}
                </>
              ) : (
                <div className="m still">
                  <Monitor size={22} aria-hidden="true" />
                  <span className="main">
                    <span className="name">{texts.machine.desktopOf[machine.kind]}</span>
                  </span>
                </div>
              )}
            </section>
          )}
          {machine.programs.length > 0 && (
            <section className="grp" aria-labelledby="way-programs">
              <h2 className="gh" id="way-programs">
                <Chevron size={14} aria-hidden="true" />
                {texts.machine.programs} <span className="n">{machine.programs.length}</span>
              </h2>
              <ul className="mgrid">
                {machine.programs.map((program) => {
                  const shows = (
                    <>
                      {program.icon ? <img className="icon" src={program.icon} alt="" width={24} height={24} /> : <AppWindow size={22} aria-hidden="true" />}
                      <span className="main">
                        <span className="name">{program.name}</span>
                      </span>
                    </>
                  );
                  // a program that Away opens is opened by a click on it
                  const to = programOf(machine.name, program.name);
                  return opensPrograms(machine) ? (
                    <li key={program.name}>
                      <a className="m prog" href={to} onClick={inPlace(go, to)}>
                        {shows}
                      </a>
                      <StarOf stars={stars} machine={machine.name} program={program.name} />
                    </li>
                  ) : (
                    <li key={program.name} className="m prog still">
                      {shows}
                      <StarOf stars={stars} machine={machine.name} program={program.name} />
                    </li>
                  );
                })}
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
