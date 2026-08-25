import { ItemRow } from '../types';
import { formatMoney } from '../types';

interface ItemCardProps {
  item: ItemRow;
  stock: number;
  isSignedIn: boolean;
  onOpen: (itemId: bigint) => void;
  onBuyNow: (itemId: bigint) => void;
  onAddToCart: (itemId: bigint) => void;
}

export default function ItemCard({
  item,
  stock,
  isSignedIn,
  onOpen,
  onBuyNow,
  onAddToCart,
}: ItemCardProps) {
  const outOfStock = stock <= 0;
  const lowStock = !outOfStock && stock <= 5;

  return (
    <div className={`item-card${outOfStock ? ' out-of-stock-card' : ''}`} data-testid="item-card">
      <button
        type="button"
        className="item-card-name"
        data-testid="item-name"
        onClick={() => onOpen(item.id)}
      >
        {item.name}
      </button>
      <div className="item-card-row">
        <span>SKU {String(item.id)}</span>
        <span className="item-price" data-testid="item-price">
          {formatMoney(item.price)}
        </span>
      </div>
      <div className="item-card-row">
        <span>
          Stock:{' '}
          <span
            className={`item-stock${outOfStock ? ' stock-zero' : lowStock ? ' stock-low' : ''}`}
            data-testid="item-stock"
          >
            {stock}
          </span>
        </span>
        {outOfStock && (
          <span className="badge badge-danger" data-testid="out-of-stock">
            Out of stock
          </span>
        )}
      </div>
      {isSignedIn && (
        <div className="item-card-actions">
          <button
            type="button"
            className="btn btn-primary"
            data-testid="buy-now"
            disabled={outOfStock}
            onClick={() => onBuyNow(item.id)}
          >
            Buy now
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            data-testid="add-to-cart"
            disabled={outOfStock}
            onClick={() => onAddToCart(item.id)}
          >
            Add to cart
          </button>
        </div>
      )}
    </div>
  );
}
