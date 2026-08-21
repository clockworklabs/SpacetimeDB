import { useEffect, useMemo, useState, type Dispatch, type FormEvent, type SetStateAction } from 'react';
import { useSpacetimeDB, useTable } from 'spacetimedb/react';
import { DbConnection, tables } from './module_bindings';

function formatPrice(n: number): string {
  return `$${n.toFixed(2)}`;
}

function toDate(ts: { microsSinceUnixEpoch: bigint }): Date {
  return new Date(Number(ts.microsSinceUnixEpoch / 1000n));
}

type Panel = 'cart' | 'orders' | null;

export default function App() {
  const { isActive, token, getConnection } = useSpacetimeDB();
  const conn = getConnection() as DbConnection | null;

  const [subscribed, setSubscribed] = useState(false);

  useEffect(() => {
    if (token) localStorage.setItem('auth_token', token);
  }, [token]);

  useEffect(() => {
    if (!conn || !isActive) return;
    conn
      .subscriptionBuilder()
      .onApplied(() => setSubscribed(true))
      .subscribe([
        tables.item,
        tables.stock,
        tables.warehouse,
        tables.review,
        tables.myAccount,
        tables.myCart,
        tables.myOrders,
        tables.myOrderItems,
        tables.adminRevenue,
      ]);
  }, [conn, isActive]);

  const [items] = useTable(tables.item);
  const [stockRows] = useTable(tables.stock);
  const [warehouses] = useTable(tables.warehouse);
  const [reviews] = useTable(tables.review);
  const [myAccountRows] = useTable(tables.myAccount);
  const [myCartRows] = useTable(tables.myCart);
  const [myOrderRows] = useTable(tables.myOrders);
  const [myOrderItemRows] = useTable(tables.myOrderItems);
  const [adminRevenueRows] = useTable(tables.adminRevenue);

  const myAccount = myAccountRows[0];
  const isSignedIn = !!myAccount;
  const isAdmin = !!myAccount?.isAdmin;

  // --- UI state ---
  const [searchQuery, setSearchQuery] = useState('');
  const [showSignIn, setShowSignIn] = useState(false);
  const [signupUsername, setSignupUsername] = useState('');
  const [signupPassword, setSignupPassword] = useState('');
  const [signinUsername, setSigninUsername] = useState('');
  const [signinPassword, setSigninPassword] = useState('');
  const [authError, setAuthError] = useState('');
  const [buyError, setBuyError] = useState('');
  const [reviewError, setReviewError] = useState('');
  const [openPanel, setOpenPanel] = useState<Panel>(null);
  const [selectedItemId, setSelectedItemId] = useState<bigint | null>(null);
  const [adminOpen, setAdminOpen] = useState(false);
  const [reviewRating, setReviewRating] = useState('5');
  const [reviewComment, setReviewComment] = useState('');
  const [restockInputs, setRestockInputs] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!isAdmin) setAdminOpen(false);
  }, [isAdmin]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        setOpenPanel(null);
        setSelectedItemId(null);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  // --- Derived data ---
  const stockByItem = useMemo(() => {
    const m = new Map<string, number>();
    for (const s of stockRows) {
      const key = s.itemId.toString();
      m.set(key, (m.get(key) ?? 0) + s.quantity);
    }
    return m;
  }, [stockRows]);

  const stockOf = (itemId: bigint) => stockByItem.get(itemId.toString()) ?? 0;

  const rankedItems = useMemo(() => {
    return [...items].sort((a, b) => {
      if (b.purchaseCount !== a.purchaseCount) return b.purchaseCount - a.purchaseCount;
      return a.name.localeCompare(b.name);
    });
  }, [items]);

  const topTen = rankedItems.slice(0, 10);

  const searchResults = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return [];
    return [...items]
      .filter(i => i.name.toLowerCase().includes(q))
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [items, searchQuery]);

  const cartCount = useMemo(
    () => myCartRows.reduce((sum, l) => sum + l.quantity, 0),
    [myCartRows]
  );
  const cartTotal = useMemo(
    () => myCartRows.reduce((sum, l) => sum + l.quantity * l.unitPrice, 0),
    [myCartRows]
  );

  const ordersSorted = useMemo(
    () => [...myOrderRows].sort((a, b) => toDate(b.createdAt).getTime() - toDate(a.createdAt).getTime()),
    [myOrderRows]
  );

  const orderItemsByOrder = useMemo(() => {
    type OrderItemRow = (typeof myOrderItemRows)[number];
    const m = new Map<string, OrderItemRow[]>();
    for (const oi of myOrderItemRows) {
      const key = oi.orderId.toString();
      const arr = m.get(key) ?? [];
      arr.push(oi);
      m.set(key, arr);
    }
    return m;
  }, [myOrderItemRows]);

  const adminRevenue = adminRevenueRows[0]?.total ?? 0;

  const selectedItem = selectedItemId != null ? items.find(i => i.id === selectedItemId) : undefined;
  const itemReviews = useMemo(
    () => (selectedItemId != null ? reviews.filter(r => r.itemId === selectedItemId) : []),
    [reviews, selectedItemId]
  );
  const reviewAverage =
    itemReviews.length > 0
      ? itemReviews.reduce((sum, r) => sum + r.rating, 0) / itemReviews.length
      : 0;

  const myExistingReview = useMemo(
    () =>
      selectedItemId != null && myAccount
        ? reviews.find(r => r.itemId === selectedItemId && r.accountId === myAccount.id)
        : undefined,
    [reviews, selectedItemId, myAccount]
  );

  useEffect(() => {
    if (myExistingReview) {
      setReviewRating(String(myExistingReview.rating));
      setReviewComment(myExistingReview.comment);
    } else {
      setReviewRating('5');
      setReviewComment('');
    }
    setReviewError('');
  }, [selectedItemId, myExistingReview?.rating, myExistingReview?.comment]);

  // --- Actions ---
  function handleSignUp(e: FormEvent) {
    e.preventDefault();
    setAuthError('');
    if (!conn) return;
    conn.reducers
      .signUp({ username: signupUsername, password: signupPassword })
      .then(() => {
        setSignupUsername('');
        setSignupPassword('');
      })
      .catch((err: Error) => setAuthError(err.message || 'Sign up failed.'));
  }

  function handleSignIn(e: FormEvent) {
    e.preventDefault();
    setAuthError('');
    if (!conn) return;
    conn.reducers
      .signIn({ username: signinUsername, password: signinPassword })
      .then(() => {
        setSigninUsername('');
        setSigninPassword('');
        setShowSignIn(false);
      })
      .catch((err: Error) => setAuthError(err.message || 'Sign in failed.'));
  }

  function handleSignOut() {
    conn?.reducers.signOut({}).catch(() => {});
    setAdminOpen(false);
  }

  function handleBuyNow(itemId: bigint) {
    setBuyError('');
    conn?.reducers.buyNow({ itemId }).catch((err: Error) => setBuyError(err.message || 'Purchase failed.'));
  }

  function handleAddToCart(itemId: bigint) {
    setBuyError('');
    conn?.reducers.addToCart({ itemId }).catch((err: Error) => setBuyError(err.message || 'Could not add to cart.'));
  }

  function handleSetQuantity(itemId: bigint, quantity: number) {
    if (quantity < 1) return;
    setBuyError('');
    conn?.reducers
      .updateCartQuantity({ itemId, quantity })
      .catch((err: Error) => setBuyError(err.message || 'Could not update quantity.'));
  }

  function handleRemoveFromCart(itemId: bigint) {
    conn?.reducers.removeFromCart({ itemId }).catch(() => {});
  }

  function handleCheckout() {
    setBuyError('');
    conn?.reducers.checkout({}).catch((err: Error) => setBuyError(err.message || 'Checkout failed.'));
  }

  function handleSubmitReview(e: FormEvent) {
    e.preventDefault();
    if (selectedItemId == null) return;
    setReviewError('');
    conn?.reducers
      .submitReview({ itemId: selectedItemId, rating: Number(reviewRating), comment: reviewComment })
      .catch((err: Error) => setReviewError(err.message || 'Could not submit review.'));
  }

  function handleRestock(itemId: bigint, warehouseId: bigint) {
    const key = `${itemId}-${warehouseId}`;
    const raw = restockInputs[key];
    const quantity = Number(raw);
    if (!raw || !Number.isFinite(quantity) || quantity <= 0) return;
    conn?.reducers.adminRestock({ itemId, warehouseId, quantity }).then(() => {
      setRestockInputs(prev => ({ ...prev, [key]: '' }));
    }).catch(() => {});
  }

  // --- Rendering ---

  if (!isActive || !subscribed) {
    return (
      <div className="loading-screen">
        <div className="spinner" />
        <div>Connecting to SpacetimeDB…</div>
      </div>
    );
  }

  function renderItemCard(item: (typeof items)[number]) {
    const stock = stockOf(item.id);
    const outOfStock = stock <= 0;
    return (
      <div
        key={item.id.toString()}
        className={`item-card${outOfStock ? ' out-of-stock-card' : ''}`}
        data-testid="item-card"
      >
        <div className="item-card-top" onClick={() => setSelectedItemId(item.id)}>
          <span className="item-name" data-testid="item-name">
            {item.name}
          </span>
          <span className="item-price" data-testid="item-price">
            {formatPrice(item.price)}
          </span>
        </div>
        <div className="item-meta">
          <span>
            Stock:{' '}
            <span className={`item-stock${stock > 0 && stock <= 10 ? ' low' : ''}`} data-testid="item-stock">
              {stock}
            </span>
          </span>
          {outOfStock && (
            <span className="badge-out" data-testid="out-of-stock">
              Out of stock
            </span>
          )}
        </div>
        {isSignedIn && (
          <div className="item-card-actions">
            <button
              type="button"
              className="btn-ghost"
              data-testid="add-to-cart"
              disabled={outOfStock}
              onClick={() => handleAddToCart(item.id)}
            >
              Add to cart
            </button>
            <button
              type="button"
              className="btn-primary"
              data-testid="buy-now"
              disabled={outOfStock}
              onClick={() => handleBuyNow(item.id)}
            >
              Buy now
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="app-shell">
      <header className="app-header">
        <span className="app-title" data-testid="app-title">
          SpacetimeDB Shop
        </span>

        <input
          className="search-input"
          data-testid="search-input"
          type="text"
          placeholder="Search items…"
          value={searchQuery}
          onChange={e => setSearchQuery(e.target.value)}
        />

        <div className="header-spacer" />

        <button type="button" className="icon-btn" data-testid="orders-toggle" onClick={() => setOpenPanel('orders')}>
          Orders
        </button>

        <button type="button" className="icon-btn" data-testid="cart-toggle" onClick={() => setOpenPanel('cart')}>
          Cart
          <span className="pill badge-count" data-testid="cart-count">
            {cartCount}
          </span>
        </button>

        {isAdmin && (
          <button type="button" className="btn-ghost" data-testid="admin-link" onClick={() => setAdminOpen(v => !v)}>
            Admin
          </button>
        )}

        <div className="account-area">
          {isSignedIn ? (
            <>
              <span className="pill pill-muted" data-testid="current-user">
                {myAccount!.username}
              </span>
              <button type="button" className="btn-ghost" data-testid="signout" onClick={handleSignOut}>
                Sign out
              </button>
            </>
          ) : (
            <div className="auth-box">
              <form className="auth-row" onSubmit={handleSignUp}>
                <input
                  data-testid="signup-username"
                  placeholder="Username"
                  value={signupUsername}
                  onChange={e => setSignupUsername(e.target.value)}
                />
                <input
                  data-testid="signup-password"
                  type="password"
                  placeholder="Password"
                  value={signupPassword}
                  onChange={e => setSignupPassword(e.target.value)}
                />
                <button type="submit" className="btn-primary" data-testid="signup-submit">
                  Sign up
                </button>
              </form>
              <button
                type="button"
                className="auth-toggle-link"
                data-testid="signin-toggle"
                onClick={() => setShowSignIn(true)}
              >
                Already have an account? Sign in
              </button>
              {showSignIn && (
                <form className="auth-row" onSubmit={handleSignIn}>
                  <input
                    data-testid="signin-username"
                    placeholder="Username"
                    value={signinUsername}
                    onChange={e => setSigninUsername(e.target.value)}
                  />
                  <input
                    data-testid="signin-password"
                    type="password"
                    placeholder="Password"
                    value={signinPassword}
                    onChange={e => setSigninPassword(e.target.value)}
                  />
                  <button type="submit" className="btn-primary" data-testid="signin-submit">
                    Sign in
                  </button>
                </form>
              )}
              {authError && (
                <div className="error-banner" data-testid="auth-error">
                  {authError}
                </div>
              )}
            </div>
          )}
        </div>
      </header>

      <main
        className="storefront"
        style={{
          paddingRight: selectedItem ? 580 : openPanel ? 440 : undefined,
          transition: 'padding-right 0.18s ease',
        }}
      >
        {buyError && (
          <div className="error-banner" data-testid="buy-error">
            {buyError}
          </div>
        )}

        {searchQuery.trim() ? (
          <>
            <h2 className="section-title">Search results</h2>
            {searchResults.length === 0 ? (
              <div className="empty-state">No items match "{searchQuery}".</div>
            ) : (
              <div className="item-list" data-testid="search-results">
                {searchResults.map(renderItemCard)}
              </div>
            )}
          </>
        ) : (
          <>
            <h2 className="section-title">Best sellers</h2>
            {topTen.length === 0 ? (
              <div className="empty-state">No items yet.</div>
            ) : (
              <div className="item-list" data-testid="item-list">
                {topTen.map(renderItemCard)}
              </div>
            )}
          </>
        )}

        {isAdmin && adminOpen && (
          <AdminPanel
            items={items}
            warehouses={warehouses}
            stockRows={stockRows}
            stockOf={stockOf}
            adminRevenue={adminRevenue}
            restockInputs={restockInputs}
            setRestockInputs={setRestockInputs}
            onRestock={handleRestock}
          />
        )}
      </main>

      {selectedItem && (
        <>
          <div className="overlay-backdrop" />
          <div className="panel wide" data-testid="item-detail">
            <div className="panel-header">
              <h2>{selectedItem.name}</h2>
              <button type="button" className="close-btn" onClick={() => setSelectedItemId(null)}>
                ×
              </button>
            </div>
            <div className="panel-body">
              <div className="item-detail">
                <div className="item-detail-header">
                  <span className="item-name" data-testid="item-name">
                    {selectedItem.name}
                  </span>
                  <span className="item-detail-price" data-testid="item-price">
                    {formatPrice(selectedItem.price)}
                  </span>
                </div>
                <div>
                  Stock:{' '}
                  <span data-testid="item-stock">{stockOf(selectedItem.id)}</span>
                  {stockOf(selectedItem.id) <= 0 && (
                    <span className="badge-out" data-testid="out-of-stock">
                      {' '}
                      Out of stock
                    </span>
                  )}
                </div>

                <div className="review-average-row">
                  <span>Average rating:</span>
                  <span className="value" data-testid="review-average">
                    {reviewAverage.toFixed(1)}
                  </span>
                  <span>({itemReviews.length} review{itemReviews.length === 1 ? '' : 's'})</span>
                </div>

                {isSignedIn && (
                  <form className="review-form" onSubmit={handleSubmitReview}>
                    <div className="review-form-row">
                      <select
                        data-testid="review-rating"
                        value={reviewRating}
                        onChange={e => setReviewRating(e.target.value)}
                      >
                        <option value="1">1 star</option>
                        <option value="2">2 stars</option>
                        <option value="3">3 stars</option>
                        <option value="4">4 stars</option>
                        <option value="5">5 stars</option>
                      </select>
                    </div>
                    <input
                      data-testid="review-input"
                      placeholder="Write a review…"
                      value={reviewComment}
                      onChange={e => setReviewComment(e.target.value)}
                    />
                    <button type="submit" className="btn-primary" data-testid="review-submit">
                      {myExistingReview ? 'Update review' : 'Submit review'}
                    </button>
                    {reviewError && (
                      <div className="error-banner" data-testid="review-error">
                        {reviewError}
                      </div>
                    )}
                  </form>
                )}

                <div>
                  {itemReviews.length === 0 ? (
                    <div className="empty-state">No reviews yet.</div>
                  ) : (
                    itemReviews.map(r => (
                      <div key={r.id.toString()} className="review-item" data-testid="review-item">
                        <span className="review-item-rating">{'★'.repeat(r.rating)}</span>
                        <span>{r.comment}</span>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {openPanel === 'cart' && (
        <>
          <div className="overlay-backdrop" />
          <div className="panel" data-testid="cart-panel">
            <div className="panel-header">
              <h2>Your cart</h2>
              <button type="button" className="close-btn" onClick={() => setOpenPanel(null)}>
                ×
              </button>
            </div>
            <div className="panel-body">
              {!isSignedIn ? (
                <div className="empty-state">Sign in to use your cart.</div>
              ) : myCartRows.length === 0 ? (
                <div className="empty-state" data-testid="empty-cart">
                  Your cart is empty.
                </div>
              ) : (
                myCartRows.map(line => (
                  <div key={line.itemId.toString()} className="cart-item" data-testid="cart-item">
                    <div className="cart-item-info">
                      <span className="item-name" data-testid="item-name">
                        {line.itemName}
                      </span>
                      <span className="item-price" data-testid="item-price">
                        {formatPrice(line.unitPrice)}
                      </span>
                    </div>
                    <div className="cart-item-controls">
                      <button
                        type="button"
                        className="btn-ghost"
                        disabled={line.quantity <= 1}
                        onClick={() => handleSetQuantity(line.itemId, line.quantity - 1)}
                      >
                        −
                      </button>
                      <span data-testid="cart-quantity">{line.quantity}</span>
                      <button
                        type="button"
                        className="btn-ghost"
                        onClick={() => handleSetQuantity(line.itemId, line.quantity + 1)}
                      >
                        +
                      </button>
                      <button
                        type="button"
                        className="btn-danger"
                        data-testid="cart-remove"
                        onClick={() => handleRemoveFromCart(line.itemId)}
                      >
                        Remove
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
            {isSignedIn && myCartRows.length > 0 && (
              <div className="panel-footer">
                <div className="cart-total-row">
                  <span>Total</span>
                  <span data-testid="cart-total">{formatPrice(cartTotal)}</span>
                </div>
                <button type="button" className="btn-primary" data-testid="checkout-submit" onClick={handleCheckout}>
                  Checkout
                </button>
              </div>
            )}
          </div>
        </>
      )}

      {openPanel === 'orders' && (
        <>
          <div className="overlay-backdrop" />
          <div className="panel" data-testid="order-list">
            <div className="panel-header">
              <h2>Order history</h2>
              <button type="button" className="close-btn" onClick={() => setOpenPanel(null)}>
                ×
              </button>
            </div>
            <div className="panel-body">
              {!isSignedIn ? (
                <div className="empty-state">Sign in to see your orders.</div>
              ) : ordersSorted.length === 0 ? (
                <div className="empty-state">You have no past orders yet.</div>
              ) : (
                ordersSorted.map(o => {
                  const lines = orderItemsByOrder.get(o.orderId.toString()) ?? [];
                  return (
                    <div key={o.orderId.toString()} className="order-item" data-testid="order-item">
                      <span className="order-item-names">{lines.map(l => l.itemName).join(', ')}</span>
                      <div className="order-meta">
                        <span>{toDate(o.createdAt).toLocaleString()}</span>
                        <span className="order-total" data-testid="order-total">
                          {formatPrice(o.total)}
                        </span>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function AdminPanel({
  items,
  warehouses,
  stockRows,
  stockOf,
  adminRevenue,
  restockInputs,
  setRestockInputs,
  onRestock,
}: {
  items: ReadonlyArray<{ id: bigint; name: string; price: number; purchaseCount: number }>;
  warehouses: ReadonlyArray<{ id: bigint; name: string }>;
  stockRows: ReadonlyArray<{ id: bigint; itemId: bigint; warehouseId: bigint; quantity: number }>;
  stockOf: (itemId: bigint) => number;
  adminRevenue: number;
  restockInputs: Record<string, string>;
  setRestockInputs: Dispatch<SetStateAction<Record<string, string>>>;
  onRestock: (itemId: bigint, warehouseId: bigint) => void;
}) {
  const sortedItems = [...items].sort((a, b) => a.name.localeCompare(b.name));
  const sortedWarehouses = [...warehouses].sort((a, b) => a.name.localeCompare(b.name));
  const stockLookup = new Map<string, number>();
  for (const s of stockRows) stockLookup.set(`${s.itemId}-${s.warehouseId}`, s.quantity);

  return (
    <div className="admin-panel" data-testid="admin-panel">
      <div className="admin-revenue-box">
        <span>Total revenue</span>
        <span className="value" data-testid="admin-revenue">
          {adminRevenue.toFixed(2)}
        </span>
      </div>

      <div>
        <h3 className="section-title">Items</h3>
        <table className="admin-table">
          <thead>
            <tr>
              <th>Item</th>
              <th>Price</th>
              <th>Total stock</th>
            </tr>
          </thead>
          <tbody>
            {sortedItems.map(item => (
              <tr key={item.id.toString()} data-testid="admin-item-row">
                <td className="item-name" data-testid="item-name">
                  {item.name}
                </td>
                <td className="item-price" data-testid="item-price">
                  {formatPrice(item.price)}
                </td>
                <td data-testid="admin-stock">{stockOf(item.id)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div>
        <h3 className="section-title">Warehouses</h3>
        <div className="warehouse-list">
          {sortedWarehouses.map(w => (
            <span key={w.id.toString()} className="warehouse-chip" data-testid="admin-warehouse-item">
              {w.name}
            </span>
          ))}
        </div>
      </div>

      <div>
        <h3 className="section-title">Stock by warehouse</h3>
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
            {sortedItems.map(item =>
              sortedWarehouses.map(w => {
                const key = `${item.id}-${w.id}`;
                const qty = stockLookup.get(key) ?? 0;
                return (
                  <tr key={key} data-testid="admin-location-row">
                    <td>{item.name}</td>
                    <td>{w.name}</td>
                    <td data-testid="admin-location-qty">{qty}</td>
                    <td>
                      <div className="location-row-controls">
                        <input
                          className="restock-input"
                          data-testid="restock-input"
                          type="number"
                          min={1}
                          placeholder="0"
                          value={restockInputs[key] ?? ''}
                          onChange={e =>
                            setRestockInputs(prev => ({ ...prev, [key]: e.target.value }))
                          }
                        />
                        <button
                          type="button"
                          className="btn-ghost"
                          data-testid="restock-submit"
                          onClick={() => onRestock(item.id, w.id)}
                        >
                          Add
                        </button>
                      </div>
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
