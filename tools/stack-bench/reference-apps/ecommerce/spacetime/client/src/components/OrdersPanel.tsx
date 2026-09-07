import { useState } from 'react';
import { formatMoney } from '../types';

export interface OrderItemView {
  itemId: bigint;
  name: string;
  quantity: number;
  returned: boolean;
}

export interface OrderView {
  orderId: bigint;
  createdAt: Date;
  total: number;
  status: string;
  discount: number;
  refundedTotal: number;
  payments: { amount: number; status: string }[];
  items: OrderItemView[];
}

interface OrdersPanelProps {
  orders: OrderView[];
  onClose: () => void;
  onCancel: (orderId: bigint) => Promise<void>;
  onReturn: (orderId: bigint, itemId: bigint) => Promise<void>;
}

export default function OrdersPanel({ orders, onClose, onCancel, onReturn }: OrdersPanelProps) {
  const [errors, setErrors] = useState<Record<string, string>>({});

  const handleCancel = async (orderId: bigint) => {
    const k = String(orderId);
    setErrors((e) => ({ ...e, [k]: '' }));
    try {
      await onCancel(orderId);
    } catch (err) {
      setErrors((e) => ({ ...e, [k]: err instanceof Error ? err.message : 'Could not cancel order.' }));
    }
  };

  const handleReturn = async (orderId: bigint, itemId: bigint) => {
    const k = String(orderId);
    setErrors((e) => ({ ...e, [k]: '' }));
    try {
      await onReturn(orderId, itemId);
    } catch (err) {
      setErrors((e) => ({ ...e, [k]: err instanceof Error ? err.message : 'Could not return item.' }));
    }
  };

  return (
    <>
      <div className="backdrop" onClick={onClose} />
      <div
        className="panel"
        onKeyDown={(e) => {
          if (e.key === 'Escape') onClose();
        }}
      >
        <div className="panel-header">
          <h2>Order history</h2>
          <button type="button" className="close-btn" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="panel-body" data-role="order-list">
          {orders.length === 0 && <div className="empty-state">You have no past orders yet.</div>}
          {orders.map((order) => (
            <div
              className="order-item"
              data-role="order-item"
              data-entity-id={String(order.orderId)}
              data-ship-input={JSON.stringify({ orderId: Number(order.orderId) })}
              data-cancel-input={JSON.stringify({ orderId: Number(order.orderId) })}
              key={String(order.orderId)}
            >
              <div className="order-item-names">{order.items.map((i) => i.name).join(', ')}</div>
              <div className="order-item-meta">{order.createdAt.toLocaleString()}</div>
              <div className="order-item-row">
                <span className="order-status" data-role="order-status">
                  {order.status}
                </span>
                <span className="order-total" data-role="order-total">
                  {formatMoney(order.total)}
                </span>
              </div>
              {order.discount > 0 && <div data-role="order-discount">Discount: {formatMoney(order.discount)}</div>}
              {order.refundedTotal > 0 && <div data-role="order-refund-total">Refund: {formatMoney(order.refundedTotal)}</div>}
              {order.payments.map((payment, index) => payment.status === 'refunded' ? (
                <div data-role="refund-entry" key={`refund-${index}`}>
                  {order.items.map(item => item.name).join(', ')}
                  <span data-role="payment-amount">{formatMoney(payment.amount)}</span>
                  <span data-role="payment-status">{payment.status}</span>
                </div>
              ) : (
                <div data-role="payment-record" key={`payment-${index}`}>
                  {order.items.map(item => item.name).join(', ')}
                  <span data-role="payment-amount">{formatMoney(payment.amount)}</span>
                  <span data-role="payment-status">{payment.status}</span>
                </div>
              ))}
              <div className="order-item-lines">
                {order.items.map((item) => (
                  <div className="order-item-line" key={String(item.itemId)}>
                    <span>
                      {item.name} × {item.quantity}
                      {item.returned && <span className="badge badge-muted" style={{ marginLeft: 6 }}>Returned</span>}
                    </span>
                    {order.status === 'shipped' && !item.returned && (
                      <button
                        type="button"
                        className="btn btn-ghost btn-sm"
                        data-role="return-item"
                        onClick={() => handleReturn(order.orderId, item.itemId)}
                      >
                        Return
                      </button>
                    )}
                  </div>
                ))}
              </div>
              {order.status === 'pending' && (
                <button
                  type="button"
                  className="btn btn-danger btn-sm"
                  data-role="cancel-order"
                  onClick={() => handleCancel(order.orderId)}
                >
                  Cancel order
                </button>
              )}
              {errors[String(order.orderId)] && (
                <div className="error-text" data-role="order-error">
                  {errors[String(order.orderId)]}
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
