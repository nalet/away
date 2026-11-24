// The administration: the machines and the rules that are in force, each with the file
// it comes from, and the change that was refused, where one was. In this version the
// browser changes none of them: the files do.
import { Banner, Button, Code, EmptyState, Input, Table, Tabs } from '@nalet/design-system';
import { Search } from 'lucide-react';
import { useState } from 'react';

import { whatItIs } from './Home';
import { Passwords } from './Passwords';
import { matches, useInForce } from './machines';
import type { FarmState, Rule, Written } from './machines';
import { texts } from './texts';

export const RULES = '/administration/rules';
export const FARMS = '/administration/farms';
export const PASSWORDS = '/administration/passwords';

// A moment, as people write one: the day and the time, in the reader's own way.
function moment(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'long', timeStyle: 'short' });
}

export function Administration({ path, go }: { path: string; go: (to: string) => void }) {
  const [inForce, again] = useInForce();
  const [sought, setSought] = useState('');
  const tab = path === RULES ? 'rules' : path === FARMS ? 'farms' : path === PASSWORDS ? 'passwords' : 'machines';
  const t = texts.administration;

  if (inForce === 'asking') return <div className="fill" aria-busy="true" />;
  if (inForce === 'unanswered') {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views.administration}</h1>
        <EmptyState illustration="error" size="lg" headingLevel={2} title={t.unanswered} actions={<Button variant="primary" onClick={again}>{t.again}</Button>} />
      </div>
    );
  }
  if (!inForce.has_files) {
    return (
      <div className="fill">
        <h1 className="unseen">{texts.views.administration}</h1>
        <EmptyState illustration="empty" size="lg" headingLevel={2} title={t.noFiles} description={t.noFilesWhy} />
      </div>
    );
  }

  const { refused } = inForce;
  const nothing = inForce.machines.length === 0 && inForce.rules.length === 0;
  const machines = sought.trim() ? inForce.machines.filter((machine) => matches(machine, sought) || machine.file.toLowerCase().includes(sought.trim().toLowerCase())) : inForce.machines;
  return (
    <div className="pg wide adm">
      <h1>{texts.views.administration}</h1>
      <Tabs
        aria-label={texts.views.administration}
        items={[
          { value: 'machines', label: t.tabs.machines },
          { value: 'rules', label: t.tabs.rules },
          { value: 'farms', label: t.tabs.farms },
          { value: 'passwords', label: t.tabs.passwords },
        ]}
        value={tab}
        onChange={(to) => go(to === 'rules' ? RULES : to === 'farms' ? FARMS : to === 'passwords' ? PASSWORDS : '/administration')}
      />
      {refused && (
        <Banner state="error" live="polite">
          {refused.file ? t.refused(refused.file, moment(refused.at)) : t.refusedSet(moment(refused.at))}{' '}
          {refused.line > 0 ? t.inLine(refused.line, refused.why) : t.whole(refused.why)} {nothing ? t.nothing : t.stays}
        </Banner>
      )}
      {inForce.farms
        .filter((farm) => farm.failed)
        .map((farm) => (
          <Banner state="warning" live="polite" key={`failed ${farm.name}`}>
            {t.feedFailed(farm.name)} {farm.failed} {farm.read_at ? t.whatStays : ''}
          </Banner>
        ))}
      {inForce.farms
        .filter((farm) => farm.left_out.length > 0)
        .map((farm) => (
          <Banner state="warning" live="polite" key={`left out ${farm.name}`}>
            {t.leftOut(farm.name, farm.left_out.join(', '))}
          </Banner>
        ))}
      <p className="inforce">
        {!nothing && `${t.since(moment(inForce.since))} `}
        {t.toChange[tab]}
      </p>
      {tab === 'machines' && (
        <>
          <div className="listhead">
            <Input
              type="search"
              inputSize="sm"
              className="find"
              leading={<Search size={14} aria-hidden="true" />}
              placeholder={t.filter}
              aria-label={t.filter}
              value={sought}
              onChange={(event) => setSought(event.target.value)}
            />
          </div>
          <Table<Written>
            dense
            aria-label={t.tabs.machines}
            rowKey={(machine) => machine.name}
            columns={[
              { key: 'name', header: t.machine, render: (machine) => <span className="asis strong">{machine.name}</span> },
              { key: 'kind', header: t.whatItIs, render: (machine) => whatItIs(machine) },
              { key: 'groups', header: t.groups, render: (machine) => machine.groups.join(', ') || '—' },
              {
                key: 'given_by',
                header: t.reachedBy,
                render: (machine) =>
                  machine.given_by.length > 0 ? (
                    <ul className="plain">
                      {machine.given_by.map((given) => (
                        <li key={given.rule}>{t.givesTo(given.rule, given.who.join(', '))}</li>
                      ))}
                    </ul>
                  ) : (
                    t.nobody
                  ),
              },
              { key: 'file', header: t.file, render: (machine) => <Code>{`${machine.file}:${machine.line}`}</Code> },
            ]}
            rows={machines}
            empty={inForce.machines.length === 0 ? t.noMachines : t.noMatch}
          />
        </>
      )}
      {tab === 'farms' && (
        <Table<FarmState>
          dense
          aria-label={t.tabs.farms}
          rowKey={(farm) => farm.name}
          columns={[
            { key: 'name', header: t.farm, render: (farm) => <span className="asis strong">{farm.name}</span> },
            { key: 'feed', header: t.feed, render: (farm) => <Code>{farm.feed}</Code> },
            { key: 'groups', header: t.groups, render: (farm) => farm.groups.join(', ') || '—' },
            { key: 'programs', header: t.publishes, render: (farm) => (farm.read_at ? t.published(farm.desktops, farm.programs) : '—') },
            { key: 'read_at', header: t.lastRead, render: (farm) => (farm.read_at ? moment(farm.read_at) : t.notRead) },
            { key: 'file', header: t.file, render: (farm) => <Code>{`${farm.file}:${farm.line}`}</Code> },
          ]}
          rows={inForce.farms}
          empty={t.noFarms}
        />
      )}
      {tab === 'passwords' && <Passwords />}
      {tab === 'rules' && (
        <Table<Rule>
          dense
          aria-label={t.tabs.rules}
          rowKey={(rule) => rule.name}
          columns={[
            { key: 'name', header: t.rule, render: (rule) => <span className="asis strong">{rule.name}</span> },
            { key: 'who', header: t.who, render: (rule) => rule.who.join(', ') },
            { key: 'reaches', header: t.reaches, render: (rule) => rule.reaches.join(', ') },
            { key: 'file', header: t.file, render: (rule) => <Code>{`${rule.file}:${rule.line}`}</Code> },
          ]}
          rows={inForce.rules}
          empty={t.noRules}
        />
      )}
    </div>
  );
}
