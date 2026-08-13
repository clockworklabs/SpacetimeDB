import { useState } from 'react';
import { QueueOrderRow } from '../types';

interface FulfilmentPanelProps {
  queue: readonly QueueOrderRow[];
  onShip: (orderId: bigint) => Promise<void>;
}

export default function FulfilmentPanel({ queue, onShip }: FulfilmentPanelProps) {
  const [errors, setErrors] = useState<Record<string, string>>({});

  const handleShip = async (orderId: bigint) => {
    const k = String(orderId);
    setErrors((e) => ({ ...e, [k]: '' }));
    try {
      await onShip(orderId);
    } catch (err) {
      setErrors((e) => ({ ...e, [k]: err instanceof Error ? err.message : 'Could not ship order.' }));
    }
  };

  return (
    <div className="fulfilment-panel" data-testid="fulfilment-panel">
      <div className="fulfilment-header">
        <h2 className="section-title">Fulfilment queue</h2>
        <span className="muted">
          Waiting: <span data-testid="queue-depth">{queue.length}</span>
        </span>
      </div>
      {queue.length === 0 && <div className="empty-state">No orders are waiting to ship.</div>}
      {queue.map((order) => (
        <div className="queue-item" data-testid="queue-item" key={String(order.orderId)}>
          <div className="queue-item-names">{order.itemNames.join(', ')}</div>
          <div className="queue-item-warehouses">
            {order.itemNames.map((name, i) => (
              <span className="badge badge-muted" data-testid="queue-warehouse" key={i}>
                {name}: {order.warehouseNames[i]}
              </span>
            ))}
          </div>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            data-testid="ship-submit"
            onClick={() => handleShip(order.orderId)}
          >
            Mark shipped
          </button>
          {errors[String(order.orderId)] && (
            <div className="error-text" data-testid="order-error">
              {errors[String(order.orderId)]}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
