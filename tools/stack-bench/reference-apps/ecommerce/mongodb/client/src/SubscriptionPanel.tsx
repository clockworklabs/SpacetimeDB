import { request } from './request';
import { useEffect, useState } from 'react';

type Subscription = { id: string | number; item: string; status: string; total: number; deliveries: Array<{ status: string }> };

export function SubscriptionPanel({ signedIn, token }: { signedIn: boolean; token?: string | null }) {
  const [open, setOpen] = useState(false);
  const [rows, setRows] = useState<Subscription[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState('');
  const [item, setItem] = useState('');
  const [quantity, setQuantity] = useState('1');
  const [interval, setIntervalValue] = useState('30');
  const [deliveries, setDeliveries] = useState('2');
  useEffect(() => {
    if (!open || !signedIn) return;
    const controller = new AbortController();
    const refresh = async () => {
      try {
        const response = await fetch('/api/subscriptions', { credentials: 'include', signal: controller.signal,
          headers: token ? { Authorization: `Bearer ${token}` } : {} });
        if (!response.ok) throw new Error('Could not load subscriptions');
        const value = await response.json();
        if (!controller.signal.aborted) { setRows(value); setLoaded(true); }
      } catch (error) { if (!controller.signal.aborted) { setLoaded(false); setError(String(error)); } }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => { controller.abort(); clearInterval(timer); };
  }, [open, signedIn, token]);
  async function write(path: string, body = {}) {
    try {
      setError('');
      await request(path, token ?? null, { method: 'POST', body: JSON.stringify(body) });
    } catch (error) { setError(String(error)); }
  }
  if (!signedIn) return null;
  return <section>
    <button data-role="subscriptions-link" onClick={() => setOpen(true)}>Subscriptions</button>
    {open && <div data-role="subscriptions-panel" aria-busy={!loaded}>
      {error && <p role="alert">{error}</p>}
      <input aria-label="Subscription item" data-role="subscription-item-input" value={item} onChange={event => setItem(event.target.value)} />
      <input aria-label="Subscription quantity" type="number" min="1" data-role="subscription-quantity-input" value={quantity} onChange={event => setQuantity(event.target.value)} />
      <input aria-label="Delivery interval in seconds" type="number" min="30" data-role="subscription-interval-input" value={interval} onChange={event => setIntervalValue(event.target.value)} />
      <input aria-label="Number of deliveries" type="number" min="1" max="12" data-role="subscription-deliveries-input" value={deliveries} onChange={event => setDeliveries(event.target.value)} />
      <button data-role="subscription-create" onClick={() => void write('/api/subscriptions', {
        item, quantity: Number(quantity), intervalSeconds: Number(interval), deliveries: Number(deliveries),
      })}>Subscribe</button>
      {rows.map(row => <article key={row.id} data-role="subscription-row">
        {row.item} <span data-role="subscription-status">{row.status}</span>
        <span data-role="subscription-total">{row.total.toFixed(2)}</span>
        {row.deliveries.map((delivery, index) => <div key={index} data-role="subscription-delivery">
          <span data-role="subscription-delivery-status">{delivery.status}</span>
        </div>)}
        {(['pause', 'resume', 'cancel'] as const).map(action => <button key={action} data-role={`subscription-${action}`}
          data-action-input={JSON.stringify({ subscriptionId: String(row.id) })}
          onClick={() => void write(`/api/subscriptions/${row.id}/${action}`)}>{action}</button>)}
      </article>)}
    </div>}
  </section>;
}
