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
import ProgressionWorkbench from './components/ProgressionWorkbench';
import { formatMoney } from './types';

const LOW_STOCK_THRESHOLD = 10;
const CATALOG_PAGE_SIZE = 10;
type ActivePanel = 'cart' | 'orders' | 'admin' | 'staff' | null;

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
  const [catalogDetailRows] = useTable(tables.catalogDetails);
  const [itemVariantRows] = useTable(tables.itemVariant);
  const [profileRows] = useTable(tables.myProfile);
  const [staffRoleRows] = useTable(tables.staffRoles);
  const [supportTicketRows] = useTable(tables.visibleSupportTickets);
  const [supportReplyRows] = useTable(tables.visibleSupportReplies);
  const [preferenceRows] = useTable(tables.myNotificationPreferences);
  const [notificationRows] = useTable(tables.myNotifications);
  const [reservationRows] = useTable(tables.myReservations);
  const [expiredCartRows] = useTable(tables.myExpiredCart);
  const [restockRows] = useTable(tables.visibleRestocks);
  const [stockLedgerRows] = useTable(tables.visibleStockLedger);
  const [paymentRows] = useTable(tables.myPayments);
  const [activityRows] = useTable(tables.activityHistory);
  const [promotionReportRows] = useTable(tables.promotionReports);
  const [promotionRows] = useTable(tables.visiblePromotions);
  const [reorderRuleRows] = useTable(tables.visibleReorderRules);
  const [completedOrderRows] = useTable(tables.completedOrders);

  const currentUser = currentUserRows[0] ?? null;
  const isSignedIn = currentUser !== null;
  const isAdmin = currentUser?.isAdmin ?? false;
  const isStaff = currentUser?.isStaff ?? false;

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedItemId, setSelectedItemId] = useState<bigint | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>(null);
  const [buyError, setBuyError] = useState<string | null>(null);
  const [categoryFilter, setCategoryFilter] = useState('');
  const [minimumPrice, setMinimumPrice] = useState('');
  const [maximumPrice, setMaximumPrice] = useState('');
  const [inStockOnly, setInStockOnly] = useState(false);
  const [searchPage, setSearchPage] = useState(0);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== 'Escape') return;
      setSelectedItemId(null);
      setActivePanel(null);
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

  const itemVariantsByItem = useMemo(() => {
    const map = new Map<bigint, string[]>();
    for (const row of itemVariantRows) {
      map.set(row.itemId, [...(map.get(row.itemId) ?? []), row.name]);
    }
    return map;
  }, [itemVariantRows]);

  const purchasedItemIds = useMemo(() => {
    const set = new Set<bigint>();
    for (const li of orderItemRows) set.add(li.itemId);
    return set;
  }, [orderItemRows]);

  const rankedItems = useMemo(() => {
    const arr = [...items];
    arr.sort((a, b) => {
      const pa = purchaseCountByItem.get(a.id) ?? 0;
      const pb = purchaseCountByItem.get(b.id) ?? 0;
      if (pb !== pa) return pb - pa;
      return a.name.localeCompare(b.name);
    });
    return arr;
  }, [items, purchaseCountByItem]);

  const filteredSearchResults = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    const min = minimumPrice === '' ? -Infinity : Number(minimumPrice);
    const max = maximumPrice === '' ? Infinity : Number(maximumPrice);
    const categoryByItem = new Map(catalogDetailRows.map(row => [row.itemId, row.category]));
    return [...items]
      .filter(item => !q || item.name.toLowerCase().includes(q))
      .filter(item => !categoryFilter || categoryByItem.get(item.id) === categoryFilter)
      .filter(item => item.price >= min && item.price <= max)
      .filter(item => !inStockOnly || (stockByItem.get(item.id) ?? 0) > 0)
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [items, searchQuery, categoryFilter, minimumPrice, maximumPrice, inStockOnly, catalogDetailRows, stockByItem]);

  const showingSearch = searchQuery.trim().length > 0 || categoryFilter !== ''
    || minimumPrice !== '' || maximumPrice !== '' || inStockOnly;
  const catalogItems = showingSearch ? filteredSearchResults : rankedItems;
  const searchResults = catalogItems.slice(searchPage * CATALOG_PAGE_SIZE,
    searchPage * CATALOG_PAGE_SIZE + CATALOG_PAGE_SIZE);
  const categories = [...new Set(catalogDetailRows.map(row => row.category))].sort();

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
        discount: o.discount,
        refundedTotal: o.refundedTotal,
        payments: paymentRows
          .filter(payment => payment.orderId === o.orderId)
          .map(payment => ({ amount: payment.amount, status: payment.status })),
        items: orderItemRows
          .filter((li) => li.orderId === o.orderId)
          .map((li) => ({
            itemId: li.itemId,
            name: li.itemName,
            quantity: li.quantity,
            returned: li.returned,
          })),
      })),
    [orderRows, orderItemRows, paymentRows]
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

  const handleStockAlert = async (itemId: bigint) => {
    await conn?.reducers.requestStockAlert({ itemId });
  };

  const handleSignUp = async (username: string, password: string) => {
    await conn?.reducers.signUp({ username, password });
  };

  const handleSignIn = async (username: string, password: string) => {
    await conn?.reducers.signIn({ username, password });
  };

  const handleSignOut = async () => {
    setActivePanel(null);
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

  const progressionWorkbench = () => (
    <ProgressionWorkbench
      conn={conn}
      items={items}
      warehouses={warehouses}
      orders={orderViews}
      itemVariants={itemVariantRows}
      profile={profileRows[0] ?? null}
      staffRoles={staffRoleRows}
      supportTickets={supportTicketRows}
      supportReplies={supportReplyRows}
      preferences={preferenceRows[0] ?? null}
      notifications={notificationRows}
      expiredCart={expiredCartRows}
      restocks={restockRows}
      stockLedger={stockLedgerRows}
      activity={activityRows}
      promotionReports={promotionReportRows}
      promotions={promotionRows}
      reorderRules={reorderRuleRows}
      completedOrders={completedOrderRows}
      isSignedIn={isSignedIn}
      isStaff={isStaff}
      isAdmin={isAdmin}
    />
  );

  return (
    <div className="app">
      <header className="header">
        <span className="app-title" data-role="app-title">
          Storefront
        </span>
        <button type="button" className="btn btn-ghost btn-sm" data-role="catalog-link" onClick={() => setActivePanel(null)}>
          Catalog
        </button>
        <input
          type="text"
          className="search-input"
          data-role="search-input"
          placeholder="Search items..."
          value={searchQuery}
          onChange={(e) => {
            setSearchQuery(e.target.value);
            setSearchPage(0);
            setSelectedItemId(null);
            setActivePanel(null);
          }}
        />
        <input data-role="category-filter" list="category-options" placeholder="Category" value={categoryFilter} onChange={e => { setCategoryFilter(e.target.value); setSearchPage(0); }} />
        <datalist id="category-options">{categories.map(category => <option key={category}>{category}</option>)}</datalist>
        <input data-role="minimum-price" type="number" placeholder="Min" value={minimumPrice} onChange={e => { setMinimumPrice(e.target.value); setSearchPage(0); }} />
        <input data-role="maximum-price" type="number" placeholder="Max" value={maximumPrice} onChange={e => { setMaximumPrice(e.target.value); setSearchPage(0); }} />
        <label><input data-role="in-stock-filter" type="checkbox" checked={inStockOnly} onChange={e => { setInStockOnly(e.target.checked); setSearchPage(0); }} />In stock</label>
        <div className="header-spacer" />
        <div className="header-actions">
          {isAdmin && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              data-role="admin-link"
              onClick={() => setActivePanel('admin')}
            >
              Admin
            </button>
          )}
          {(isStaff || isAdmin) && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              data-role="staff-link"
              onClick={() => setActivePanel('staff')}
            >
              Fulfilment
            </button>
          )}
          {isSignedIn && (
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              data-role="orders-toggle"
              onClick={() => setActivePanel('orders')}
            >
              Orders
            </button>
          )}
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            data-role="cart-toggle"
            onClick={() => setActivePanel('cart')}
          >
            Cart <span className="badge" data-role="cart-count" style={{ marginLeft: 6 }}>{cartCount}</span>
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
          <div className="error-text" data-role="buy-error" style={{ marginBottom: 16 }}>
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

        <h1 className="section-title">{showingSearch ? 'Search results' : 'Best sellers'}</h1>
        <div data-role="item-list">
          <div className="item-list" data-role="search-results">
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
                onStockAlert={handleStockAlert}
                variants={itemVariantsByItem.get(item.id) ?? []}
              />
            ))}
          </div>
        </div>
        <div className="search-pagination">
          <button className="btn btn-ghost btn-sm" data-role="search-previous-page"
            disabled={searchPage === 0}
            onClick={() => setSearchPage(page => Math.max(0, page - 1))}>Previous</button>
          <button className="btn btn-ghost btn-sm" data-role="search-next-page"
            disabled={(searchPage + 1) * CATALOG_PAGE_SIZE >= catalogItems.length}
            onClick={() => setSearchPage(page => page + 1)}>Next</button>
        </div>

        <h1 className="section-title" style={{ marginTop: 32 }}>
          Recommended for you
        </h1>
        <div data-role="recommendations">
        <div className="recommended-list" data-role="recommended-list">
          {recommendedRows.length === 0 && (
            <div className="empty-state">Buy something to get recommendations.</div>
          )}
          {[...recommendedRows].sort((a, b) => a.rank - b.rank).map((r) => (
            <div className="recommended-item" data-role="recommended-item" key={String(r.itemId)}>
              <span>
                {r.name} <span data-role="recommendation-rank">{r.rank}</span>
                <span>{formatMoney(r.price)}</span>
                <button className="btn btn-ghost btn-sm" data-role="dismiss-recommendation" onClick={() => conn?.reducers.dismissRecommendation({ itemId: r.itemId })}>Dismiss</button>
              </span>
            </div>
          ))}
        </div>
        </div>

        {activePanel !== 'admin' && activePanel !== 'staff' && progressionWorkbench()}
      </main>

      {activePanel === 'cart' && (
        <CartPanel
          lines={cartLines}
          onClose={() => setActivePanel(null)}
          onChangeQuantity={handleChangeQuantity}
          onRemove={handleRemove}
          onCheckout={handleCheckout}
          reservations={reservationRows}
          onApplyPromotion={(code) => conn?.reducers.applyPromotion({ code })}
        />
      )}

      {activePanel === 'orders' && isSignedIn && (
        <OrdersPanel
          orders={orderViews}
          onClose={() => setActivePanel(null)}
          onCancel={handleCancelOrder}
          onReturn={handleReturnItem}
        />
      )}

      {activePanel === 'admin' && isAdmin && (
        <>
          <div className="backdrop" onClick={() => setActivePanel(null)} />
          <div className="panel" style={{ width: 'min(900px, 96vw)' }}>
            <div className="panel-header">
              <h2>Admin</h2>
              <button
                type="button"
                className="close-btn"
                aria-label="Close"
                onClick={() => setActivePanel(null)}
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
              {progressionWorkbench()}
            </div>
          </div>
        </>
      )}

      {activePanel === 'staff' && (isStaff || isAdmin) && (
        <>
          <div className="backdrop" onClick={() => setActivePanel(null)} />
          <div className="panel" style={{ width: 'min(700px, 96vw)' }}>
            <div className="panel-header">
              <h2>Fulfilment</h2>
              <button
                type="button"
                className="close-btn"
                aria-label="Close"
                onClick={() => setActivePanel(null)}
              >
                ×
              </button>
            </div>
            <div className="panel-body">
              <FulfilmentPanel queue={queueRows} onShip={handleShipOrder} />
              {progressionWorkbench()}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
