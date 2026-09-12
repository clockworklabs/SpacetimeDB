import { request } from './request';
import { useEffect, useState } from 'react';

export function CreditPanel({ token, staff }: { token: string | null; staff: boolean }) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<any>(null);
  const [customer, setCustomer] = useState('');
  const [amount, setAmount] = useState('');
  const [reference, setReference] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    if (!open || !token) return;
    let active = true;
    const refresh = async () => { try { const value = await request('/api/credit', token); if (active) setState(value); } catch (error) { if (active) setError(String(error)); } };
    void refresh(); const timer = setInterval(refresh, 1000);
    return () => { active = false; clearInterval(timer); };
  }, [open, token]);
  const input = { accountId: state?.customers.find((row: any) => row.name === customer)?.id ?? state?.customers[0]?.id ?? '', amountMinor: Math.round(Number(amount) * 100), reference };
  return <section>
    <button data-role="credit-link" onClick={() => setOpen(true)}>Store credit</button>
    {open && <div data-role="credit-panel" data-account-id={state?.accountId ?? ''} aria-busy={!state}>
      {error && <p role="alert">{error}</p>}
      <span data-role="credit-balance">{Number(state?.balance ?? 0).toFixed(2)}</span>
      {(state?.entries ?? []).map((row: any) => <div data-role="credit-entry" key={row.id}>{row.reference}: {row.amount.toFixed(2)}</div>)}
      {staff && <div>
        <input data-role="credit-customer" aria-label="Customer name" value={customer} onChange={event => setCustomer(event.target.value)} />
        <input data-role="credit-amount-input" aria-label="Credit amount" value={amount} onChange={event => setAmount(event.target.value)} />
        <input data-role="credit-reference-input" aria-label="Credit reference" value={reference} onChange={event => setReference(event.target.value)} />
        <button data-role="credit-grant" data-action-input={JSON.stringify(input)} onClick={() => request('/api/staff/credit', token, { method: 'POST', body: JSON.stringify(input) }).catch(error => setError(String(error)))}>Issue credit</button>
      </div>}
    </div>}
  </section>;
}
