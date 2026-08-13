import { useEffect, useMemo, useState } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';
import ItemCard from './components/ItemCard';
import ItemDetail from './components/ItemDetail';
import CartPanel, { CartLine } from './components/CartPanel';
import OrdersPanel, { OrderView } from './components/OrdersPanel';
import AdminPanel from './components/AdminPanel';
import FulfilmentPanel from './components/FulfilmentPanel';
import AuthWidget from './components/AuthWidget';
import { formatMoney } from './types';

const LOW_STOCK_THRESHOLD = 10;

function microsToDate(micros: bigint): Date {
  return new Date(Number(micros / 1000n));
}

export default function App() {
  const { isActive, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  const [items] = useTable(tables.item);
  const [warehouses] = useTable(tables.warehouse);
  const [stocks] = useTable(tables.stock);
  const [reviews] = useTable(tables.review);
  const [itemStats] = useTable(tables.itemStats);
  const [currentUserRows] = useTable(tables.currentUser);
  const [cartRows] = useTable(tables.myCart);
  const [orderRows] = useTable(tables.myOrders);
  const [orderItemRows] = useTable(tables.myOrderItems);
  const [revenueRows] = useTable(tables.adminRevenue);
  const [queueRows] = useTable(tables.fulfilmentQueue);
  const [categoryTotalRows] = useTable(tables.categoryTotals);
  const [recommendedRows] = useTable(tables.recommended);

  const currentUser = currentUserRows[0] ?? null;
  const isSignedIn = currentUser !== null;
  const isAdmin = currentUser?.isAdmin ?? false;
  const isStaff = currentUser?.isStaff ?? false;

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedItemId, setSelectedItemId] = useState<bigint | null>(null);
  const [cartOpen, setCartOpen] = useState(false);
  const [ordersOpen, setOrdersOpen] = useState(false);
  const [adminOpen, setAdminOpen] = useState(false);
  const [staffOpen, setStaffOpen] = useState(false);
  const [buyError, setBuyError] = useState<string | null>(null);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== 'Escape') return;
      setSelectedItemId(null);
      setCartOpen(false);
      setOrdersOpen(false);
      setAdminOpen(false);
      setStaffOpen(false);
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const stockByItem = useMemo(() => {
    const map = new Map<bigint, number>();
    for (const row of stocks) {
      map.set(row.itemId, (map.get(row.itemId) ?? 0) + row.quantity);
    }
    return map;
  }, [stocks]);

  const stockByItemWarehouse = useMemo(() => {
    const map = new Map<string, number>();
    for (const row of stocks) {
      map.set(`${row.itemId}-${row.warehouseId}`, row.quantity);
    }
    return map;
  }, [stocks]);

  const stockByWarehouse = useMemo(() => {
    const map = new Map<bigint, number>();
    for (const row of stocks) {
      map.set(row.warehouseId, (map.get(row.warehouseId) ?? 0) + row.quantity);
    }
    return map;
  }, [stocks]);

  const lowStockItems = useMemo(
    () =>
      [...items]
        .filter((i) => (stockByItem.get(i.id) ?? 0) <= LOW_STOCK_THRESHOLD)
        .sort((a, b) => (stockByItem.get(a.id) ?? 0) - (stockByItem.get(b.id) ?? 0)),
    [items, stockByItem]
  );

  const purchaseCountByItem = useMemo(() => {
    const map = new Map<bigint, number>();
    for (const row of itemStats) map.set(row.itemId, row.purchaseCount);
    return map;
  }, [itemStats]);

  const reviewsByItem = useMemo(() => {
    const map = new Map<bigint, (typeof reviews)[number][]>();
    for (const row of reviews) {
      const list = map.get(row.itemId) ?? [];
      list.push(row);
      map.set(row.itemId, list);
    }
    return map;
  }, [reviews]);

  const purchasedItemIds = useMemo(() => {
    const set = new Set<bigint>();
    for (const li of orderItemRows) set.add(li.itemId);
    return set;
  }, [orderItemRows]);

  const storefrontItems = useMemo(() => {
    const arr = [...items];
    arr.sort((a, b) => {
      const pa = purchaseCountByItem.get(a.id) ?? 0;
      const pb = purchaseCountByItem.get(b.id) ?? 0;
      if (pb !== pa) return pb - pa;
      return a.name.localeCompare(b.name);
    });
    return arr.slice(0, 10);
  }, [items, purchaseCountByItem]);

  const searchResults = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return [];
    return items.filter((i) => i.name.toLowerCase().includes(q));
  }, [items, searchQuery]);

  const cartLines: CartLine[] = useMemo(
    () =>
      cartRows
        .map((c) => {
          const item = items.find((i) => i.id === c.itemId);
          if (!item) return null;
          return {
            itemId: c.itemId,
            name: item.name,
            price: item.price,
            quantity: c.quantity,
            stock: stockByItem.get(c.itemId) ?? 0,
          };
        })
        .filter((x): x is CartLine => x !== null),
    [cartRows, items, stockByItem]
  );

  const cartCount = cartLines.reduce((sum, l) => sum + l.quantity, 0);

  const orderViews: OrderView[] = useMemo(
    () =>
      orderRows.map((o) => ({
        orderId: o.orderId,
        createdAt: microsToDate(o.createdAt.microsSinceUnixEpoch),
        total: o.total,
        status: o.status,
        items: orderItemRows
          .filter((li) => li.orderId === o.orderId)
          .map((li) => ({
            itemId: li.itemId,
            name: li.itemName,
            quantity: li.quantity,
            returned: li.returned,
          })),
      })),
    [orderRows, orderItemRows]
  );

  const revenue = revenueRows[0]?.total ?? 0;

  const selectedItem = selectedItemId !== null ? items.find((i) => i.id === selectedItemId) ?? null : null;
  const selectedItemReviews = selectedItemId !== null ? reviewsByItem.get(selectedItemId) ?? [] : [];

  const handleBuyNow = async (itemId: bigint) => {
    setBuyError(null);
    try {
      await conn?.reducers.buyNow({ itemId });
    } catch (err) {
      setBuyError(err instanceof Error ? err.message : 'Could not complete purchase.');
    }
  };

  const handleAddToCart = async (itemId: bigint) => {
    setBuyError(null);
    try {
      await conn?.reducers.addToCart({ itemId });
    } catch (err) {
      setBuyError(err instanceof Error ? err.message : 'Could not add to cart.');
    }
  };

  const handleSignUp = async (username: string, password: string) => {
    await conn?.reducers.signUp({ username, password });
  };

  const handleSignIn = async (username: string, password: string) => {
    await conn?.reducers.signIn({ username, password });
  };

  const handleSignOut = async () => {
    try {
      await conn?.reducers.signOut({});
    } catch {
      // ignore
    }
  };

  const handleChangeQuantity = async (itemId: bigint, quantity: number) => {
    await conn?.reducers.updateCartQuantity({ itemId, quantity });
  };

  const handleRemove = async (itemId: bigint) => {
    await conn?.reducers.removeFromCart({ itemId });
  };

  const handleCheckout = async () => {
    await conn?.reducers.checkout({});
  };

  const handleSubmitReview = async (itemId: bigint, rating: number, comment: string) => {
    await conn?.reducers.writeReview({ itemId, rating, comment });
  };

  const handleRestock = async (itemId: bigint, warehouseId: bigint, quantity: number) => {
    await conn?.reducers.adminRestock({ itemId, warehouseId, quantity });
  };

  const handleChangePrice = async (itemId: bigint, price: number) => {
    await conn?.reducers.adminChangePrice({ itemId, price });
  };

  const handleTransfer = async (
    itemId: bigint,
    fromWarehouseId: bigint,
    toWarehouseId: bigint,
    quantity: number
  ) => {
    await conn?.reducers.adminTransferStock({ itemId, fromWarehouseId, toWarehouseId, quantity });
  };

  const handleShipOrder = async (orderId: bigint) => {
    await conn?.reducers.shipOrder({ orderId });
  };

  const handleCancelOrder = async (orderId: bigint) => {
    await conn?.reducers.cancelOrder({ orderId });
  };

  const handleReturnItem = async (orderId: bigint, itemId: bigint) => {
    await conn?.reducers.returnOrderItem({ orderId, itemId });
  };

  if (!isActive) {
    return (
      <div className="connecting">
        <div className="spinner" />
        <span>Connecting to SpacetimeDB...</span>
      </div>
    );
  }

  const showingSearch = searchQuery.trim().length > 0;

  return (
    <div className="app">
      <header className="header">
        <span className="app-title" data-testid="app-title">
          SpacetimeDB Shop
        </span>
        <input
          type="text"
          className="search-input"
          data-testid="search-input"
          placeholder="Search items..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
        <div className="header-spacer" />
        <div className="header-actions">
          {isAdmin && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              data-testid="admin-link"
              onClick={() => setAdminOpen(true)}
            >
              Admin
            </button>
          )}
          {isStaff && !isAdmin && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              data-testid="staff-link"
              onClick={() => setStaffOpen(true)}
            >
              Fulfilment
            </button>
          )}
          {isSignedIn && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              data-testid="orders-toggle"
              onClick={() => setOrdersOpen(true)}
            >
              Orders
            </button>
          )}
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            data-testid="cart-toggle"
            onClick={() => setCartOpen(true)}
          >
            Cart <span className="badge" data-testid="cart-count" style={{ marginLeft: 6 }}>{cartCount}</span>
          </button>
          <AuthWidget
            currentUsername={currentUser?.username ?? null}
            onSignUp={handleSignUp}
            onSignIn={handleSignIn}
            onSignOut={handleSignOut}
          />
        </div>
      </header>

      <main className="main">
        {buyError && (
          <div className="error-text" data-testid="buy-error" style={{ marginBottom: 16 }}>
            {buyError}
          </div>
        )}

        {selectedItem && (
          <ItemDetail
            item={selectedItem}
            stock={stockByItem.get(selectedItem.id) ?? 0}
            reviews={selectedItemReviews}
            isSignedIn={isSignedIn}
            hasPurchased={purchasedItemIds.has(selectedItem.id)}
            onClose={() => setSelectedItemId(null)}
            onSubmitReview={handleSubmitReview}
          />
        )}

        <h1 className="section-title">Best sellers</h1>
        <div className="item-list" data-testid="item-list">
          {storefrontItems.map((item) => (
            <ItemCard
              key={String(item.id)}
              item={item}
              stock={stockByItem.get(item.id) ?? 0}
              isSignedIn={isSignedIn}
              onOpen={setSelectedItemId}
              onBuyNow={handleBuyNow}
              onAddToCart={handleAddToCart}
            />
          ))}
        </div>

        <h1 className="section-title" style={{ marginTop: 32 }}>
          Recommended for you
        </h1>
        <div className="recommended-list" data-testid="recommended-list">
          {recommendedRows.length === 0 && (
            <div className="empty-state">Buy something to get recommendations.</div>
          )}
          {recommendedRows.map((r) => (
            <div className="recommended-item" data-testid="recommended-item" key={String(r.itemId)}>
              <span>{r.name}</span>
              <span>{formatMoney(r.price)}</span>
            </div>
          ))}
        </div>

        {showingSearch && (
          <>
            <h1 className="section-title" style={{ marginTop: 32 }}>
              Search results
            </h1>
            <div className="item-list" data-testid="search-results">
              {searchResults.length === 0 && (
                <div className="empty-state">No items match "{searchQuery}".</div>
              )}
              {searchResults.map((item) => (
                <ItemCard
                  key={String(item.id)}
                  item={item}
                  stock={stockByItem.get(item.id) ?? 0}
                  isSignedIn={isSignedIn}
                  onOpen={setSelectedItemId}
                  onBuyNow={handleBuyNow}
                  onAddToCart={handleAddToCart}
                />
              ))}
            </div>
          </>
        )}
      </main>

      {cartOpen && (
        <CartPanel
          lines={cartLines}
          onClose={() => setCartOpen(false)}
          onChangeQuantity={handleChangeQuantity}
          onRemove={handleRemove}
          onCheckout={handleCheckout}
        />
      )}

      {ordersOpen && (
        <OrdersPanel
          orders={orderViews}
          onClose={() => setOrdersOpen(false)}
          onCancel={handleCancelOrder}
          onReturn={handleReturnItem}
        />
      )}

      {adminOpen && isAdmin && (
        <>
          <div className="backdrop" onClick={() => setAdminOpen(false)} />
          <div className="panel" style={{ width: 'min(900px, 96vw)' }}>
            <div className="panel-header">
              <h2>Admin</h2>
              <button
                type="button"
                className="close-btn"
                aria-label="Close"
                onClick={() => setAdminOpen(false)}
              >
                ×
              </button>
            </div>
            <div className="panel-body">
              <AdminPanel
                items={items}
                warehouses={warehouses}
                stockOf={(itemId, warehouseId) => stockByItemWarehouse.get(`${itemId}-${warehouseId}`) ?? 0}
                totalStockOf={(itemId) => stockByItem.get(itemId) ?? 0}
                warehouseTotal={(warehouseId) => stockByWarehouse.get(warehouseId) ?? 0}
                revenue={revenue}
                categoryTotals={categoryTotalRows}
                lowStockItems={lowStockItems}
                onRestock={handleRestock}
                onChangePrice={handleChangePrice}
                onTransfer={handleTransfer}
              />
              <div className="admin-section">
                <FulfilmentPanel queue={queueRows} onShip={handleShipOrder} />
              </div>
            </div>
          </div>
        </>
      )}

      {staffOpen && isStaff && (
        <>
          <div className="backdrop" onClick={() => setStaffOpen(false)} />
          <div className="panel" style={{ width: 'min(700px, 96vw)' }}>
            <div className="panel-header">
              <h2>Fulfilment</h2>
              <button
                type="button"
                className="close-btn"
                aria-label="Close"
                onClick={() => setStaffOpen(false)}
              >
                ×
              </button>
            </div>
            <div className="panel-body">
              <FulfilmentPanel queue={queueRows} onShip={handleShipOrder} />
            </div>
          </div>
        </>
      )}
    </div>
  );
}
