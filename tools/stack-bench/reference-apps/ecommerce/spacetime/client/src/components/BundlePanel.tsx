import { useState } from 'react';
import type { DbConnection } from '../module_bindings';

export default function BundlePanel({ conn, canManage, signedIn, bundles }: {
  conn: DbConnection | null; canManage: boolean; signedIn: boolean;
  bundles: Array<{ id: bigint; name: string; price: number; components: Array<{ item: string; quantity: number }> }>;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [price, setPrice] = useState('');
  const [componentsJson, setComponents] = useState('[]');
  const [error, setError] = useState('');
  async function write(action: () => Promise<unknown> | undefined) {
    try { setError(''); await action(); } catch (error) { setError(String(error)); }
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
          onClick={() => write(() => conn?.reducers.saveBundle({ name, price: Number(price), componentsJson }))}>Save bundle</button>
      </div>}
      {bundles.map(bundle => <div key={String(bundle.id)} data-role="bundle-card" data-bundle-input={JSON.stringify({ bundleId: String(bundle.id) })}>
        <span data-role="bundle-name">{bundle.name}</span> <span data-role="bundle-price">{bundle.price.toFixed(2)}</span>
        {bundle.components.map(component => <div key={component.item} data-role="bundle-component" data-quantity={component.quantity}>
          <span data-role="bundle-component-name">{component.item}</span> <span data-role="bundle-component-quantity">{component.quantity}</span>
        </div>)}
        {signedIn && <button data-role="bundle-add-to-cart" onClick={() => write(() => conn?.reducers.addBundleToCart({ bundleId: bundle.id }))}>Add bundle</button>}
      </div>)}
    </div>}
  </section>;
}
