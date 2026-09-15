import { useState } from 'react';

type Entry = { id: string; name: string; address: string; isDefault: boolean };
export function AddressBook({ actions }: { actions: {
  load: () => Promise<{ entries: Entry[] }>;
  save: (entry: { id?: string; name: string; address: string }) => Promise<unknown>;
  remove: (id: string) => Promise<unknown>;
  choose: (id: string) => Promise<unknown>;
} }) {
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [entries, setEntries] = useState<Entry[]>([]);
  const [editor, setEditor] = useState<{ id?: string; name: string; address: string } | null>(null);
  const [state, setState] = useState('idle');
  const [error, setError] = useState('');
  const load = async () => {
    setLoaded(false);
    const result = await actions.load();
    setEntries(result.entries); setLoaded(true);
  };
  const write = async (work: () => Promise<unknown>) => {
    setState('pending'); setError('');
    try { await work(); await load(); setEditor(null); setState('succeeded'); }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Request failed'); setState('failed'); }
  };
  return <>
    <button data-role="address-book-link" onClick={async () => {
      setOpen(true); setError('');
      try { await load(); } catch (cause) { setError(cause instanceof Error ? cause.message : 'Read failed'); }
    }}>Address book</button>
    {open && <section data-role="address-book" data-loaded={loaded} data-submit-state={state}>
      <h3>Addresses</h3>
      {error && <p data-role="address-error" role="alert">{error}</p>}
      {loaded && entries.map(entry => <article key={entry.id} data-role="address-entry"
        data-address-id={entry.id} data-default={entry.isDefault}>
        <p data-role="address-name">{entry.name}</p><p data-role="address-text">{entry.address}</p>
        <button data-role="address-edit" onClick={() => setEditor(entry)}>Edit</button>
        <button data-role="address-default" onClick={() => void write(() => actions.choose(entry.id))}>Make default</button>
        <button data-role="address-delete" onClick={() => void write(() => actions.remove(entry.id))}>Delete</button>
      </article>)}
      {loaded && <button data-role="address-add" onClick={() => setEditor({ name: '', address: '' })}>Add address</button>}
      {editor && <div>
        <label>Recipient<input data-role="address-name-input" value={editor.name}
          onChange={event => setEditor({ ...editor, name: event.target.value })} /></label>
        <label>Address<textarea data-role="address-text-input" value={editor.address}
          onChange={event => setEditor({ ...editor, address: event.target.value })} /></label>
        <button data-role="address-save" disabled={state === 'pending'} onClick={() => void write(() => actions.save(editor))}>Save</button>
        <button data-role="address-cancel" onClick={() => setEditor(null)}>Cancel</button>
      </div>}
    </section>}
  </>;
}
