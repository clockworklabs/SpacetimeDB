import { ItemRow } from '../types';
import { formatMoney } from '../types';

interface ItemCardProps {
  item: ItemRow;
  stock: number;
  isSignedIn: boolean;
  onOpen: (itemId: bigint) => void;
  onBuyNow: (itemId: bigint) => void;
  onAddToCart: (itemId: bigint) => void;
  onStockAlert: (itemId: bigint) => void;
  variants: readonly string[];
}

export default function ItemCard({
  item,
  stock,
  isSignedIn,
  onOpen,
  onBuyNow,
  onAddToCart,
  onStockAlert,
  variants,
}: ItemCardProps) {
  const outOfStock = stock <= 0;
  const lowStock = !outOfStock && stock <= 5;

  return (
    <div className={`item-card${outOfStock ? ' out-of-stock-card' : ''}`} data-role="item-card" data-buy-input={JSON.stringify({ itemId: Number(item.id) })}>
      <button
        type="button"
        className="item-card-name"
        data-role="item-name"
        onClick={() => onOpen(item.id)}
      >
        {item.name}
      </button>
      <div className="item-card-row">
        <span>SKU {String(item.id)}</span>
        <span className="item-price" data-role="item-price">
          {formatMoney(item.price)}
        </span>
      </div>
      {variants.map(variant => (
        <span className="badge badge-muted" data-role="item-variant" key={variant}>{variant}</span>
      ))}
      <div className="item-card-row">
        <span>
          Stock:{' '}
          <span
            className={`item-stock${outOfStock ? ' stock-zero' : lowStock ? ' stock-low' : ''}`}
            data-role="item-stock"
          >
            {stock}
          </span>
        </span>
        {outOfStock && (
          <span className="badge badge-danger" data-role="out-of-stock">
            Out of stock
          </span>
        )}
      </div>
      {isSignedIn && (
        <div className="item-card-actions">
          <button
            type="button"
            className="btn btn-primary"
            data-role="buy-now"
            disabled={outOfStock}
            onClick={() => onBuyNow(item.id)}
          >
            Buy now
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            data-role="add-to-cart"
            disabled={outOfStock}
            onClick={() => onAddToCart(item.id)}
          >
            Add to cart
          </button>
        </div>
      )}
      {isSignedIn && outOfStock && (
        <button type="button" className="btn btn-ghost" data-role="stock-alert" onClick={() => onStockAlert(item.id)}>
          Alert me
        </button>
      )}
    </div>
  );
}
