import { useEffect, useMemo, useRef, useState } from "react";
import { io, Socket } from "socket.io-client";

type Item = { id: number; name: string; price: number; stock: number; purchaseCount: number; category: string };
type Account = { id: number; username: string; isAdmin: boolean; isStaff: boolean } | null;
type CartLine = { itemId: number; name: string; price: number; quantity: number; lineTotal: number };
type CartState = { items: CartLine[]; total: number };
type OrderLine = { orderItemId: number; itemId: number; name: string; quantity: number; price: number; returned: boolean };
type Order = { id: number; createdAt: string; total: number; status: "pending" | "shipped" | "cancelled"; items: OrderLine[] };
type Review = { id: number; accountId: number; username: string; rating: number; comment: string; createdAt: string };
type AdminItem = { id: number; name: string; price: number; stock: number; category: string };
type AdminLocation = { itemId: number; itemName: string; warehouseId: number; warehouseName: string; quantity: number };
type AdminWarehouse = { id: number; name: string; total: number };
type LowStockItem = { id: number; name: string; stock: number };
type CategoryTotal = { category: string; units: number; revenue: number };
type AdminState = {
  items: AdminItem[];
  warehouses: AdminWarehouse[];
  locations: AdminLocation[];
  revenue: number;
  lowStock: LowStockItem[];
  categoryTotals: CategoryTotal[];
  queueDepth: number;
};
type QueueItem = { id: number; createdAt: string; items: { name: string; quantity: number; warehouse: string }[] };
type QueueState = { queue: QueueItem[]; depth: number };

let toastId = 0;
type Toast = { id: number; kind: "buy-error" | "auth-error" | "review-error" | "order-error"; message: string };

async function api<T = any>(path: string, opts: RequestInit = {}): Promise<T> {
  const res = await fetch(path, {
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    ...opts,
  });
  let body: any = null;
  try {
    body = await res.json();
  } catch {
    body = null;
  }
  if (!res.ok) {
    const message = body?.error || `request failed (${res.status})`;
    throw new Error(message);
  }
  return body as T;
}

function money(n: number): string {
  return `$${n.toFixed(2)}`;
}

export default function App() {
  const [connected, setConnected] = useState(false);
  const [initialLoaded, setInitialLoaded] = useState(false);
  const [account, setAccount] = useState<Account>(null);
  const [items, setItems] = useState<Item[]>([]);
  const [cart, setCart] = useState<CartState>({ items: [], total: 0 });
  const [orders, setOrders] = useState<Order[]>([]);
  const [admin, setAdmin] = useState<AdminState | null>(null);
  const [queue, setQueue] = useState<QueueState>({ queue: [], depth: 0 });
  const [recommended, setRecommended] = useState<Item[]>([]);
  const [search, setSearch] = useState("");
  const [openItemId, setOpenItemId] = useState<number | null>(null);
  const [itemDetail, setItemDetail] = useState<{ reviews: Review[]; average: number | null } | null>(null);
  const [cartOpen, setCartOpen] = useState(false);
  const [ordersOpen, setOrdersOpen] = useState(false);
  const [adminOpen, setAdminOpen] = useState(false);
  const [fulfilmentOpen, setFulfilmentOpen] = useState(false);
  const [showSignin, setShowSignin] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);

  const socketRef = useRef<Socket | null>(null);
  const cartObservationRef = useRef(0);

  function applyCartResponse(state: CartState, observedAtRequest: number) {
    // A socket update may carry state committed after this request began. Do
    // not let a slower HTTP response roll the live cart back to an older view.
    if (cartObservationRef.current === observedAtRequest) setCart(state);
  }

  function pushToast(kind: Toast["kind"], message: string) {
    const id = ++toastId;
    setToasts((t) => [...t, { id, kind, message }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4000);
  }

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [me, catalog, rec] = await Promise.all([
          api<{ account: Account }>("/api/me"),
          api<{ items: Item[] }>("/api/items"),
          api<{ items: Item[] }>("/api/recommended"),
        ]);
        if (cancelled) return;
        setAccount(me.account);
        setItems(catalog.items);
        setRecommended(rec.items);
      } catch (err) {
        console.error(err);
      } finally {
        if (!cancelled) setInitialLoaded(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const socket = io({ path: "/socket.io" });
    socketRef.current = socket;
    socket.on("connect", () => setConnected(true));
    socket.on("disconnect", () => setConnected(false));
    socket.on("items:update", (payload: { items: Item[] }) => setItems(payload.items));
    socket.on("cart:update", (payload: CartState) => {
      cartObservationRef.current += 1;
      setCart(payload);
    });
    socket.on("orders:update", (payload: { orders: Order[] }) => setOrders(payload.orders));
    socket.on("admin:update", (payload: AdminState) => setAdmin(payload));
    socket.on("queue:update", (payload: QueueState) => setQueue(payload));
    socket.on("recommended:update", (payload: { items: Item[] }) => setRecommended(payload.items));
    socket.on("review:update", (payload: { itemId: number; reviews: Review[]; average: number | null }) => {
      setOpenItemId((current) => {
        if (current === payload.itemId) {
          setItemDetail({ reviews: payload.reviews, average: payload.average });
        }
        return current;
      });
    });
    return () => {
      socket.disconnect();
    };
  }, []);

  useEffect(() => {
    const observedAtRequest = ++cartObservationRef.current;
    if (!account) {
      setCart({ items: [], total: 0 });
      setOrders([]);
      setAdmin(null);
      setQueue({ queue: [], depth: 0 });
      return;
    }
    api<CartState>("/api/cart").then(state => applyCartResponse(state, observedAtRequest)).catch(() => {});
    api<{ orders: Order[] }>("/api/orders").then((r) => setOrders(r.orders)).catch(() => {});
    if (account.isAdmin) {
      api<AdminState>("/api/admin/state").then(setAdmin).catch(() => {});
    }
    if (account.isAdmin || account.isStaff) {
      api<QueueState>("/api/fulfilment/queue").then(setQueue).catch(() => {});
    }
    api<{ items: Item[] }>("/api/recommended").then((r) => setRecommended(r.items)).catch(() => {});
  }, [account]);

  useEffect(() => {
    if (openItemId == null) {
      setItemDetail(null);
      return;
    }
    api<{ item: Item; reviews: Review[]; average: number | null }>(`/api/items/${openItemId}`)
      .then((r) => setItemDetail({ reviews: r.reviews, average: r.average }))
      .catch(() => {});
  }, [openItemId]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setCartOpen(false);
        setOrdersOpen(false);
        setAdminOpen(false);
        setFulfilmentOpen(false);
        setOpenItemId(null);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const visibleItems = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return items.slice(0, 10);
    return items.filter((i) => i.name.toLowerCase().includes(q));
  }, [items, search]);

  function handleSearchChange(value: string) {
    setSearch(value);
    setCartOpen(false);
    setOrdersOpen(false);
    setAdminOpen(false);
    setFulfilmentOpen(false);
  }

  function resyncSocket() {
    // Cookies changed (signed in/out) — reconnect so the server re-reads
    // the session cookie and joins this socket to the right rooms.
    const socket = socketRef.current;
    if (socket) {
      socket.disconnect();
      socket.connect();
    }
  }

  async function handleSignup(username: string, password: string) {
    try {
      const r = await api<{ account: Account }>("/api/auth/signup", {
        method: "POST",
        body: JSON.stringify({ username, password }),
      });
      setAccount(r.account);
      resyncSocket();
    } catch (err: any) {
      pushToast("auth-error", err.message);
    }
  }

  async function handleSignin(username: string, password: string) {
    try {
      const r = await api<{ account: Account }>("/api/auth/signin", {
        method: "POST",
        body: JSON.stringify({ username, password }),
      });
      setAccount(r.account);
      resyncSocket();
    } catch (err: any) {
      pushToast("auth-error", err.message);
    }
  }

  async function handleSignout() {
    try {
      await api("/api/auth/signout", { method: "POST" });
    } catch {
      // ignore
    }
    setAccount(null);
    resyncSocket();
  }

  async function buyNow(itemId: number) {
    try {
      await api(`/api/items/${itemId}/buy`, { method: "POST" });
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function addToCart(itemId: number) {
    try {
      const observedAtRequest = cartObservationRef.current;
      const state = await api<CartState>("/api/cart", {
        method: "POST",
        body: JSON.stringify({ itemId, quantity: 1 }),
      });
      applyCartResponse(state, observedAtRequest);
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function changeCartQuantity(itemId: number, quantity: number) {
    try {
      const observedAtRequest = cartObservationRef.current;
      const state = await api<CartState>(`/api/cart/${itemId}`, {
        method: "PATCH",
        body: JSON.stringify({ quantity }),
      });
      applyCartResponse(state, observedAtRequest);
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function removeCartLine(itemId: number) {
    try {
      const observedAtRequest = cartObservationRef.current;
      const state = await api<CartState>(`/api/cart/${itemId}`, { method: "DELETE" });
      applyCartResponse(state, observedAtRequest);
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function checkout() {
    try {
      await api("/api/checkout", { method: "POST" });
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function submitReview(itemId: number, rating: number, comment: string) {
    try {
      const r = await api<{ reviews: Review[]; average: number | null }>(`/api/items/${itemId}/reviews`, {
        method: "POST",
        body: JSON.stringify({ rating, comment }),
      });
      setItemDetail(r);
    } catch (err: any) {
      pushToast("review-error", err.message);
    }
  }

  async function restock(itemId: number, warehouseId: number, quantity: number) {
    try {
      const state = await api<AdminState>("/api/admin/restock", {
        method: "POST",
        body: JSON.stringify({ itemId, warehouseId, quantity }),
      });
      setAdmin(state);
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function transfer(itemId: number, fromWarehouseId: number, toWarehouseId: number, quantity: number) {
    try {
      const state = await api<AdminState>("/api/admin/transfer", {
        method: "POST",
        body: JSON.stringify({ itemId, fromWarehouseId, toWarehouseId, quantity }),
      });
      setAdmin(state);
    } catch (err: any) {
      pushToast("order-error", err.message);
    }
  }

  async function changePrice(itemId: number, price: number) {
    try {
      const state = await api<AdminState>("/api/admin/price", {
        method: "POST",
        body: JSON.stringify({ itemId, price }),
      });
      setAdmin(state);
    } catch (err: any) {
      pushToast("buy-error", err.message);
    }
  }

  async function cancelOrder(orderId: number) {
    try {
      await api(`/api/orders/${orderId}/cancel`, { method: "POST" });
    } catch (err: any) {
      pushToast("order-error", err.message);
    }
  }

  async function returnItem(orderId: number, orderItemId: number) {
    try {
      await api(`/api/orders/${orderId}/return`, {
        method: "POST",
        body: JSON.stringify({ orderItemId }),
      });
    } catch (err: any) {
      pushToast("order-error", err.message);
    }
  }

  async function shipOrder(orderId: number) {
    try {
      const state = await api<QueueState>("/api/fulfilment/ship", {
        method: "POST",
        body: JSON.stringify({ orderId }),
      });
      setQueue(state);
    } catch (err: any) {
      pushToast("order-error", err.message);
    }
  }

  if (!initialLoaded) {
    return (
      <div className="loading-screen">
        <div className="spinner" />
        <span>Connecting to PostgreSQL Shop...</span>
      </div>
    );
  }

  const detailItem = openItemId != null ? items.find((i) => i.id === openItemId) ?? null : null;
  const cartItemIds = new Set(cart.items.map((l) => l.itemId));

  return (
    <div className="app">
      <ToastArea toasts={toasts} />
      <Header
        account={account}
        search={search}
        setSearch={handleSearchChange}
        cartCount={cart.items.reduce((s, l) => s + l.quantity, 0)}
        onCartToggle={() => {
          setOrdersOpen(false);
          setAdminOpen(false);
          setFulfilmentOpen(false);
          setCartOpen(true);
        }}
        onOrdersToggle={() => {
          setCartOpen(false);
          setAdminOpen(false);
          setFulfilmentOpen(false);
          setOrdersOpen(true);
        }}
        onAdminToggle={() => {
          setCartOpen(false);
          setOrdersOpen(false);
          setFulfilmentOpen(false);
          setAdminOpen(true);
        }}
        onFulfilmentToggle={() => {
          setCartOpen(false);
          setOrdersOpen(false);
          setAdminOpen(false);
          setFulfilmentOpen(true);
        }}
        onSignout={handleSignout}
        showSignin={showSignin}
        setShowSignin={setShowSignin}
        onSignup={handleSignup}
        onSignin={handleSignin}
        connected={connected}
      />
      <main className="main">
        {detailItem && itemDetail && (
          <ItemDetailPanel
            item={detailItem}
            reviews={itemDetail.reviews}
            average={itemDetail.average}
            account={account}
            onClose={() => setOpenItemId(null)}
            onSubmitReview={(rating, comment) => submitReview(detailItem.id, rating, comment)}
          />
        )}

        {account && !account.isAdmin && !account.isStaff && (
          <RecommendedSection items={recommended} />
        )}
        {!account && <RecommendedSection items={recommended} />}

        <h2 className="section-title">{search.trim() ? "Search results" : "Best sellers"}</h2>
        {visibleItems.length === 0 ? (
          <div className="empty-state" data-testid="search-results">
            No items found.
          </div>
        ) : (
          <div className="item-list" data-testid={search.trim() ? "search-results" : "item-list"}>
            {visibleItems.map((it) => (
              <ItemCard
                key={it.id}
                item={it}
                account={account}
                onOpen={() => setOpenItemId(it.id)}
                onBuy={() => buyNow(it.id)}
                onAddToCart={() => addToCart(it.id)}
              />
            ))}
          </div>
        )}
      </main>

      {cartOpen && (
        <>
          <div className="backdrop" onClick={() => setCartOpen(false)} />
          <CartPanel
            cart={cart}
            onClose={() => setCartOpen(false)}
            onChangeQuantity={changeCartQuantity}
            onRemove={removeCartLine}
            onCheckout={checkout}
          />
        </>
      )}

      {ordersOpen && (
        <>
          <div className="backdrop" onClick={() => setOrdersOpen(false)} />
          <OrdersPanel orders={orders} onClose={() => setOrdersOpen(false)} onCancel={cancelOrder} onReturn={returnItem} />
        </>
      )}

      {adminOpen && account?.isAdmin && (
        <>
          <div className="backdrop" onClick={() => setAdminOpen(false)} />
          <AdminPanel
            admin={admin}
            onClose={() => setAdminOpen(false)}
            onRestock={restock}
            onTransfer={transfer}
            onChangePrice={changePrice}
          />
        </>
      )}

      {fulfilmentOpen && account && (account.isAdmin || account.isStaff) && (
        <>
          <div className="backdrop" onClick={() => setFulfilmentOpen(false)} />
          <FulfilmentPanel queue={queue} onClose={() => setFulfilmentOpen(false)} onShip={shipOrder} />
        </>
      )}
    </div>
  );
}

function ToastArea({ toasts }: { toasts: Toast[] }) {
  return (
    <div className="toast">
      {toasts.map((t) => (
        <div className="toast-item" key={t.id} data-testid={t.kind}>
          {t.message}
        </div>
      ))}
    </div>
  );
}

function Header(props: {
  account: Account;
  search: string;
  setSearch: (s: string) => void;
  cartCount: number;
  onCartToggle: () => void;
  onOrdersToggle: () => void;
  onAdminToggle: () => void;
  onFulfilmentToggle: () => void;
  onSignout: () => void;
  showSignin: boolean;
  setShowSignin: (b: boolean) => void;
  onSignup: (u: string, p: string) => void;
  onSignin: (u: string, p: string) => void;
  connected: boolean;
}) {
  const {
    account,
    search,
    setSearch,
    cartCount,
    onCartToggle,
    onOrdersToggle,
    onAdminToggle,
    onFulfilmentToggle,
    onSignout,
    showSignin,
    setShowSignin,
    onSignup,
    onSignin,
    connected,
  } = props;

  const [signupUsername, setSignupUsername] = useState("");
  const [signupPassword, setSignupPassword] = useState("");
  const [signinUsername, setSigninUsername] = useState("");
  const [signinPassword, setSigninPassword] = useState("");

  function submitSignup() {
    if (!signupUsername || !signupPassword) return;
    onSignup(signupUsername, signupPassword);
    setSignupUsername("");
    setSignupPassword("");
  }

  function submitSignin() {
    if (!signinUsername || !signinPassword) return;
    onSignin(signinUsername, signinPassword);
    setSigninUsername("");
    setSigninPassword("");
  }

  return (
    <header className="header">
      <h1 className="app-title" data-testid="app-title">
        PostgreSQL Shop
      </h1>
      <input
        className="input search-input"
        data-testid="search-input"
        placeholder="Search items..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <div className="header-actions">
        {!connected && <span className="muted">reconnecting...</span>}
        {account && !account.isAdmin && !account.isStaff && (
          <button className="btn btn-ghost btn-sm" data-testid="orders-toggle" onClick={onOrdersToggle}>
            Orders
          </button>
        )}
        <button className="btn btn-ghost btn-sm" data-testid="cart-toggle" onClick={onCartToggle}>
          Cart <span className="badge" data-testid="cart-count">{cartCount}</span>
        </button>
        {account?.isAdmin && (
          <button className="btn btn-ghost btn-sm" data-testid="admin-link" onClick={onAdminToggle}>
            Admin
          </button>
        )}
        {account && (account.isStaff || account.isAdmin) && (
          <button className="btn btn-ghost btn-sm" data-testid="staff-link" onClick={onFulfilmentToggle}>
            Fulfilment
          </button>
        )}
      </div>
      <div className="account-area">
        {account ? (
          <>
            <span className="current-user" data-testid="current-user">
              {account.username}
            </span>
            <button className="btn btn-ghost btn-sm" data-testid="signout" onClick={onSignout}>
              Sign out
            </button>
          </>
        ) : (
          <div className="auth-forms">
            <div className="auth-form-group">
              <input
                className="input"
                data-testid="signup-username"
                placeholder="Username"
                value={signupUsername}
                onChange={(e) => setSignupUsername(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && submitSignup()}
              />
              <input
                className="input"
                type="password"
                data-testid="signup-password"
                placeholder="Password"
                value={signupPassword}
                onChange={(e) => setSignupPassword(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && submitSignup()}
              />
              <button className="btn btn-sm" data-testid="signup-submit" onClick={submitSignup}>
                Sign up
              </button>
            </div>
            <button className="link-btn" data-testid="signin-toggle" onClick={() => setShowSignin(true)}>
              Sign in
            </button>
            {showSignin && (
              <div className="auth-form-group">
                <input
                  className="input"
                  data-testid="signin-username"
                  placeholder="Username"
                  value={signinUsername}
                  onChange={(e) => setSigninUsername(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && submitSignin()}
                />
                <input
                  className="input"
                  type="password"
                  data-testid="signin-password"
                  placeholder="Password"
                  value={signinPassword}
                  onChange={(e) => setSigninPassword(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && submitSignin()}
                />
                <button className="btn btn-sm" data-testid="signin-submit" onClick={submitSignin}>
                  Sign in
                </button>
              </div>
            )}
          </div>
        )}
      </div>
    </header>
  );
}

function RecommendedSection({ items }: { items: Item[] }) {
  return (
    <div className="recommended-section">
      <h2 className="section-title">Recommended for you</h2>
      <div className="recommended-list" data-testid="recommended-list">
        {items.length === 0 ? (
          <div className="empty-state">Nothing recommended yet.</div>
        ) : (
          items.map((it) => (
            <div className="recommended-item" data-testid="recommended-item" key={it.id}>
              <span className="item-name">{it.name}</span>
              <span className="item-price">{money(it.price)}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function ItemCard(props: {
  item: Item;
  account: Account;
  onOpen: () => void;
  onBuy: () => void;
  onAddToCart: () => void;
}) {
  const { item, account, onOpen, onBuy, onAddToCart } = props;
  const outOfStock = item.stock <= 0;
  const canBuy = !!account && !account.isAdmin && !account.isStaff;

  return (
    <div
      className={`item-card${outOfStock ? " out-of-stock-card" : ""}`}
      data-testid="item-card"
      onClick={onOpen}
    >
      <div className="item-row">
        <span className="item-name" data-testid="item-name">{item.name}</span>
      </div>
      <div className="item-row">
        <span className="item-price" data-testid="item-price">{money(item.price)}</span>
      </div>
      <div className="item-stock-row">
        <span>
          Stock: <span className={`item-stock${item.stock > 0 && item.stock <= 5 ? " low-stock" : ""}`} data-testid="item-stock">{item.stock}</span>
        </span>
        {outOfStock && (
          <span className="out-of-stock-badge" data-testid="out-of-stock">
            Out of stock
          </span>
        )}
      </div>
      {canBuy && (
        <div className="item-card-actions" onClick={(e) => e.stopPropagation()}>
          <button className="btn btn-sm" data-testid="buy-now" disabled={outOfStock} onClick={onBuy}>
            Buy now
          </button>
          <button className="btn btn-ghost btn-sm" data-testid="add-to-cart" disabled={outOfStock} onClick={onAddToCart}>
            Add to cart
          </button>
        </div>
      )}
    </div>
  );
}

function ItemDetailPanel(props: {
  item: Item;
  reviews: Review[];
  average: number | null;
  account: Account;
  onClose: () => void;
  onSubmitReview: (rating: number, comment: string) => void;
}) {
  const { item, reviews, average, account, onClose, onSubmitReview } = props;
  const [rating, setRating] = useState(5);
  const [comment, setComment] = useState("");

  function submit() {
    if (!comment.trim()) return;
    onSubmitReview(rating, comment.trim());
    setComment("");
  }

  return (
    <div className="item-detail" data-testid="item-detail">
      <div className="panel-header">
        <h3>{item.name}</h3>
        <button className="close-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      <div className="item-detail-body">
        <div className="item-row">
          <span className="item-price">{money(item.price)}</span>
        </div>
        <div className="muted">Stock: {item.stock}</div>
        <div className="rating-row">
          <span>Average rating:</span>
          <strong data-testid="review-average">{average != null ? average.toFixed(1) : "—"}</strong>
        </div>
        <div className="stack">
          <h4>Reviews</h4>
          {reviews.length === 0 ? (
            <div className="empty-state">No reviews yet. Be the first to write one.</div>
          ) : (
            reviews.map((r) => (
              <div className="review-item" data-testid="review-item" key={r.id}>
                <div className="muted">
                  {r.username} · {"★".repeat(r.rating)}{"☆".repeat(5 - r.rating)}
                </div>
                <div>{r.comment}</div>
              </div>
            ))
          )}
        </div>
        {account && !account.isAdmin && !account.isStaff && (
          <div className="review-form">
            <h4>Write a review</h4>
            <div className="review-form-row">
              <select
                className="input"
                data-testid="review-rating"
                value={rating}
                onChange={(e) => setRating(Number(e.target.value))}
              >
                {[5, 4, 3, 2, 1].map((n) => (
                  <option key={n} value={n}>
                    {n} star{n === 1 ? "" : "s"}
                  </option>
                ))}
              </select>
            </div>
            <input
              className="input"
              data-testid="review-input"
              placeholder="Share your thoughts..."
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
            />
            <button className="btn btn-sm" data-testid="review-submit" onClick={submit}>
              Submit review
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function CartPanel(props: {
  cart: CartState;
  onClose: () => void;
  onChangeQuantity: (itemId: number, quantity: number) => void;
  onRemove: (itemId: number) => void;
  onCheckout: () => void;
}) {
  const { cart, onClose, onChangeQuantity, onRemove, onCheckout } = props;
  return (
    <div className="panel" data-testid="cart-panel">
      <div className="panel-header">
        <h3>Your cart</h3>
        <button className="close-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      {cart.items.length === 0 ? (
        <div className="empty-state" data-testid="empty-cart">
          Your cart is empty.
        </div>
      ) : (
        <div className="stack">
          {cart.items.map((line) => (
            <div className="cart-item" data-testid="cart-item" key={line.itemId}>
              <div className="cart-item-top">
                <span className="item-name">{line.name}</span>
                <span>{money(line.lineTotal)}</span>
              </div>
              <div className="cart-item-top">
                <div className="qty-controls">
                  <button
                    className="btn btn-ghost btn-sm"
                    onClick={() => onChangeQuantity(line.itemId, Math.max(1, line.quantity - 1))}
                    disabled={line.quantity <= 1}
                  >
                    −
                  </button>
                  <input
                    className="input qty-input"
                    data-testid="cart-quantity"
                    type="number"
                    min={1}
                    value={line.quantity}
                    onChange={(e) => {
                      const v = Number(e.target.value);
                      if (Number.isInteger(v) && v >= 1) onChangeQuantity(line.itemId, v);
                    }}
                  />
                  <button className="btn btn-ghost btn-sm" onClick={() => onChangeQuantity(line.itemId, line.quantity + 1)}>
                    +
                  </button>
                </div>
                <button className="btn btn-ghost btn-sm" data-testid="cart-remove" onClick={() => onRemove(line.itemId)}>
                  Remove
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="panel-footer">
        <div className="cart-total" data-testid="cart-total">
          Total: {money(cart.total)}
        </div>
        <button className="btn" data-testid="checkout-submit" disabled={cart.items.length === 0} onClick={onCheckout}>
          Checkout
        </button>
      </div>
    </div>
  );
}

function OrdersPanel(props: {
  orders: Order[];
  onClose: () => void;
  onCancel: (orderId: number) => void;
  onReturn: (orderId: number, orderItemId: number) => void;
}) {
  const { orders, onClose, onCancel, onReturn } = props;
  return (
    <div className="panel" data-testid="order-list">
      <div className="panel-header">
        <h3>Order history</h3>
        <button className="close-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      {orders.length === 0 ? (
        <div className="empty-state">You haven't placed any orders yet.</div>
      ) : (
        <div className="stack">
          {orders.map((o) => (
            <div className="order-item" data-testid="order-item" key={o.id}>
              <div className="order-item-top">
                <span className="muted">{new Date(o.createdAt).toLocaleString()}</span>
                <span className={`order-status order-status-${o.status}`} data-testid="order-status">
                  {o.status}
                </span>
              </div>
              <div className="order-lines">
                {o.items.map((l) => (
                  <div className="order-line" key={l.orderItemId}>
                    <span>
                      {l.name} × {l.quantity}
                      {l.returned ? " (returned)" : ""}
                    </span>
                    {o.status === "shipped" && !l.returned && (
                      <button
                        className="btn btn-ghost btn-sm"
                        data-testid="return-item"
                        onClick={() => onReturn(o.id, l.orderItemId)}
                      >
                        Return
                      </button>
                    )}
                  </div>
                ))}
              </div>
              <div className="order-item-top">
                <div className="order-total" data-testid="order-total">
                  {money(o.total)}
                </div>
                {o.status === "pending" && (
                  <button className="btn btn-ghost btn-sm" data-testid="cancel-order" onClick={() => onCancel(o.id)}>
                    Cancel order
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function AdminPanel(props: {
  admin: AdminState | null;
  onClose: () => void;
  onRestock: (itemId: number, warehouseId: number, quantity: number) => void;
  onTransfer: (itemId: number, fromWarehouseId: number, toWarehouseId: number, quantity: number) => void;
  onChangePrice: (itemId: number, price: number) => void;
}) {
  const { admin, onClose, onRestock, onTransfer, onChangePrice } = props;
  const [restockAmounts, setRestockAmounts] = useState<Record<string, number>>({});
  const [transferState, setTransferState] = useState<Record<number, { from: number; to: number; qty: number }>>({});
  const [priceState, setPriceState] = useState<Record<number, number>>({});

  if (!admin) {
    return (
      <div className="panel wide" data-testid="admin-panel">
        <div className="panel-header">
          <h3>Admin</h3>
          <button className="close-btn" onClick={onClose} aria-label="Close">
            ×
          </button>
        </div>
        <div className="empty-state">Loading admin data...</div>
      </div>
    );
  }

  function key(itemId: number, warehouseId: number) {
    return `${itemId}:${warehouseId}`;
  }

  const warehouses = admin.warehouses;

  function transferFor(itemId: number) {
    return (
      transferState[itemId] ?? {
        from: warehouses[0]?.id ?? 0,
        to: warehouses[1]?.id ?? warehouses[0]?.id ?? 0,
        qty: 1,
      }
    );
  }

  return (
    <div className="panel wide" data-testid="admin-panel">
      <div className="panel-header">
        <h3>Admin</h3>
        <button className="close-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>

      <div className="revenue-box">
        <span>Total revenue</span>
        <strong data-testid="admin-revenue">{admin.revenue.toFixed(2)}</strong>
      </div>
      <div className="revenue-box">
        <span>Orders waiting to ship</span>
        <strong data-testid="queue-depth">{admin.queueDepth}</strong>
      </div>

      <div className="admin-section">
        <h4>Warehouses</h4>
        <div className="warehouse-list">
          {admin.warehouses.map((w) => (
            <span className="warehouse-chip" data-testid="admin-warehouse-item" key={w.id}>
              {w.name}: <span data-testid="warehouse-total">{w.total}</span>
            </span>
          ))}
        </div>
      </div>

      <div className="admin-section">
        <h4>Low stock (≤ 10 units)</h4>
        <div className="stack" data-testid="low-stock-list">
          {admin.lowStock.length === 0 ? (
            <div className="empty-state">Nothing is low on stock.</div>
          ) : (
            admin.lowStock.map((it) => (
              <div className="low-stock-item" data-testid="low-stock-item" key={it.id}>
                <span className="item-name">{it.name}</span>
                <span className="low-stock-qty">{it.stock} left</span>
              </div>
            ))
          )}
        </div>
      </div>

      <div className="admin-section">
        <h4>Category totals</h4>
        <div className="stack">
          {admin.categoryTotals.map((c) => (
            <div className="category-row" data-testid="category-row" key={c.category}>
              <span className="item-name">{c.category}</span>
              <span>
                Units: <span data-testid="category-units">{c.units}</span>
              </span>
              <span>
                Revenue: <span data-testid="category-revenue">{c.revenue.toFixed(2)}</span>
              </span>
            </div>
          ))}
        </div>
      </div>

      <div className="admin-section">
        <h4>Items</h4>
        <div className="stack">
          {admin.items.map((it) => {
            const t = transferFor(it.id);
            const priceValue = priceState[it.id] ?? it.price;
            return (
              <div className="admin-item-row" data-testid="admin-item-row" key={it.id}>
                <div className="admin-row-top">
                  <span className="item-name">{it.name}</span>
                  <span className="muted">{it.category}</span>
                </div>
                <div className="muted">
                  Total stock: <span data-testid="admin-stock">{it.stock}</span>
                </div>

                <div className="price-form">
                  <input
                    className="input qty-input price-input"
                    data-testid="price-input"
                    type="number"
                    min={0.01}
                    step="0.01"
                    value={priceValue}
                    onChange={(e) => setPriceState((prev) => ({ ...prev, [it.id]: Number(e.target.value) }))}
                  />
                  <button
                    className="btn btn-sm"
                    data-testid="price-submit"
                    onClick={() => onChangePrice(it.id, Number(priceValue))}
                  >
                    Set price
                  </button>
                </div>

                <div className="transfer-form">
                  <select
                    className="input"
                    data-testid="transfer-from"
                    value={t.from}
                    onChange={(e) =>
                      setTransferState((prev) => ({ ...prev, [it.id]: { ...t, from: Number(e.target.value) } }))
                    }
                  >
                    {warehouses.map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}
                      </option>
                    ))}
                  </select>
                  <span>→</span>
                  <select
                    className="input"
                    data-testid="transfer-to"
                    value={t.to}
                    onChange={(e) =>
                      setTransferState((prev) => ({ ...prev, [it.id]: { ...t, to: Number(e.target.value) } }))
                    }
                  >
                    {warehouses.map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}
                      </option>
                    ))}
                  </select>
                  <input
                    className="input qty-input"
                    data-testid="transfer-qty"
                    type="number"
                    min={1}
                    value={t.qty}
                    onChange={(e) =>
                      setTransferState((prev) => ({ ...prev, [it.id]: { ...t, qty: Number(e.target.value) } }))
                    }
                  />
                  <button
                    className="btn btn-sm"
                    data-testid="transfer-submit"
                    onClick={() => onTransfer(it.id, t.from, t.to, Math.max(1, Math.floor(t.qty)))}
                  >
                    Transfer
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="admin-section">
        <h4>Stock by warehouse</h4>
        <div className="stack">
          {admin.locations.map((loc) => {
            const k = key(loc.itemId, loc.warehouseId);
            const amount = restockAmounts[k] ?? 1;
            return (
              <div className="admin-location-row" data-testid="admin-location-row" key={k}>
                <div className="admin-row-top">
                  <span>
                    {loc.itemName} @ {loc.warehouseName}
                  </span>
                  <span data-testid="admin-location-qty">{loc.quantity}</span>
                </div>
                <div className="restock-form">
                  <input
                    className="input qty-input"
                    data-testid="restock-input"
                    type="number"
                    min={1}
                    value={amount}
                    onChange={(e) =>
                      setRestockAmounts((prev) => ({ ...prev, [k]: Number(e.target.value) }))
                    }
                  />
                  <button
                    className="btn btn-sm"
                    data-testid="restock-submit"
                    onClick={() => onRestock(loc.itemId, loc.warehouseId, Math.max(1, Math.floor(amount)))}
                  >
                    Restock
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function FulfilmentPanel(props: { queue: QueueState; onClose: () => void; onShip: (orderId: number) => void }) {
  const { queue, onClose, onShip } = props;
  return (
    <div className="panel wide" data-testid="fulfilment-panel">
      <div className="panel-header">
        <h3>Fulfilment queue</h3>
        <button className="close-btn" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      <div className="revenue-box">
        <span>Orders waiting</span>
        <strong data-testid="queue-depth">{queue.depth}</strong>
      </div>
      {queue.queue.length === 0 ? (
        <div className="empty-state">No orders waiting to ship.</div>
      ) : (
        <div className="stack">
          {queue.queue.map((o) => (
            <div className="queue-item" data-testid="queue-item" key={o.id}>
              <div className="muted">{new Date(o.createdAt).toLocaleString()}</div>
              <div className="stack">
                {o.items.map((line, idx) => (
                  <div className="queue-line" key={idx}>
                    <span>
                      {line.name} × {line.quantity}
                    </span>
                    <span className="muted">
                      from <span data-testid="queue-warehouse">{line.warehouse}</span>
                    </span>
                  </div>
                ))}
              </div>
              <button className="btn btn-sm" data-testid="ship-submit" onClick={() => onShip(o.id)}>
                Mark shipped
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
