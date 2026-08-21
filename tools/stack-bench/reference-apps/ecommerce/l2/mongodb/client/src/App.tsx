import React, { useEffect, useMemo, useRef, useState, useCallback } from "react";
import { io, Socket } from "socket.io-client";

const TOKEN_KEY = "mongodb_shop_token";

interface ItemT {
  id: string;
  name: string;
  price: number;
  description?: string;
  category: string;
  stock: number;
  purchaseCount: number;
}

interface ReviewT {
  id: string;
  itemId: string;
  userId: string;
  username: string;
  rating: number;
  comment: string;
  createdAt: string;
}

interface ItemDetailT {
  id: string;
  name: string;
  price: number;
  description: string;
  stock: number;
  reviews: ReviewT[];
  average: number;
}

interface CartLineT {
  itemId: string;
  name: string;
  price: number;
  stock: number;
  quantity: number;
}

interface CartT {
  items: CartLineT[];
  total: number;
}

interface OrderLineT {
  itemId: string;
  name: string;
  price: number;
  quantity: number;
  returned?: boolean;
  warehouseNames?: string[];
}

interface OrderT {
  id: string;
  items: OrderLineT[];
  total: number;
  status: "pending" | "shipped" | "cancelled";
  createdAt: string;
}

interface UserT {
  id: string;
  username: string;
  isAdmin: boolean;
  isStaff: boolean;
}

interface AdminLocationT {
  id: string;
  itemId: string;
  itemName: string;
  warehouseId: string;
  warehouseName: string;
  quantity: number;
}

interface CategoryTotalT {
  category: string;
  units: number;
  revenue: number;
}

interface AdminOverviewT {
  items: Array<{ id: string; name: string; price: number; stock: number; category: string }>;
  warehouses: Array<{ id: string; name: string; total: number }>;
  locations: AdminLocationT[];
  revenue: number;
  categories: CategoryTotalT[];
  lowStock: Array<{ id: string; name: string; stock: number }>;
  queueDepth: number;
}

interface FulfilmentQueueT {
  orders: OrderT[];
  depth: number;
}

function useTransientError(): [string, (msg: string) => void] {
  const [message, setMessage] = useState("");
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const show = useCallback((msg: string) => {
    setMessage(msg);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setMessage(""), 5000);
  }, []);
  return [message, show];
}

async function apiFetch(path: string, token: string | null, options: RequestInit = {}) {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers.Authorization = `Bearer ${token}`;
  const res = await fetch(path, { ...options, headers: { ...headers, ...(options.headers as any) } });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || "Request failed");
  return data;
}

export default function App() {
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(TOKEN_KEY));
  const [currentUser, setCurrentUser] = useState<UserT | null>(null);
  const [initializing, setInitializing] = useState(true);
  const [items, setItems] = useState<ItemT[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [cart, setCart] = useState<CartT>({ items: [], total: 0 });
  // Only one of these floating panels is ever shown at a time — the golden
  // path navigates between hook groups via header controls without
  // explicitly closing whatever was open before, so panels must not be able
  // to block each other's or the header's controls.
  const [activeView, setActiveView] = useState<"cart" | "orders" | "admin" | "fulfilment" | null>(null);
  const cartOpen = activeView === "cart";
  const ordersOpen = activeView === "orders";
  const [orders, setOrders] = useState<OrderT[]>([]);
  const adminOpen = activeView === "admin";
  const [adminOverview, setAdminOverview] = useState<AdminOverviewT | null>(null);
  const fulfilmentOpen = activeView === "fulfilment";
  const [fulfilmentQueue, setFulfilmentQueue] = useState<FulfilmentQueueT>({ orders: [], depth: 0 });
  const [recommended, setRecommended] = useState<ItemT[]>([]);
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [itemDetail, setItemDetail] = useState<ItemDetailT | null>(null);

  const [buyError, showBuyError] = useTransientError();
  const [orderError, showOrderError] = useTransientError();

  const socketRef = useRef<Socket | null>(null);

  const saveSession = (tok: string, user: UserT) => {
    localStorage.setItem(TOKEN_KEY, tok);
    setToken(tok);
    setCurrentUser(user);
  };

  const clearSession = () => {
    localStorage.removeItem(TOKEN_KEY);
    setToken(null);
    setCurrentUser(null);
    setCart({ items: [], total: 0 });
    setOrders([]);
    setAdminOverview(null);
    setFulfilmentQueue({ orders: [], depth: 0 });
    setActiveView(null);
  };

  const refreshItems = useCallback(async () => {
    const data = await apiFetch("/api/items", null);
    setItems(data.items);
  }, []);

  const refreshCart = useCallback(async (tok: string) => {
    const data = await apiFetch("/api/cart", tok);
    setCart(data);
  }, []);

  const refreshAdmin = useCallback(async (tok: string) => {
    const data = await apiFetch("/api/admin/overview", tok);
    setAdminOverview(data);
  }, []);

  const refreshFulfilment = useCallback(async (tok: string) => {
    const data = await apiFetch("/api/fulfilment/queue", tok);
    setFulfilmentQueue(data);
  }, []);

  const refreshRecommended = useCallback(async (tok: string | null) => {
    const data = await apiFetch("/api/recommended", tok);
    setRecommended(data.items);
  }, []);

  // Initial load: restore session, fetch the live catalogue, and (if signed
  // in) the account's cart. A page opened fresh always asks for current
  // numbers rather than trusting anything cached.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await refreshItems();
      } catch (err) {
        console.error(err);
      }
      const tok = localStorage.getItem(TOKEN_KEY);
      if (tok) {
        try {
          const me = await apiFetch("/api/auth/me", tok);
          if (!cancelled) {
            setCurrentUser(me.user);
            await refreshCart(tok);
            if (me.user.isAdmin) await refreshAdmin(tok);
            if (me.user.isStaff || me.user.isAdmin) await refreshFulfilment(tok);
            await refreshRecommended(tok);
          }
        } catch {
          clearSession();
        }
      } else {
        await refreshRecommended(null).catch((err) => console.error(err));
      }
      if (!cancelled) setInitializing(false);
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Socket connection follows the current token. On every (re)connect —
  // including after the server was down and the page never reloaded — pull a
  // fresh snapshot instead of trusting whatever events were missed.
  useEffect(() => {
    const socket = io({ auth: token ? { token } : {} });
    socketRef.current = socket;

    socket.on("connect", () => {
      refreshItems().catch((err) => console.error(err));
      if (token) {
        refreshCart(token).catch((err) => console.error(err));
      }
    });

    socket.on("items:update", (data: ItemT[]) => setItems(data));
    socket.on("cart:update", (data: CartT) => setCart(data));
    socket.on("admin:update", (data: AdminOverviewT) => setAdminOverview(data));
    socket.on("orders:update", (data: OrderT[]) => setOrders(data));
    socket.on("fulfilment:update", (data: FulfilmentQueueT) => setFulfilmentQueue(data));
    socket.on("recommended:update", (data: ItemT[]) => setRecommended(data));
    socket.on("reviews:update", (payload: { itemId: string; reviews: ReviewT[]; average: number }) => {
      setItemDetail((prev) => (prev && prev.id === payload.itemId ? { ...prev, reviews: payload.reviews, average: payload.average } : prev));
    });

    return () => {
      socket.disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  useEffect(() => {
    if (currentUser?.isAdmin && token) {
      refreshAdmin(token).catch((err) => console.error(err));
    }
    if ((currentUser?.isStaff || currentUser?.isAdmin) && token) {
      refreshFulfilment(token).catch((err) => console.error(err));
    }
  }, [currentUser, token, refreshAdmin, refreshFulfilment]);

  useEffect(() => {
    if (!selectedItemId) {
      setItemDetail(null);
      return;
    }
    let cancelled = false;
    apiFetch(`/api/items/${selectedItemId}`, token)
      .then((data) => {
        if (!cancelled) setItemDetail(data.item);
      })
      .catch((err) => console.error(err));
    return () => {
      cancelled = true;
    };
  }, [selectedItemId, token]);

  // Escape closes whichever overlay is open.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (selectedItemId) setSelectedItemId(null);
      else if (activeView) setActiveView(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [selectedItemId, activeView]);

  const handleSignUp = async (username: string, password: string) => {
    const data = await apiFetch("/api/auth/signup", null, { method: "POST", body: JSON.stringify({ username, password }) });
    saveSession(data.token, data.user);
  };

  const handleSignIn = async (username: string, password: string) => {
    const data = await apiFetch("/api/auth/signin", null, { method: "POST", body: JSON.stringify({ username, password }) });
    saveSession(data.token, data.user);
  };

  const handleSignOut = () => {
    clearSession();
  };

  const handleBuyNow = async (itemId: string) => {
    try {
      await apiFetch(`/api/items/${itemId}/buy`, token, { method: "POST" });
    } catch (err: any) {
      showBuyError(err.message);
    }
  };

  const handleAddToCart = async (itemId: string) => {
    try {
      const data = await apiFetch("/api/cart", token, { method: "POST", body: JSON.stringify({ itemId, quantity: 1 }) });
      setCart(data);
    } catch (err: any) {
      showBuyError(err.message);
    }
  };

  const handleQuantityChange = async (itemId: string, quantity: number) => {
    try {
      const data = await apiFetch(`/api/cart/${itemId}`, token, { method: "PATCH", body: JSON.stringify({ quantity }) });
      setCart(data);
    } catch (err: any) {
      showBuyError(err.message);
    }
  };

  const handleRemove = async (itemId: string) => {
    try {
      const data = await apiFetch(`/api/cart/${itemId}`, token, { method: "DELETE" });
      setCart(data);
    } catch (err: any) {
      showBuyError(err.message);
    }
  };

  const handleCheckout = async () => {
    try {
      await apiFetch("/api/checkout", token, { method: "POST" });
    } catch (err: any) {
      showBuyError(err.message);
    }
  };

  const openOrders = async () => {
    setActiveView("orders");
    if (token) {
      try {
        const data = await apiFetch("/api/orders", token);
        setOrders(data.orders);
      } catch (err) {
        console.error(err);
      }
    }
  };

  const openAdmin = async () => {
    setActiveView("admin");
    if (token) {
      try {
        await refreshAdmin(token);
      } catch (err) {
        console.error(err);
      }
    }
  };

  const openFulfilment = async () => {
    setActiveView("fulfilment");
    if (token) {
      try {
        await refreshFulfilment(token);
      } catch (err) {
        console.error(err);
      }
    }
  };

  const [reviewError, setReviewError] = useState("");
  const handleReviewSubmit = async (itemId: string, rating: number, comment: string) => {
    setReviewError("");
    try {
      const data = await apiFetch(`/api/items/${itemId}/reviews`, token, {
        method: "POST",
        body: JSON.stringify({ rating, comment }),
      });
      setItemDetail(data.item);
    } catch (err: any) {
      setReviewError(err.message);
    }
  };

  const handleRestock = async (itemId: string, warehouseId: string, quantity: number) => {
    try {
      const data = await apiFetch("/api/admin/restock", token, {
        method: "POST",
        body: JSON.stringify({ itemId, warehouseId, quantity }),
      });
      setAdminOverview(data);
    } catch (err: any) {
      showBuyError(err.message);
    }
  };

  const handleTransfer = async (itemId: string, fromWarehouseId: string, toWarehouseId: string, quantity: number) => {
    try {
      const data = await apiFetch("/api/admin/transfer", token, {
        method: "POST",
        body: JSON.stringify({ itemId, fromWarehouseId, toWarehouseId, quantity }),
      });
      setAdminOverview(data);
    } catch (err: any) {
      showOrderError(err.message);
    }
  };

  const handlePriceChange = async (itemId: string, price: number) => {
    try {
      const data = await apiFetch("/api/admin/price", token, {
        method: "POST",
        body: JSON.stringify({ itemId, price }),
      });
      setAdminOverview(data);
    } catch (err: any) {
      showOrderError(err.message);
    }
  };

  const handleShipOrder = async (orderId: string) => {
    try {
      await apiFetch("/api/fulfilment/ship", token, {
        method: "POST",
        body: JSON.stringify({ orderId }),
      });
    } catch (err: any) {
      showOrderError(err.message);
    }
  };

  const handleCancelOrder = async (orderId: string) => {
    try {
      const data = await apiFetch(`/api/orders/${orderId}/cancel`, token, { method: "POST" });
      setOrders((prev) => prev.map((o) => (o.id === orderId ? data.order : o)));
    } catch (err: any) {
      showOrderError(err.message);
    }
  };

  const handleReturnItem = async (orderId: string, itemId: string) => {
    try {
      const data = await apiFetch(`/api/orders/${orderId}/items/${itemId}/return`, token, { method: "POST" });
      setOrders((prev) => prev.map((o) => (o.id === orderId ? data.order : o)));
    } catch (err: any) {
      showOrderError(err.message);
    }
  };

  const top10 = useMemo(() => items.slice(0, 10), [items]);
  const searchResults = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return [];
    return items.filter((it) => it.name.toLowerCase().includes(q));
  }, [items, searchQuery]);

  const cartCount = cart.items.reduce((s, l) => s + l.quantity, 0);
  const selectedItem = items.find((it) => it.id === selectedItemId) || null;
  const isCustomer = !!currentUser && !currentUser.isAdmin && !currentUser.isStaff;

  return (
    <div className="app">
      {initializing && (
        <div className="loading-screen">
          <div className="spinner" />
          <div>Connecting to MongoDB Shop...</div>
        </div>
      )}

      <header className="header">
        <h1 className="app-title" data-testid="app-title">
          MongoDB Shop
        </h1>
        <input
          className="input search-input"
          data-testid="search-input"
          placeholder="Search items..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setSearchQuery("");
          }}
        />
        <div className="header-spacer" />
        <div className="header-actions">
          <button className="btn btn-ghost" data-testid="cart-toggle" onClick={() => setActiveView("cart")}>
            Cart <span className="badge" data-testid="cart-count">{cartCount}</span>
          </button>
          {currentUser && (
            <button className="btn btn-ghost" data-testid="orders-toggle" onClick={openOrders}>
              Orders
            </button>
          )}
          {currentUser?.isAdmin && (
            <button className="btn btn-ghost" data-testid="admin-link" onClick={openAdmin}>
              Admin
            </button>
          )}
          {(currentUser?.isStaff || currentUser?.isAdmin) && (
            <button className="btn btn-ghost" data-testid="staff-link" onClick={openFulfilment}>
              Fulfilment <span className="badge">{fulfilmentQueue.depth}</span>
            </button>
          )}
          {currentUser ? (
            <>
              <span className="current-user" data-testid="current-user">
                {currentUser.username}
              </span>
              <button className="btn btn-ghost btn-sm" data-testid="signout" onClick={handleSignOut}>
                Sign out
              </button>
            </>
          ) : (
            <AuthBox onSignUp={handleSignUp} onSignIn={handleSignIn} />
          )}
        </div>
      </header>

      <div className="content-row" style={cartOpen || ordersOpen ? { marginRight: 420 } : undefined}>
        <main className="main">
          {buyError && (
            <div className="toast" data-testid="buy-error">
              {buyError}
            </div>
          )}

          {searchQuery.trim() ? (
            <section>
              <h2 className="section-title">Search results</h2>
              {searchResults.length === 0 ? (
                <div className="empty-state">No items match "{searchQuery}"</div>
              ) : (
                <div className="search-results" data-testid="search-results">
                  {searchResults.map((item) => (
                    <ItemCard
                      key={item.id}
                      item={item}
                      isCustomer={isCustomer}
                      onOpen={() => setSelectedItemId(item.id)}
                      onBuy={() => handleBuyNow(item.id)}
                      onAddToCart={() => handleAddToCart(item.id)}
                    />
                  ))}
                </div>
              )}
            </section>
          ) : (
            <section>
              <h2 className="section-title">Best sellers</h2>
              <div className="item-list" data-testid="item-list">
                {top10.map((item) => (
                  <ItemCard
                    key={item.id}
                    item={item}
                    isCustomer={isCustomer}
                    onOpen={() => setSelectedItemId(item.id)}
                    onBuy={() => handleBuyNow(item.id)}
                    onAddToCart={() => handleAddToCart(item.id)}
                  />
                ))}
              </div>
            </section>
          )}

          <section style={{ marginTop: 32 }}>
            <h2 className="section-title">Recommended for you</h2>
            <div className="item-list" data-testid="recommended-list">
              {recommended.length === 0 ? (
                <div className="empty-state">Nothing recommended yet</div>
              ) : (
                recommended.map((item) => (
                  <ItemCard
                    key={item.id}
                    item={item}
                    isCustomer={isCustomer}
                    testId="recommended-item"
                    onOpen={() => setSelectedItemId(item.id)}
                    onBuy={() => handleBuyNow(item.id)}
                    onAddToCart={() => handleAddToCart(item.id)}
                  />
                ))
              )}
            </div>
          </section>
        </main>

        {selectedItem && (
          <ItemDetailPanel
            item={selectedItem}
            detail={itemDetail}
            isCustomer={isCustomer}
            reviewError={reviewError}
            onClose={() => {
              setSelectedItemId(null);
              setReviewError("");
            }}
            onBuy={() => handleBuyNow(selectedItem.id)}
            onAddToCart={() => handleAddToCart(selectedItem.id)}
            onSubmitReview={(rating, comment) => handleReviewSubmit(selectedItem.id, rating, comment)}
          />
        )}
      </div>

      <div className={`backdrop${cartOpen ? " open" : ""}`} onClick={() => setActiveView(null)} />
      <div className={`panel${cartOpen ? " open" : ""}`}>
        <div className="panel-header">
          <h2>Cart</h2>
          <button className="close-btn" onClick={() => setActiveView(null)}>
            ×
          </button>
        </div>
        <div data-testid="cart-panel">
          {cart.items.length === 0 ? (
            <div className="empty-state" data-testid="empty-cart">
              Your cart is empty
            </div>
          ) : (
            <>
              {cart.items.map((line) => (
                <CartLineRow
                  key={line.itemId}
                  line={line}
                  onQuantityChange={(qty) => handleQuantityChange(line.itemId, qty)}
                  onRemove={() => handleRemove(line.itemId)}
                />
              ))}
              <div className="cart-summary">
                <span>Total</span>
                <span data-testid="cart-total">${cart.total.toFixed(2)}</span>
              </div>
              <button className="btn btn-primary" data-testid="checkout-submit" style={{ marginTop: 16, width: "100%" }} onClick={handleCheckout}>
                Checkout
              </button>
            </>
          )}
        </div>
      </div>

      <div className={`backdrop${ordersOpen ? " open" : ""}`} onClick={() => setActiveView(null)} />
      <div className={`panel${ordersOpen ? " open" : ""}`}>
        <div className="panel-header">
          <h2>Order history</h2>
          <button className="close-btn" onClick={() => setActiveView(null)}>
            ×
          </button>
        </div>
        {orderError && (
          <div className="auth-error" data-testid="order-error">
            {orderError}
          </div>
        )}
        <div data-testid="order-list">
          {orders.length === 0 ? (
            <div className="empty-state">You haven't placed any orders yet</div>
          ) : (
            orders.map((order) => (
              <div className="order-item" data-testid="order-item" data-entity-id={String(order.id)} key={order.id}>
                <div className="order-item-header">
                  <span>{new Date(order.createdAt).toLocaleString()}</span>
                  <span data-testid="order-status">{order.status}</span>
                </div>
                <div>{order.items.map((l) => `${l.name} ×${l.quantity}${l.returned ? " (returned)" : ""}`).join(", ")}</div>
                <div className="order-total" data-testid="order-total">
                  ${order.total.toFixed(2)}
                </div>
                <div className="order-item-actions">
                  {order.status === "pending" && (
                    <button className="btn btn-danger btn-sm" data-testid="cancel-order" onClick={() => handleCancelOrder(order.id)}>
                      Cancel order
                    </button>
                  )}
                  {order.status === "shipped" &&
                    order.items
                      .filter((l) => !l.returned)
                      .map((l) => (
                        <button
                          key={l.itemId}
                          className="btn btn-ghost btn-sm"
                          data-testid="return-item"
                          onClick={() => handleReturnItem(order.id, l.itemId)}
                        >
                          Return {l.name}
                        </button>
                      ))}
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {fulfilmentOpen && (currentUser?.isStaff || currentUser?.isAdmin) && (
        <FulfilmentPanel queue={fulfilmentQueue} onClose={() => setActiveView(null)} onShip={handleShipOrder} orderError={orderError} />
      )}

      {adminOpen && currentUser?.isAdmin && (
        <AdminPanel
          overview={adminOverview}
          onClose={() => setActiveView(null)}
          onRestock={handleRestock}
          onTransfer={handleTransfer}
          onPriceChange={handlePriceChange}
          orderError={orderError}
        />
      )}
    </div>
  );
}

function ItemCard({
  item,
  isCustomer,
  onOpen,
  onBuy,
  onAddToCart,
  testId = "item-card",
}: {
  item: ItemT;
  isCustomer: boolean;
  onOpen: () => void;
  onBuy: () => void;
  onAddToCart: () => void;
  testId?: string;
}) {
  const outOfStock = item.stock === 0;
  return (
    <div className={`item-card${outOfStock ? " out-of-stock-card" : ""}`} data-testid={testId} onClick={onOpen}>
      <div className="item-name" data-testid="item-name">
        {item.name}
      </div>
      <div className="item-row">
        <span className="item-price" data-testid="item-price">
          ${item.price.toFixed(2)}
        </span>
      </div>
      <div className="item-row">
        <span className={`item-stock${item.stock > 0 && item.stock <= 5 ? " low" : ""}`} data-testid="item-stock">
          {item.stock}
        </span>
        {outOfStock && (
          <span className="pill-danger" data-testid="out-of-stock">
            Out of stock
          </span>
        )}
      </div>
      {isCustomer && (
        <div className="item-actions" onClick={(e) => e.stopPropagation()}>
          <button className="btn btn-primary btn-sm" data-testid="buy-now" disabled={outOfStock} onClick={onBuy}>
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

function ItemDetailPanel({
  item,
  detail,
  isCustomer,
  reviewError,
  onClose,
  onBuy,
  onAddToCart,
  onSubmitReview,
}: {
  item: ItemT;
  detail: ItemDetailT | null;
  isCustomer: boolean;
  reviewError: string;
  onClose: () => void;
  onBuy: () => void;
  onAddToCart: () => void;
  onSubmitReview: (rating: number, comment: string) => void;
}) {
  const [rating, setRating] = useState(5);
  const [comment, setComment] = useState("");
  const outOfStock = item.stock === 0;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    onSubmitReview(rating, comment);
    setComment("");
  };

  return (
      <div className="item-detail" data-testid="item-detail">
        <div className="panel-header">
          <h2 data-testid="item-name">{item.name}</h2>
          <button className="close-btn" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="item-row">
          <span data-testid="item-price" className="item-price">
            ${item.price.toFixed(2)}
          </span>
          <span data-testid="item-stock" className={item.stock > 0 && item.stock <= 5 ? "item-stock low" : "item-stock"}>
            {item.stock} in stock
          </span>
        </div>
        {outOfStock && (
          <span className="pill-danger" data-testid="out-of-stock">
            Out of stock
          </span>
        )}
        <p className="detail-description">{detail?.description || "Loading description..."}</p>

        {isCustomer && (
          <div className="item-actions">
            <button className="btn btn-primary" data-testid="buy-now" disabled={outOfStock} onClick={onBuy}>
              Buy now
            </button>
            <button className="btn btn-ghost" data-testid="add-to-cart" disabled={outOfStock} onClick={onAddToCart}>
              Add to cart
            </button>
          </div>
        )}

        <h3 style={{ marginTop: 24 }}>
          Reviews — average <span data-testid="review-average">{(detail?.average ?? 0).toFixed(1)}</span>
        </h3>
        {!detail || detail.reviews.length === 0 ? (
          <div className="empty-state">No reviews yet</div>
        ) : (
          detail.reviews.map((r) => (
            <div className="review-item" data-testid="review-item" key={r.id}>
              <div className="review-item-header">
                <span>{r.username}</span>
                <span className="stars">{"★".repeat(r.rating)}</span>
              </div>
              <div>{r.comment}</div>
            </div>
          ))
        )}

        {isCustomer && (
          <form className="review-form" onSubmit={submit}>
            <select data-testid="review-rating" value={rating} onChange={(e) => setRating(Number(e.target.value))}>
              {[5, 4, 3, 2, 1].map((n) => (
                <option key={n} value={n}>
                  {n} star{n > 1 ? "s" : ""}
                </option>
              ))}
            </select>
            <input
              className="input"
              data-testid="review-input"
              placeholder="Write a review..."
              value={comment}
              onChange={(e) => setComment(e.target.value)}
            />
            <button className="btn btn-primary" data-testid="review-submit" type="submit">
              Submit review
            </button>
          </form>
        )}
        {reviewError && (
          <div className="auth-error" data-testid="review-error">
            {reviewError}
          </div>
        )}
      </div>
  );
}

function CartLineRow({
  line,
  onQuantityChange,
  onRemove,
}: {
  line: CartLineT;
  onQuantityChange: (qty: number) => void;
  onRemove: () => void;
}) {
  const [value, setValue] = useState(String(line.quantity));

  useEffect(() => {
    setValue(String(line.quantity));
  }, [line.quantity]);

  const commit = () => {
    const qty = Number(value);
    if (Number.isInteger(qty) && qty >= 1 && qty !== line.quantity) {
      onQuantityChange(qty);
    } else {
      setValue(String(line.quantity));
    }
  };

  return (
    <div className="cart-item" data-testid="cart-item">
      <span className="cart-item-name">{line.name}</span>
      <input
        type="number"
        min={1}
        className="input cart-quantity-input"
        data-testid="cart-quantity"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit();
          }
        }}
      />
      <span className="cart-line-price">${(line.price * line.quantity).toFixed(2)}</span>
      <button className="btn btn-danger btn-sm" data-testid="cart-remove" onClick={onRemove}>
        Remove
      </button>
    </div>
  );
}

function AuthBox({
  onSignUp,
  onSignIn,
}: {
  onSignUp: (username: string, password: string) => Promise<void>;
  onSignIn: (username: string, password: string) => Promise<void>;
}) {
  const [signUpUsername, setSignUpUsername] = useState("");
  const [signUpPassword, setSignUpPassword] = useState("");
  const [signUpError, setSignUpError] = useState("");

  const [showSignIn, setShowSignIn] = useState(false);
  const [signInUsername, setSignInUsername] = useState("");
  const [signInPassword, setSignInPassword] = useState("");
  const [signInError, setSignInError] = useState("");

  const submitSignUp = async (e: React.FormEvent) => {
    e.preventDefault();
    setSignUpError("");
    try {
      await onSignUp(signUpUsername.trim(), signUpPassword);
    } catch (err: any) {
      setSignUpError(err.message);
    }
  };

  const submitSignIn = async (e: React.FormEvent) => {
    e.preventDefault();
    setSignInError("");
    try {
      await onSignIn(signInUsername.trim(), signInPassword);
    } catch (err: any) {
      setSignInError(err.message);
    }
  };

  return (
    <div className="auth-box">
      <form className="auth-form" onSubmit={submitSignUp}>
        <input
          className="input"
          data-testid="signup-username"
          placeholder="Username"
          value={signUpUsername}
          onChange={(e) => setSignUpUsername(e.target.value)}
        />
        <input
          className="input"
          data-testid="signup-password"
          type="password"
          placeholder="Password"
          value={signUpPassword}
          onChange={(e) => setSignUpPassword(e.target.value)}
        />
        <button className="btn btn-primary btn-sm" data-testid="signup-submit" type="submit">
          Sign up
        </button>
        {signUpError && (
          <div className="auth-error" data-testid="auth-error">
            {signUpError}
          </div>
        )}
      </form>
      <button className="link-btn" data-testid="signin-toggle" onClick={() => setShowSignIn((s) => !s)}>
        {showSignIn ? "Hide sign in" : "Already have an account? Sign in"}
      </button>
      {showSignIn && (
        <form className="auth-form" onSubmit={submitSignIn}>
          <input
            className="input"
            data-testid="signin-username"
            placeholder="Username"
            value={signInUsername}
            onChange={(e) => setSignInUsername(e.target.value)}
          />
          <input
            className="input"
            data-testid="signin-password"
            type="password"
            placeholder="Password"
            value={signInPassword}
            onChange={(e) => setSignInPassword(e.target.value)}
          />
          <button className="btn btn-primary btn-sm" data-testid="signin-submit" type="submit">
            Sign in
          </button>
          {signInError && (
            <div className="auth-error" data-testid="auth-error">
              {signInError}
            </div>
          )}
        </form>
      )}
    </div>
  );
}

function AdminPanel({
  overview,
  onClose,
  onRestock,
  onTransfer,
  onPriceChange,
  orderError,
}: {
  overview: AdminOverviewT | null;
  onClose: () => void;
  onRestock: (itemId: string, warehouseId: string, quantity: number) => void;
  onTransfer: (itemId: string, fromWarehouseId: string, toWarehouseId: string, quantity: number) => void;
  onPriceChange: (itemId: string, price: number) => void;
  orderError: string;
}) {
  const [restockValues, setRestockValues] = useState<Record<string, string>>({});
  const [priceValues, setPriceValues] = useState<Record<string, string>>({});
  const [transferValues, setTransferValues] = useState<
    Record<string, { from: string; to: string; qty: string }>
  >({});

  if (!overview) {
    return (
      <div className="admin-panel" data-testid="admin-panel">
        <div className="panel-header">
          <h2>Admin</h2>
          <button className="close-btn" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="empty-state">Loading admin data...</div>
      </div>
    );
  }

  const warehouses = overview.warehouses;

  const transferFor = (itemId: string) =>
    transferValues[itemId] || { from: warehouses[0]?.id || "", to: warehouses[1]?.id || warehouses[0]?.id || "", qty: "" };

  return (
    <div className="admin-panel" data-testid="admin-panel">
      <div className="panel-header">
        <h2>Admin</h2>
        <button className="close-btn" onClick={onClose}>
          ×
        </button>
      </div>

      {orderError && (
        <div className="auth-error" data-testid="order-error">
          {orderError}
        </div>
      )}

      <div className="admin-revenue-box">
        Total revenue: <span className="value" data-testid="admin-revenue">${overview.revenue.toFixed(2)}</span>
      </div>

      <div className="admin-grid">
        <div>
          <h3 className="section-title">Items</h3>
          {overview.items.map((it) => {
            const transfer = transferFor(it.id);
            return (
              <div className="admin-item-row" data-testid="admin-item-row" key={it.id}>
                <span>{it.name}</span>
                <span data-testid="admin-stock">{it.stock}</span>
                <div className="admin-price-controls">
                  <input
                    type="number"
                    min={0.01}
                    step={0.01}
                    className="input price-input"
                    data-testid="price-input"
                    placeholder={it.price.toFixed(2)}
                    value={priceValues[it.id] ?? ""}
                    onChange={(e) => setPriceValues((prev) => ({ ...prev, [it.id]: e.target.value }))}
                  />
                  <button
                    className="btn btn-primary btn-sm"
                    data-testid="price-submit"
                    onClick={() => {
                      const price = Number(priceValues[it.id]);
                      if (Number.isFinite(price) && price > 0) {
                        onPriceChange(it.id, price);
                        setPriceValues((prev) => ({ ...prev, [it.id]: "" }));
                      }
                    }}
                  >
                    Set price
                  </button>
                </div>
                <div className="admin-transfer-controls">
                  <select
                    className="input"
                    data-testid="transfer-from"
                    value={transfer.from}
                    onChange={(e) =>
                      setTransferValues((prev) => ({ ...prev, [it.id]: { ...transferFor(it.id), from: e.target.value } }))
                    }
                  >
                    {warehouses.map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}
                      </option>
                    ))}
                  </select>
                  <select
                    className="input"
                    data-testid="transfer-to"
                    value={transfer.to}
                    onChange={(e) =>
                      setTransferValues((prev) => ({ ...prev, [it.id]: { ...transferFor(it.id), to: e.target.value } }))
                    }
                  >
                    {warehouses.map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}
                      </option>
                    ))}
                  </select>
                  <input
                    type="number"
                    min={1}
                    className="input transfer-qty-input"
                    data-testid="transfer-qty"
                    value={transfer.qty}
                    onChange={(e) =>
                      setTransferValues((prev) => ({ ...prev, [it.id]: { ...transferFor(it.id), qty: e.target.value } }))
                    }
                  />
                  <button
                    className="btn btn-ghost btn-sm"
                    data-testid="transfer-submit"
                    onClick={() => {
                      const qty = Number(transfer.qty);
                      if (Number.isInteger(qty) && qty >= 1 && transfer.from && transfer.to && transfer.from !== transfer.to) {
                        onTransfer(it.id, transfer.from, transfer.to, qty);
                        setTransferValues((prev) => ({ ...prev, [it.id]: { ...transferFor(it.id), qty: "" } }));
                      }
                    }}
                  >
                    Transfer
                  </button>
                </div>
              </div>
            );
          })}
        </div>
        <div>
          <h3 className="section-title">Warehouses</h3>
          <div style={{ marginBottom: 16 }}>
            {overview.warehouses.map((w) => (
              <span className="admin-warehouse-item" data-testid="admin-warehouse-item" key={w.id}>
                {w.name} — <span data-testid="warehouse-total">{w.total}</span>
              </span>
            ))}
          </div>
          <h3 className="section-title">Stock by warehouse</h3>
          {overview.locations.map((loc) => (
            <div className="admin-location-row" data-testid="admin-location-row" key={loc.id}>
              <span>
                {loc.itemName} @ {loc.warehouseName}
              </span>
              <span data-testid="admin-location-qty">{loc.quantity}</span>
              <div className="admin-restock-controls">
                <input
                  type="number"
                  min={1}
                  className="input restock-input"
                  data-testid="restock-input"
                  value={restockValues[loc.id] ?? ""}
                  onChange={(e) => setRestockValues((prev) => ({ ...prev, [loc.id]: e.target.value }))}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      const qty = Number(restockValues[loc.id]);
                      if (Number.isInteger(qty) && qty >= 1) {
                        onRestock(loc.itemId, loc.warehouseId, qty);
                        setRestockValues((prev) => ({ ...prev, [loc.id]: "" }));
                      }
                    }
                  }}
                />
                <button
                  className="btn btn-primary btn-sm"
                  data-testid="restock-submit"
                  onClick={() => {
                    const qty = Number(restockValues[loc.id]);
                    if (Number.isInteger(qty) && qty >= 1) {
                      onRestock(loc.itemId, loc.warehouseId, qty);
                      setRestockValues((prev) => ({ ...prev, [loc.id]: "" }));
                    }
                  }}
                >
                  Restock
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="admin-grid" style={{ marginTop: 24 }}>
        <div>
          <h3 className="section-title">Low stock</h3>
          <div data-testid="low-stock-list">
            {overview.lowStock.length === 0 ? (
              <div className="empty-state">Nothing is running low</div>
            ) : (
              overview.lowStock.map((it) => (
                <div className="admin-item-row" data-testid="low-stock-item" key={it.id}>
                  <span>{it.name}</span>
                  <span>{it.stock}</span>
                </div>
              ))
            )}
          </div>
        </div>
        <div>
          <h3 className="section-title">Category totals</h3>
          {overview.categories.map((c) => (
            <div className="admin-item-row" data-testid="category-row" key={c.category}>
              <span>{c.category}</span>
              <span data-testid="category-units">{c.units}</span>
              <span data-testid="category-revenue">${c.revenue.toFixed(2)}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function FulfilmentPanel({
  queue,
  onClose,
  onShip,
  orderError,
}: {
  queue: FulfilmentQueueT;
  onClose: () => void;
  onShip: (orderId: string) => void;
  orderError: string;
}) {
  return (
    <div className="admin-panel" data-testid="fulfilment-panel">
      <div className="panel-header">
        <h2>Fulfilment queue</h2>
        <button className="close-btn" onClick={onClose}>
          ×
        </button>
      </div>

      {orderError && (
        <div className="auth-error" data-testid="order-error">
          {orderError}
        </div>
      )}

      <div className="admin-revenue-box">
        Orders waiting: <span className="value" data-testid="queue-depth">{queue.depth}</span>
      </div>

      {queue.orders.length === 0 ? (
        <div className="empty-state">Nothing waiting to ship</div>
      ) : (
        queue.orders.map((order) => (
          <div className="order-item" data-testid="queue-item" key={order.id}>
            <div className="order-item-header">
              <span>{new Date(order.createdAt).toLocaleString()}</span>
            </div>
            <div>{order.items.map((l) => `${l.name} ×${l.quantity}`).join(", ")}</div>
            <div>
              {order.items.map((l, idx) => (
                <span className="pill-warning" data-testid="queue-warehouse" key={`${l.itemId}-${idx}`}>
                  {(l.warehouseNames || []).join(", ") || "Unknown"}
                </span>
              ))}
            </div>
            <button className="btn btn-primary btn-sm" data-testid="ship-submit" style={{ marginTop: 8 }} onClick={() => onShip(order.id)}>
              Mark shipped
            </button>
          </div>
        ))
      )}
    </div>
  );
}
