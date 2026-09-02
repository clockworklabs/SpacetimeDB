import { useEffect, useState } from 'react';
import { formatMoney } from '../types';

export interface CartLine {
  itemId: bigint;
  name: string;
  price: number;
  quantity: number;
  stock: number;
}

interface CartPanelProps {
  lines: CartLine[];
  onClose: () => void;
  onChangeQuantity: (itemId: bigint, quantity: number) => Promise<void>;
  onRemove: (itemId: bigint) => Promise<void>;
  onCheckout: () => Promise<void>;
  reservations: readonly { itemId: bigint; expiresMicros: bigint; expired: boolean }[];
  onApplyPromotion: (code: string) => Promise<void> | void;
}

export default function CartPanel({
  lines,
  onClose,
  onChangeQuantity,
  onRemove,
  onCheckout,
  reservations,
  onApplyPromotion,
}: CartPanelProps) {
  const [error, setError] = useState<string | null>(null);
  const [checkingOut, setCheckingOut] = useState(false);
  const [promotionCode, setPromotionCode] = useState('');
  const [promotionError, setPromotionError] = useState<string | null>(null);
  const [, setClock] = useState(0);

  useEffect(() => {
    const timer = window.setInterval(() => setClock(value => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const total = lines.reduce((sum, l) => sum + l.price * l.quantity, 0);

  const handleCheckout = async () => {
    setError(null);
    setCheckingOut(true);
    try {
      await onCheckout();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Checkout failed.');
    } finally {
      setCheckingOut(false);
    }
  };

  const handleQuantity = async (itemId: bigint, value: number) => {
    setError(null);
    try {
      await onChangeQuantity(itemId, value);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not update quantity.');
    }
  };

  return (
    <>
      <div className="backdrop" onClick={onClose} />
      <div
        className="panel"
        data-role="cart-panel"
        onKeyDown={(e) => {
          if (e.key === 'Escape') onClose();
        }}
      >
        <div className="panel-header">
          <h2>Your cart</h2>
          <button type="button" className="close-btn" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="panel-body">
          {lines.length === 0 && (
            <div className="empty-state" data-role="empty-cart">
              Your cart is empty. Add something you like!
            </div>
          )}
          {lines.map((line) => (
            <div
              className="cart-item"
              data-role="cart-item"
              data-cart-input={JSON.stringify({ itemId: line.itemId.toString(), quantity: -3 })}
              key={String(line.itemId)}
            >
              <div className="cart-item-info">
                <div className="cart-item-name">{line.name}</div>
                <div className="cart-item-price">{formatMoney(line.price)} each</div>
              </div>
              <input
                type="number"
                className="cart-quantity"
                data-role="cart-quantity"
                min={1}
                value={line.quantity}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (Number.isFinite(v) && v >= 1) handleQuantity(line.itemId, v);
                }}
              />
              <button
                type="button"
                className="btn btn-danger btn-sm"
                data-role="cart-remove"
                onClick={() => onRemove(line.itemId)}
              >
                Remove
              </button>
              {(() => {
                const reservation = reservations.find(row => row.itemId === line.itemId);
                if (!reservation || reservation.expired) return <span data-role="cart-item-expired">expired</span>;
                const seconds = Math.max(0, Number((reservation.expiresMicros - BigInt(Date.now()) * 1000n) / 1_000_000n));
                return <span data-role="cart-reservation-timer">{seconds}</span>;
              })()}
            </div>
          ))}
          {error && (
            <div className="error-text" data-role="buy-error">
              {error}
            </div>
          )}
        </div>
        <div className="panel-footer">
          <div className="inline-form">
            <input data-role="cart-promotion" value={promotionCode} onChange={event => setPromotionCode(event.target.value)} placeholder="Promotion code" />
            <button className="btn btn-ghost btn-sm" data-role="apply-promotion" onClick={async () => {
              setPromotionError(null);
              try { await onApplyPromotion(promotionCode); }
              catch (error) { setPromotionError(error instanceof Error ? error.message : 'Promotion refused.'); }
            }}>Apply</button>
          </div>
          {promotionError && <div className="error-text" data-role="promotion-error">{promotionError}</div>}
          <div className="cart-total-row">
            <span>Total</span>
            <span data-role="cart-total">{formatMoney(total)}</span>
          </div>
          <button
            type="button"
            className="btn btn-primary"
            data-role="checkout-submit"
            disabled={lines.length === 0 || checkingOut}
            style={{ width: '100%', marginTop: 12 }}
            onClick={handleCheckout}
          >
            Checkout
          </button>
        </div>
      </div>
    </>
  );
}
