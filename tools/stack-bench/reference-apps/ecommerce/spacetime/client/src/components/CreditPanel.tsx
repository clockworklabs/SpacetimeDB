import { useState } from 'react';
import { useTable } from 'spacetimedb/react';
import { tables, type DbConnection } from '../module_bindings';

export default function CreditPanel({ conn, staff }: { conn: DbConnection | null; staff: boolean }) {
  const [wallets] = useTable(tables.myCredit);
  const [entries] = useTable(tables.myCreditEntries);
  const [customers] = useTable(tables.creditCustomers);
  const [open, setOpen] = useState(false);
  const [customer, setCustomer] = useState('');
  const [amount, setAmount] = useState('');
  const [reference, setReference] = useState('');
  const [error, setError] = useState('');
  const wallet = wallets[0];
  const accountId = customers.find(row => row.name === customer)?.accountId ?? customers[0]?.accountId ?? 0n;
  return <section>
    <button data-role="credit-link" onClick={() => setOpen(true)}>Store credit</button>
    {open && <div data-role="credit-panel" data-account-id={wallet ? String(wallet.accountId) : ''} aria-busy={!wallet}>
      {error && <p role="alert">{error}</p>}
      <span data-role="credit-balance">{((wallet?.amountMinor ?? 0) / 100).toFixed(2)}</span>
      {entries.map(row => <div data-role="credit-entry" key={String(row.id)}>{row.reference}: {(row.amountMinor / 100).toFixed(2)}</div>)}
      {staff && <div>
        <input data-role="credit-customer" aria-label="Customer name" value={customer} onChange={event => setCustomer(event.target.value)} />
        <input data-role="credit-amount-input" aria-label="Credit amount" value={amount} onChange={event => setAmount(event.target.value)} />
        <input data-role="credit-reference-input" aria-label="Credit reference" value={reference} onChange={event => setReference(event.target.value)} />
        <button data-role="credit-grant" data-action-input={JSON.stringify({ accountId: String(accountId), amountMinor: Math.round(Number(amount) * 100), reference })}
          onClick={() => conn?.reducers.grantCredit({ accountId, amountMinor: Math.round(Number(amount) * 100), reference }).catch(error => setError(String(error)))}>Issue credit</button>
      </div>}
    </div>}
  </section>;
}
