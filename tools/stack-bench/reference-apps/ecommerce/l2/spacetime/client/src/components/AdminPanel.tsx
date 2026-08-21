import { useState } from 'react';
import { ItemRow, WarehouseRow, CategoryTotalRow, formatMoney } from '../types';

interface AdminPanelProps {
  items: readonly ItemRow[];
  warehouses: readonly WarehouseRow[];
  stockOf: (itemId: bigint, warehouseId: bigint) => number;
  totalStockOf: (itemId: bigint) => number;
  warehouseTotal: (warehouseId: bigint) => number;
  revenue: number;
  categoryTotals: readonly CategoryTotalRow[];
  lowStockItems: readonly ItemRow[];
  onRestock: (itemId: bigint, warehouseId: bigint, quantity: number) => Promise<void>;
  onChangePrice: (itemId: bigint, price: number) => Promise<void>;
  onTransfer: (
    itemId: bigint,
    fromWarehouseId: bigint,
    toWarehouseId: bigint,
    quantity: number
  ) => Promise<void>;
}

export default function AdminPanel({
  items,
  warehouses,
  stockOf,
  totalStockOf,
  warehouseTotal,
  revenue,
  categoryTotals,
  lowStockItems,
  onRestock,
  onChangePrice,
  onTransfer,
}: AdminPanelProps) {
  const [restockInputs, setRestockInputs] = useState<Record<string, string>>({});
  const [restockErrors, setRestockErrors] = useState<Record<string, string>>({});
  const [priceInputs, setPriceInputs] = useState<Record<string, string>>({});
  const [transferInputs, setTransferInputs] = useState<
    Record<string, { from: string; to: string; qty: string }>
  >({});
  const [itemErrors, setItemErrors] = useState<Record<string, string>>({});

  const key = (itemId: bigint, warehouseId: bigint) => `${itemId}-${warehouseId}`;

  const handleRestock = async (itemId: bigint, warehouseId: bigint) => {
    const k = key(itemId, warehouseId);
    const qty = Number(restockInputs[k] ?? '0');
    setRestockErrors((e) => ({ ...e, [k]: '' }));
    if (!Number.isFinite(qty) || qty < 1) {
      setRestockErrors((e) => ({ ...e, [k]: 'Enter a quantity of at least 1.' }));
      return;
    }
    try {
      await onRestock(itemId, warehouseId, Math.floor(qty));
      setRestockInputs((v) => ({ ...v, [k]: '' }));
    } catch (err) {
      setRestockErrors((e) => ({ ...e, [k]: err instanceof Error ? err.message : 'Restock failed.' }));
    }
  };

  const handlePriceSubmit = async (itemId: bigint) => {
    const k = String(itemId);
    const price = Number(priceInputs[k] ?? '');
    setItemErrors((e) => ({ ...e, [k]: '' }));
    if (!Number.isFinite(price) || price <= 0) {
      setItemErrors((e) => ({ ...e, [k]: 'Enter a positive price.' }));
      return;
    }
    try {
      await onChangePrice(itemId, price);
      setPriceInputs((v) => ({ ...v, [k]: '' }));
    } catch (err) {
      setItemErrors((e) => ({ ...e, [k]: err instanceof Error ? err.message : 'Price change failed.' }));
    }
  };

  const handleTransferSubmit = async (itemId: bigint) => {
    const k = String(itemId);
    const input = transferInputs[k] ?? { from: '', to: '', qty: '' };
    const qty = Number(input.qty);
    setItemErrors((e) => ({ ...e, [k]: '' }));
    if (!input.from || !input.to) {
      setItemErrors((e) => ({ ...e, [k]: 'Choose both warehouses.' }));
      return;
    }
    if (!Number.isFinite(qty) || qty < 1) {
      setItemErrors((e) => ({ ...e, [k]: 'Enter a quantity of at least 1.' }));
      return;
    }
    try {
      await onTransfer(itemId, BigInt(input.from), BigInt(input.to), Math.floor(qty));
      setTransferInputs((v) => ({ ...v, [k]: { ...input, qty: '' } }));
    } catch (err) {
      setItemErrors((e) => ({ ...e, [k]: err instanceof Error ? err.message : 'Transfer failed.' }));
    }
  };

  return (
    <div className="admin-panel" data-testid="admin-panel">
      <div className="admin-section">
        <h2 className="section-title">Revenue</h2>
        <div className="admin-revenue" data-testid="admin-revenue">
          {formatMoney(revenue)}
        </div>
      </div>

      <div className="admin-section">
        <h2 className="section-title">Items</h2>
        <table className="admin-table">
          <thead>
            <tr>
              <th>Item</th>
              <th>Price</th>
              <th>Total stock</th>
              <th>Change price</th>
              <th>Transfer stock</th>
            </tr>
          </thead>
          <tbody>
            {items.map((item) => {
              const k = String(item.id);
              const transfer = transferInputs[k] ?? { from: '', to: '', qty: '' };
              return (
                <tr data-testid="admin-item-row" key={k}>
                  <td>{item.name}</td>
                  <td>{formatMoney(item.price)}</td>
                  <td data-testid="admin-stock">{totalStockOf(item.id)}</td>
                  <td>
                    <div className="inline-form">
                      <input
                        type="number"
                        className="price-input"
                        data-testid="price-input"
                        min={0.01}
                        step={0.01}
                        placeholder={item.price.toFixed(2)}
                        value={priceInputs[k] ?? ''}
                        onChange={(e) => setPriceInputs((v) => ({ ...v, [k]: e.target.value }))}
                      />
                      <button
                        type="button"
                        className="btn btn-ghost btn-sm"
                        data-testid="price-submit"
                        onClick={() => handlePriceSubmit(item.id)}
                      >
                        Set
                      </button>
                    </div>
                  </td>
                  <td>
                    <div className="inline-form">
                      <select
                        data-testid="transfer-from"
                        value={transfer.from}
                        onChange={(e) =>
                          setTransferInputs((v) => ({ ...v, [k]: { ...transfer, from: e.target.value } }))
                        }
                      >
                        <option value="">From</option>
                        {warehouses.map((wh) => (
                          <option key={String(wh.id)} value={String(wh.id)}>
                            {wh.name}
                          </option>
                        ))}
                      </select>
                      <select
                        data-testid="transfer-to"
                        value={transfer.to}
                        onChange={(e) =>
                          setTransferInputs((v) => ({ ...v, [k]: { ...transfer, to: e.target.value } }))
                        }
                      >
                        <option value="">To</option>
                        {warehouses.map((wh) => (
                          <option key={String(wh.id)} value={String(wh.id)}>
                            {wh.name}
                          </option>
                        ))}
                      </select>
                      <input
                        type="number"
                        className="transfer-qty"
                        data-testid="transfer-qty"
                        min={1}
                        placeholder="qty"
                        value={transfer.qty}
                        onChange={(e) =>
                          setTransferInputs((v) => ({ ...v, [k]: { ...transfer, qty: e.target.value } }))
                        }
                      />
                      <button
                        type="button"
                        className="btn btn-ghost btn-sm"
                        data-testid="transfer-submit"
                        onClick={() => handleTransferSubmit(item.id)}
                      >
                        Move
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {Object.entries(itemErrors).map(
          ([k, msg]) =>
            msg && (
              <div className="error-text" data-testid="order-error" key={k}>
                {msg}
              </div>
            )
        )}
      </div>

      <div className="admin-section">
        <h2 className="section-title">Low stock</h2>
        <div className="low-stock-list" data-testid="low-stock-list">
          {lowStockItems.length === 0 && <div className="empty-state">Nothing is running low.</div>}
          {lowStockItems.map((item) => (
            <div className="low-stock-item" data-testid="low-stock-item" key={String(item.id)}>
              {item.name}: {totalStockOf(item.id)} left
            </div>
          ))}
        </div>
      </div>

      <div className="admin-section">
        <h2 className="section-title">Warehouses</h2>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {warehouses.map((wh) => (
            <span className="badge badge-muted" data-testid="admin-warehouse-item" key={String(wh.id)}>
              {wh.name}: <span data-testid="warehouse-total">{warehouseTotal(wh.id)}</span>
            </span>
          ))}
        </div>
      </div>

      <div className="admin-section">
        <h2 className="section-title">Category totals</h2>
        <table className="admin-table">
          <thead>
            <tr>
              <th>Category</th>
              <th>Units sold</th>
              <th>Revenue</th>
            </tr>
          </thead>
          <tbody>
            {categoryTotals.map((cat) => (
              <tr data-testid="category-row" key={String(cat.categoryId)}>
                <td>{cat.name}</td>
                <td data-testid="category-units">{cat.unitsSold}</td>
                <td data-testid="category-revenue">{formatMoney(cat.revenue)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="admin-section">
        <h2 className="section-title">Stock by warehouse</h2>
        <table className="admin-table">
          <thead>
            <tr>
              <th>Item</th>
              <th>Warehouse</th>
              <th>Quantity</th>
              <th>Restock</th>
            </tr>
          </thead>
          <tbody>
            {items.map((item) =>
              warehouses.map((wh) => {
                const k = key(item.id, wh.id);
                return (
                  <tr data-testid="admin-location-row" key={k}>
                    <td>{item.name}</td>
                    <td>{wh.name}</td>
                    <td data-testid="admin-location-qty">{stockOf(item.id, wh.id)}</td>
                    <td>
                      <div className="restock-form">
                        <input
                          type="number"
                          className="restock-input"
                          data-testid="restock-input"
                          min={1}
                          placeholder="qty"
                          value={restockInputs[k] ?? ''}
                          onChange={(e) => setRestockInputs((v) => ({ ...v, [k]: e.target.value }))}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handleRestock(item.id, wh.id);
                          }}
                        />
                        <button
                          type="button"
                          className="btn btn-ghost btn-sm"
                          data-testid="restock-submit"
                          onClick={() => handleRestock(item.id, wh.id)}
                        >
                          Add
                        </button>
                      </div>
                      {restockErrors[k] && <div className="error-text">{restockErrors[k]}</div>}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
