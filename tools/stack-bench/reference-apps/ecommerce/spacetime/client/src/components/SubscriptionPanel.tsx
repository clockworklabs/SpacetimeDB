import { useState } from 'react';
import { useTable } from 'spacetimedb/react';
import { type DbConnection, tables } from '../module_bindings';

export default function SubscriptionPanel({ conn, signedIn }: { conn: DbConnection | null; signedIn: boolean }) {
  const [rows, ready] = useTable(tables.mySubscriptions);
  const [open, setOpen] = useState(false);
  const [item, setItem] = useState('');
  const [quantity, setQuantity] = useState('1');
  const [interval, setIntervalValue] = useState('30');
  const [deliveries, setDeliveries] = useState('2');
  const [error, setError] = useState('');
  async function write(action: () => Promise<unknown>) {
    try { setError(''); await action(); } catch (error) { setError(String(error)); }
  }
  if (!signedIn || !conn) return null;
  return <section>
    <button data-role="subscriptions-link" onClick={() => setOpen(true)}>Subscriptions</button>
    {open && <div data-role="subscriptions-panel" aria-busy={!ready}>
      {error && <p role="alert">{error}</p>}
      <input aria-label="Subscription item" data-role="subscription-item-input" value={item} onChange={event => setItem(event.target.value)} />
      <input aria-label="Subscription quantity" type="number" min="1" data-role="subscription-quantity-input" value={quantity} onChange={event => setQuantity(event.target.value)} />
      <input aria-label="Delivery interval in seconds" type="number" min="30" data-role="subscription-interval-input" value={interval} onChange={event => setIntervalValue(event.target.value)} />
      <input aria-label="Number of deliveries" type="number" min="1" max="12" data-role="subscription-deliveries-input" value={deliveries} onChange={event => setDeliveries(event.target.value)} />
      <button data-role="subscription-create" onClick={() => void write(() => conn.reducers.subscribeItem({
        item, quantity: Number(quantity), intervalSeconds: Number(interval), deliveries: Number(deliveries),
      }))}>Subscribe</button>
      {rows.map(row => <article key={String(row.id)} data-role="subscription-row">
        {row.item} <span data-role="subscription-status">{row.status}</span>
        <span data-role="subscription-total">{row.total.toFixed(2)}</span>
        {row.deliveries.map((status, index) => <div key={index} data-role="subscription-delivery">
          <span data-role="subscription-delivery-status">{status}</span>
        </div>)}
        <button data-role="subscription-pause" data-action-input={JSON.stringify({ subscriptionId: String(row.id) })}
          onClick={() => void write(() => conn.reducers.pauseSubscription({ subscriptionId: row.id }))}>Pause</button>
        <button data-role="subscription-resume" data-action-input={JSON.stringify({ subscriptionId: String(row.id) })}
          onClick={() => void write(() => conn.reducers.resumeSubscription({ subscriptionId: row.id }))}>Resume</button>
        <button data-role="subscription-cancel" data-action-input={JSON.stringify({ subscriptionId: String(row.id) })}
          onClick={() => void write(() => conn.reducers.cancelSubscription({ subscriptionId: row.id }))}>Cancel</button>
      </article>)}
    </div>}
  </section>;
}
