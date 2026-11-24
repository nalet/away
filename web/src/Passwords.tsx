// The passwords Away keeps for machines: each name a machine's entry gives, the machines
// that give it, and whether a password is kept for it, since when and by whom. A password
// is typed once and never shown again: the page seals it to the issuer's key before it
// leaves, and it can only be replaced or removed.
import { Banner, Button, EmptyState, Field, Input, Modal, Table } from '@nalet/design-system';
import { useState } from 'react';

import { forget, keep, useKept } from './kept';
import type { KeptSecret } from './kept';
import { texts } from './texts';

function moment(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'long', timeStyle: 'short' });
}

export function Passwords() {
  const [kept, again] = useKept();
  const [typing, setTyping] = useState<KeptSecret | null>(null);
  const [removing, setRemoving] = useState<KeptSecret | null>(null);
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const t = texts.administration.passwords;

  if (kept === 'asking') return <div className="fill" aria-busy="true" />;
  if (kept === 'unanswered') {
    return <EmptyState illustration="error" size="md" headingLevel={2} title={t.unanswered} actions={<Button variant="primary" onClick={again}>{texts.administration.again}</Button>} />;
  }
  const close = () => {
    setTyping(null);
    setRemoving(null);
    setPassword('');
    setFailed(null);
    setBusy(false);
  };
  const save = async () => {
    if (!typing || !kept.key || password === '') return;
    setBusy(true);
    setFailed(null);
    try {
      const status = await keep(kept.key, typing.name, password);
      if (status === 200) {
        close();
        again();
        return;
      }
      setFailed(status === 409 ? t.keyChanged : t.notKept(status));
      if (status === 409) again();
    } catch {
      setFailed(t.notSealed);
    }
    setBusy(false);
  };
  const remove = async () => {
    if (!removing) return;
    setBusy(true);
    const status = await forget(removing.name).catch(() => 0);
    if (status === 200 || status === 404) {
      close();
      again();
      return;
    }
    setFailed(t.notRemoved(status));
    setBusy(false);
  };

  return (
    <>
      {!kept.keeps && (
        <Banner state="warning" live="polite">
          {t.keepsNothing}
        </Banner>
      )}
      <p className="inforce">{t.how}</p>
      <Table<KeptSecret>
        dense
        aria-label={t.title}
        rowKey={(secret) => secret.name}
        columns={[
          { key: 'name', header: t.name, render: (secret) => <span className="asis strong">{secret.name}</span> },
          { key: 'machines', header: t.machines, render: (secret) => secret.machines.join(', ') || t.namedByNone },
          {
            key: 'kept',
            header: t.state,
            render: (secret) => (secret.kept && secret.since !== null ? t.keptSince(moment(secret.since), secret.by ?? '') : t.notYet),
          },
          {
            key: 'by',
            header: <span className="unseen">{t.actions}</span>,
            render: (secret) => (
              <span className="acts">
                <Button size="sm" variant="default" disabled={!kept.keeps} onClick={() => setTyping(secret)}>
                  {secret.kept ? t.replace : t.keep}
                </Button>
                {secret.kept && (
                  <Button size="sm" variant="ghost" onClick={() => setRemoving(secret)}>
                    {t.remove}
                  </Button>
                )}
              </span>
            ),
          },
        ]}
        rows={kept.secrets}
        empty={t.none}
      />
      <Modal
        open={typing !== null}
        onClose={close}
        title={typing ? t.keepFor(typing.name) : ''}
        footer={
          <>
            <Button variant="ghost" onClick={close}>
              {t.cancel}
            </Button>
            <Button variant="primary" disabled={busy || password === ''} onClick={save}>
              {t.save}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <p>{typing ? t.forMachines(typing.machines.join(', ') || t.namedByNone) : ''}</p>
          <Field label={t.password} hint={t.neverShown} error={failed ?? undefined}>
            <Input type="password" autoComplete="new-password" value={password} onChange={(event) => setPassword(event.target.value)} autoFocus />
          </Field>
        </form>
      </Modal>
      <Modal
        open={removing !== null}
        onClose={close}
        title={removing ? t.removeFor(removing.name) : ''}
        footer={
          <>
            <Button variant="ghost" onClick={close}>
              {t.cancel}
            </Button>
            <Button variant="primary" disabled={busy} onClick={remove}>
              {t.remove}
            </Button>
          </>
        }
      >
        <p>{removing ? t.removeWhat(removing.machines.join(', ') || t.namedByNone) : ''}</p>
        {failed && <p role="alert">{failed}</p>}
      </Modal>
    </>
  );
}
