import { useMemo, useState, type FormEvent } from 'react';
import { Identity, type Timestamp } from 'spacetimedb';
import { useReducer, useSpacetimeDB, useTable } from 'spacetimedb/react';
import './App.css';
import { reducers, tables } from './module_bindings';
import { formatMoney, parseMoney } from './money';

type DirectoryEntry = {
  identity: Identity;
  name: string;
};

type AccountChangeEntry = {
  id: bigint;
  accountIdentity: Identity;
  counterpartyIdentity: Identity;
  direction: { tag: 'Credit' } | { tag: 'Debit' };
  amountCents: bigint;
  createdAt: Timestamp;
};

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : 'The request failed.';
}

function shortIdentity(identity: Identity) {
  return identity.toHexString().slice(0, 8);
}

export function ActivityFeed({
  changes,
  directory,
}: {
  changes: readonly AccountChangeEntry[];
  directory: readonly DirectoryEntry[];
}) {
  const names = new Map(
    directory.map(entry => [entry.identity.toHexString(), entry.name])
  );
  const sorted = [...changes].sort((left, right) =>
    Number(
      right.createdAt.microsSinceUnixEpoch - left.createdAt.microsSinceUnixEpoch
    )
  );

  if (sorted.length === 0) {
    return (
      <p className="empty">No account changes yet. Send your first payment.</p>
    );
  }

  return (
    <ol className="activity-list">
      {sorted.map(change => {
        const debit = change.direction.tag === 'Debit';
        const counterparty = change.counterpartyIdentity;
        const name =
          names.get(counterparty.toHexString()) ?? shortIdentity(counterparty);
        return (
          <li key={change.id.toString()}>
            <div>
              <strong>
                {debit ? `Sent to ${name}` : `Received from ${name}`}
              </strong>
              <time>{change.createdAt.toDate().toLocaleString()}</time>
            </div>
            <span className={debit ? 'outgoing' : 'incoming'}>
              {debit ? '-' : '+'}
              {formatMoney(change.amountCents)}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

function NameForm({
  initialName = '',
  onSubmit,
}: {
  initialName?: string;
  onSubmit: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState(initialName);
  const [saving, setSaving] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim()) return;
    setSaving(true);
    try {
      await onSubmit(name);
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="name-form" onSubmit={submit}>
      <label htmlFor="display-name">Nickname</label>
      <div>
        <input
          autoFocus
          id="display-name"
          maxLength={20}
          onChange={event => setName(event.target.value)}
          placeholder="Choose a unique name"
          value={name}
        />
        <button disabled={!name.trim() || saving} type="submit">
          {saving ? 'Saving...' : 'Save name'}
        </button>
      </div>
    </form>
  );
}

function App() {
  const { identity, isActive: connected } = useSpacetimeDB();
  const [accounts] = useTable(tables.myAccount);
  const [directory] = useTable(tables.directory);
  const [changes] = useTable(tables.myAccountChanges);
  const setName = useReducer(reducers.setName);
  const sendTransfer = useReducer(reducers.transfer);
  const [editingName, setEditingName] = useState(false);
  const [recipientId, setRecipientId] = useState('');
  const [amount, setAmount] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');

  const me = directory.find(row => identity?.isEqual(row.identity));
  const account = accounts[0];
  const availableRecipients = useMemo(
    () =>
      directory
        .filter(row => !identity?.isEqual(row.identity))
        .sort((left, right) => left.name.localeCompare(right.name)),
    [identity, directory]
  );

  const saveName = async (name: string) => {
    setError('');
    try {
      await setName({ name });
      setEditingName(false);
    } catch (caught) {
      setError(errorMessage(caught));
    }
  };

  const submitTransfer = async (event: FormEvent) => {
    event.preventDefault();
    const amountCents = parseMoney(amount);
    const destination = availableRecipients.find(
      row => row.identity.toHexString() === recipientId
    );
    if (!destination) {
      setError('Choose a recipient.');
      return;
    }
    if (!amountCents) {
      setError('Enter a positive amount with no more than two decimals.');
      return;
    }
    setError('');
    setSubmitting(true);
    try {
      await sendTransfer({
        recipient: destination.identity,
        amountCents,
      });
      setAmount('');
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setSubmitting(false);
    }
  };

  if (!connected || !identity || !account) {
    return (
      <main className="loading">
        <p className="eyebrow">SpacetimeDB sample</p>
        <h1>Money Exchange</h1>
        <p>Opening your private account...</p>
      </main>
    );
  }

  if (!me) {
    return (
      <main className="welcome">
        <section className="welcome-card">
          <p className="eyebrow">SpacetimeDB sample</p>
          <h1>Money Exchange</h1>
          <p>
            You received <strong>{formatMoney(account.balanceCents)}</strong>.
            Choose a unique nickname to start exchanging money.
          </p>
          {error && <p className="error">{error}</p>}
          <NameForm onSubmit={saveName} />
        </section>
      </main>
    );
  }

  return (
    <main className="app">
      <header>
        <div>
          <p className="eyebrow">SpacetimeDB sample</p>
          <h1>Money Exchange</h1>
        </div>
        {editingName ? (
          <NameForm initialName={me.name} onSubmit={saveName} />
        ) : (
          <div className="profile">
            <span>{me.name}</span>
            <button
              className="secondary"
              onClick={() => setEditingName(true)}
              type="button"
            >
              Rename
            </button>
          </div>
        )}
      </header>

      {error && (
        <p className="error banner" role="alert">
          {error}
        </p>
      )}

      <section className="dashboard">
        <section className="balance-card">
          <p>Your private account balance</p>
          <strong>{formatMoney(account.balanceCents)}</strong>
          <small>Only you can see this balance and your activity.</small>
        </section>

        <form className="send-card" onSubmit={submitTransfer}>
          <h2>Send money</h2>
          <label htmlFor="recipient">Recipient</label>
          <select
            id="recipient"
            onChange={event => setRecipientId(event.target.value)}
            value={recipientId}
          >
            <option value="">Choose a recipient</option>
            {availableRecipients.map(recipient => (
              <option
                key={recipient.identity.toHexString()}
                value={recipient.identity.toHexString()}
              >
                {recipient.name}
              </option>
            ))}
          </select>
          <label htmlFor="amount">Amount</label>
          <div className="amount">
            <span>$</span>
            <input
              id="amount"
              inputMode="decimal"
              onChange={event => setAmount(event.target.value)}
              placeholder="0.00"
              value={amount}
            />
          </div>
          <button disabled={submitting} type="submit">
            {submitting ? 'Sending...' : 'Send payment'}
          </button>
        </form>
      </section>

      <section className="activity">
        <div>
          <p className="eyebrow">Private account history</p>
          <h2>Your balance changes</h2>
        </div>
        <ActivityFeed changes={changes} directory={directory} />
      </section>
    </main>
  );
}

export default App;
