import { request } from './request';
import { useEffect, useState } from 'react';

type Bundle = { id: string; name: string; price: number; components: Array<{ item: string; quantity: number }> };
export function BundlePanel({ token, canManage, onAdded }: {
  token: string | null; canManage: boolean; onAdded: () => Promise<void>;
}) {
  const [open, setOpen] = useState(false);
  const [bundles, setBundles] = useState<Bundle[]>([]);
  const [name, setName] = useState('');
  const [price, setPrice] = useState('');
  const [componentsJson, setComponents] = useState('[]');
  const [error, setError] = useState('');
  const refresh = async () => setBundles(await request('/api/bundles', token));
  useEffect(() => { if (open) void refresh().catch(error => setError(String(error))); }, [open, token]);
  async function write(path: string, body: unknown) {
    try { setError(''); await request(path, token, { method: 'POST', body: JSON.stringify(body) }); await refresh(); await onAdded(); }
    catch (error) { setError(String(error)); }
  }
  return <section>
    <button data-role="bundles-link" onClick={() => setOpen(true)}>Bundles</button>
    {open && <div>
      {error && <p role="alert">{error}</p>}
      {canManage && <div>
        <input aria-label="Bundle name" data-role="bundle-name-input" value={name} onChange={event => setName(event.target.value)} />
        <input aria-label="Bundle price" data-role="bundle-price-input" value={price} onChange={event => setPrice(event.target.value)} />
        <textarea aria-label="Bundle components" data-role="bundle-components-input" value={componentsJson} onChange={event => setComponents(event.target.value)} />
        <button data-role="bundle-save" data-bundle-save-input={JSON.stringify({ name, price: Number(price), componentsJson })}
          onClick={() => write('/api/bundles', { name, price: Number(price), componentsJson })}>Save bundle</button>
      </div>}
      {bundles.map(bundle => <div key={bundle.id} data-role="bundle-card" data-bundle-input={JSON.stringify({ bundleId: bundle.id })}>
        <span data-role="bundle-name">{bundle.name}</span> <span data-role="bundle-price">{Number(bundle.price).toFixed(2)}</span>
        {bundle.components.map(component => <div key={component.item} data-role="bundle-component" data-quantity={component.quantity}>
          <span data-role="bundle-component-name">{component.item}</span> <span data-role="bundle-component-quantity">{component.quantity}</span>
        </div>)}
        {token && <button data-role="bundle-add-to-cart" onClick={() => write('/api/cart/bundles', { bundleId: bundle.id })}>Add bundle</button>}
      </div>)}
    </div>}
  </section>;
}
