import React, { useEffect, useMemo, useRef, useState, useCallback } from "react";
import { io, Socket } from "socket.io-client";

const TOKEN_KEY = "mongodb_shop_token";

interface ItemT {
  id: string;
  name: string;
  price: number;
  description?: string;
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
}

interface OrderT {
  id: string;
  items: OrderLineT[];
  total: number;
  createdAt: string;
}

interface UserT {
  id: string;
  username: string;
  isAdmin: boolean;
}

interface AdminLocationT {
  id: string;
  itemId: string;
  itemName: string;
  warehouseId: string;
  warehouseName: string;
  quantity: number;
}

interface AdminOverviewT {
  items: Array<{ id: string; name: string; price: number; stock: number }>;
  warehouses: Array<{ id: string; name: string }>;
  locations: AdminLocationT[];
  revenue: number;
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
  const [activeView, setActiveView] = useState<"cart" | "orders" | "admin" | null>(null);
  const cartOpen = activeView === "cart";
  const ordersOpen = activeView === "orders";
  const [orders, setOrders] = useState<OrderT[]>([]);
  const adminOpen = activeView === "admin";
  const [adminOverview, setAdminOverview] = useState<AdminOverviewT | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [itemDetail, setItemDetail] = useState<ItemDetailT | null>(null);

  const [buyError, showBuyError] = useTransientError();

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
          }
        } catch {
          clearSession();
        }
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
  }, [currentUser, token, refreshAdmin]);

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

  const top10 = useMemo(() => items.slice(0, 10), [items]);
  const searchResults = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return [];
    return items.filter((it) => it.name.toLowerCase().includes(q));
  }, [items, searchQuery]);

  const cartCount = cart.items.reduce((s, l) => s + l.quantity, 0);
  const selectedItem = items.find((it) => it.id === selectedItemId) || null;
  const isCustomer = !!currentUser && !currentUser.isAdmin;

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
        <div data-testid="order-list">
          {orders.length === 0 ? (
            <div className="empty-state">You haven't placed any orders yet</div>
          ) : (
            orders.map((order) => (
              <div className="order-item" data-testid="order-item" key={order.id}>
                <div className="order-item-header">
                  <span>{new Date(order.createdAt).toLocaleString()}</span>
                </div>
                <div>{order.items.map((l) => `${l.name} ×${l.quantity}`).join(", ")}</div>
                <div className="order-total" data-testid="order-total">
                  ${order.total.toFixed(2)}
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      {adminOpen && currentUser?.isAdmin && (
        <AdminPanel overview={adminOverview} onClose={() => setActiveView(null)} onRestock={handleRestock} />
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
}: {
  item: ItemT;
  isCustomer: boolean;
  onOpen: () => void;
  onBuy: () => void;
  onAddToCart: () => void;
}) {
  const outOfStock = item.stock === 0;
  return (
    <div className={`item-card${outOfStock ? " out-of-stock-card" : ""}`} data-testid="item-card" onClick={onOpen}>
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
}: {
  overview: AdminOverviewT | null;
  onClose: () => void;
  onRestock: (itemId: string, warehouseId: string, quantity: number) => void;
}) {
  const [restockValues, setRestockValues] = useState<Record<string, string>>({});

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

  return (
    <div className="admin-panel" data-testid="admin-panel">
      <div className="panel-header">
        <h2>Admin</h2>
        <button className="close-btn" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="admin-revenue-box">
        Total revenue: <span className="value" data-testid="admin-revenue">${overview.revenue.toFixed(2)}</span>
      </div>

      <div className="admin-grid">
        <div>
          <h3 className="section-title">Items</h3>
          {overview.items.map((it) => (
            <div className="admin-item-row" data-testid="admin-item-row" key={it.id}>
              <span>{it.name}</span>
              <span data-testid="admin-stock">{it.stock}</span>
            </div>
          ))}
        </div>
        <div>
          <h3 className="section-title">Warehouses</h3>
          <div style={{ marginBottom: 16 }}>
            {overview.warehouses.map((w) => (
              <span className="admin-warehouse-item" data-testid="admin-warehouse-item" key={w.id}>
                {w.name}
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
    </div>
  );
}
