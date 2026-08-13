import { useState } from 'react';
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
}

export default function CartPanel({
  lines,
  onClose,
  onChangeQuantity,
  onRemove,
  onCheckout,
}: CartPanelProps) {
  const [error, setError] = useState<string | null>(null);
  const [checkingOut, setCheckingOut] = useState(false);

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
        data-testid="cart-panel"
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
            <div className="empty-state" data-testid="empty-cart">
              Your cart is empty. Add something you like!
            </div>
          )}
          {lines.map((line) => (
            <div className="cart-item" data-testid="cart-item" key={String(line.itemId)}>
              <div className="cart-item-info">
                <div className="cart-item-name">{line.name}</div>
                <div className="cart-item-price">{formatMoney(line.price)} each</div>
              </div>
              <input
                type="number"
                className="cart-quantity"
                data-testid="cart-quantity"
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
                data-testid="cart-remove"
                onClick={() => onRemove(line.itemId)}
              >
                Remove
              </button>
            </div>
          ))}
          {error && (
            <div className="error-text" data-testid="buy-error">
              {error}
            </div>
          )}
        </div>
        <div className="panel-footer">
          <div className="cart-total-row">
            <span>Total</span>
            <span data-testid="cart-total">{formatMoney(total)}</span>
          </div>
          <button
            type="button"
            className="btn btn-primary"
            data-testid="checkout-submit"
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
