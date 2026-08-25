import { useEffect, useState } from "react";

type Account = { id: number; username: string; isAdmin: boolean; isStaff: boolean } | null;
type Item = { id: number; name: string; price: number; stock: number; category: string; variants?: string[] };
type Order = {
  id: number; status: string; total: number; discount?: number; paymentStatus?: string;
  paymentAmount?: number; refundTotal?: number; items?: Array<{ name: string }>;
};
type ProgressionState = any;

async function request(path: string, method = "GET", body?: unknown) {
  const response = await fetch(path, {
    method,
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const result = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(result.error || "request failed");
  return result;
}

export function ProgressionPanel({
  account, items, orders, state, reload,
}: {
  account: Account; items: Item[]; orders: Order[]; state: ProgressionState; reload: () => Promise<void>;
}) {
  const staff = Boolean(account?.isAdmin || account?.isStaff);
  const [message, setMessage] = useState("");
  const [profile, setProfile] = useState({ name: state?.profile?.name ?? "", address: state?.profile?.address ?? "" });
  const [signin, setSignin] = useState({ username: "", password: "" });
  const [support, setSupport] = useState({ email: "", subject: "", message: "" });
  const [createdReference, setCreatedReference] = useState("");
  const [catalog, setCatalog] = useState({ name: "", category: "", price: "", variants: "" });
  const [promotion, setPromotion] = useState({ code: "", discount: "", start: "", end: "", limit: "" });
  const [promotionCode, setPromotionCode] = useState("");
  const [restock, setRestock] = useState({ item: "", warehouse: "East", quantity: "", delaySeconds: "90" });
  const [reply, setReply] = useState<Record<number, string>>({});
  const [supportOrders, setSupportOrders] = useState<Record<number, number>>({});
  const [reorders, setReorders] = useState<Record<number, { threshold: string; quantity: string }>>({});
  const [preferences, setPreferences] = useState({ order: false, stock: false });

  useEffect(() => {
    if (state?.preferences) setPreferences(state.preferences);
  }, [state?.preferences?.order, state?.preferences?.stock]);
  useEffect(() => {
    if (state?.profile) setProfile({ name: state.profile.name, address: state.profile.address });
  }, [state?.profile?.name, state?.profile?.address]);

  const run = async (work: () => Promise<unknown>) => {
    try { await work(); setMessage(""); await reload(); }
    catch (error) { setMessage(error instanceof Error ? error.message : "request failed"); }
  };
  return <section className="progression" data-testid="progression-panel">
    <h2>Progression features</h2>
    <nav className="progression-links">
      <a data-testid="profile-link" href="#profile">Profile</a>
      <a data-testid="support-link" href="#support">Support</a>
      {staff && <a data-testid="promotions-link" href="#promotions">Promotions</a>}
      {staff && <a data-testid="activity-link" href="#activity">Activity</a>}
      {staff && <a data-testid="reorder-link" href="#reorders">Reorders</a>}
    </nav>
    {message && <p className="progression-error" data-testid="progression-error"><span data-testid="promotion-error">{message}</span></p>}

    <div className="progression-grid">
      <article className="progression-card">
        <h3>Staff access</h3>
        <input data-testid="staff-signin-username" value={signin.username} placeholder="Username"
          onChange={(event) => setSignin({ ...signin, username: event.target.value })} />
        <input data-testid="staff-signin-password" value={signin.password} type="password" placeholder="Password"
          onChange={(event) => setSignin({ ...signin, password: event.target.value })} />
        <button data-testid="staff-signin-submit" onClick={() => run(async () => {
          await request("/api/auth/signin", "POST", signin); window.location.reload();
        })}>Sign in</button>
        {account && <span data-testid="staff-current-user">{account.username}</span>}
      </article>

      {account && <article className="progression-card" id="profile">
        <h3>Profile</h3>
        <input data-testid="profile-name" value={profile.name} placeholder="Name"
          onChange={(event) => setProfile({ ...profile, name: event.target.value })} />
        <input data-testid="profile-address" value={profile.address} placeholder="Address"
          onChange={(event) => setProfile({ ...profile, address: event.target.value })} />
        <button data-testid="profile-save" onClick={() => run(() => request("/api/profile", "PUT", profile))}>Save profile</button>
        <span data-testid="profile-address-summary">{state?.profile?.address}</span>
      </article>}

      <article className="progression-card" id="support">
        <h3>Support</h3>
        <input data-testid="support-email" value={support.email} placeholder="Email"
          onChange={(event) => setSupport({ ...support, email: event.target.value })} />
        <input data-testid="support-subject" value={support.subject} placeholder="Subject"
          onChange={(event) => setSupport({ ...support, subject: event.target.value })} />
        <textarea data-testid="support-message" value={support.message} placeholder="Message"
          onChange={(event) => setSupport({ ...support, message: event.target.value })} />
        <button data-testid="support-submit" onClick={() => run(async () => {
          const created = await request("/api/support/cases", "POST", support); setCreatedReference(created.reference);
        })}>Open case</button>
        {createdReference && <strong data-testid="support-reference">{createdReference}</strong>}
        {(state?.support ?? []).map((entry: any) => <div data-testid="support-ticket" key={entry.id}>
          <strong data-testid="support-reference">{entry.reference}</strong>
          <span data-testid="support-status">{entry.status}</span>
          {staff && <>
            <input data-testid="support-assignee" defaultValue={entry.assignee} onBlur={(event) => run(() =>
              request(`/api/support/cases/${entry.id}`, "PUT", { assignee: event.target.value }))} />
            <select data-testid="support-priority" defaultValue={entry.priority} onChange={(event) => run(() =>
              request(`/api/support/cases/${entry.id}`, "PUT", { priority: event.target.value }))}>
              <option>low</option><option>normal</option><option>high</option>
            </select>
            <select data-testid="support-status-input" defaultValue={entry.status} onChange={(event) => run(() =>
              request(`/api/support/cases/${entry.id}`, "PUT", { status: event.target.value }))}>
              <option>new</option><option>open</option><option>resolved</option>
            </select>
            <button data-testid="support-update">Update</button>
          </>}
          {account && <select data-testid="support-order-option" value={supportOrders[entry.id] ?? ""}
            onChange={(event) => setSupportOrders({ ...supportOrders, [entry.id]: Number(event.target.value) })}>
            <option value="">Select order</option>{orders.map((order) => <option value={order.id} key={order.id}>{order.id}</option>)}
          </select>}
          {account && supportOrders[entry.id] && <button data-testid="support-link-order"
            data-action-input={JSON.stringify({ caseId: entry.id, orderId: supportOrders[entry.id] })}
            onClick={() => run(() => request(`/api/support/cases/${entry.id}/order`, "POST", { orderId: supportOrders[entry.id] }))}>Link order</button>}
          {entry.orderId && <span data-testid="support-order">Order {entry.orderId}</span>}
          {staff && entry.orderId && <button data-testid="support-refund"
            data-action-input={JSON.stringify({ caseId: entry.id })}
            onClick={() => run(() => request(`/api/support/cases/${entry.id}/refund`, "POST"))}>Refund order</button>}
          <span data-testid="support-refund-total">{entry.refundTotal}</span>
          {entry.replies.map((item: any) => <div data-testid="support-reply-item" key={item.id}>{item.username}: {item.message}</div>)}
          {account && <><input data-testid="support-reply" value={reply[entry.id] ?? ""}
            onChange={(event) => setReply({ ...reply, [entry.id]: event.target.value })} />
            <button data-testid="support-reply-submit" onClick={() => run(() =>
              request(`/api/support/cases/${entry.id}/replies`, "POST", { message: reply[entry.id] }))}>Reply</button></>}
        </div>)}
      </article>

      {account && <article className="progression-card" data-testid="notification-settings">
        <h3>Notifications</h3>
        <button data-testid="notification-order" onClick={() => setPreferences({ ...preferences, order: !preferences.order })}>
          Order updates: {preferences.order ? "on" : "off"}
        </button>
        <button data-testid="notification-stock" onClick={() => setPreferences({ ...preferences, stock: !preferences.stock })}>
          Stock updates: {preferences.stock ? "on" : "off"}
        </button>
        <button data-testid="notification-save" onClick={() => run(() =>
          request("/api/notifications/preferences", "PUT", preferences))}>Save</button>
        <span data-testid="notifications-toggle">{state?.notifications?.length ?? 0}</span>
        <span data-testid="notification-unread-count">{state?.notifications?.filter((item: any) => !item.read).length ?? 0}</span>
        {(state?.notifications ?? []).map((item: any) => <div data-testid="notification-item" key={item.id}>{item.message}</div>)}
      </article>}

      {account && !staff && <article className="progression-card" data-testid="recommendations">
        <h3>Recommendations</h3>
        {(state?.recommendations ?? []).map((item: any) => <div data-testid="recommendation-item" key={item.id}>
          <span data-testid="recommendation-rank">{item.rank}</span> {item.name}
          <button data-testid="dismiss-recommendation" onClick={() => run(() =>
            request(`/api/recommendations/${item.id}/dismiss`, "POST"))}>Dismiss</button>
        </div>)}
      </article>}

      {account && <article className="progression-card">
        <h3>Orders and payment</h3>
        {orders.map((order) => <div data-testid="payment-record" key={order.id}>
          <span data-testid="payment-status">{order.paymentStatus}</span>
          <span data-testid="payment-amount">{order.paymentAmount?.toFixed(2)}</span>
          <span data-testid="order-discount">{order.discount?.toFixed(2)}</span>
          <span data-testid="order-refund-total">{order.refundTotal?.toFixed(2)}</span>
          {order.refundTotal ? <span data-testid="refund-entry">{order.items?.map((item) => item.name).join(", ")} refund {order.refundTotal.toFixed(2)}</span> : null}
        </div>)}
        <input data-testid="cart-promotion" value={promotionCode} placeholder="Promotion code"
          onChange={(event) => setPromotionCode(event.target.value)} />
        <button data-testid="apply-promotion" onClick={() => run(() =>
          request("/api/cart/promotion", "POST", { code: promotionCode }))}>Apply</button>
      </article>}

      {(state?.expiredCarts ?? []).map((cart: any) => <article className="progression-card" data-testid="expired-cart" key={cart.id}>
        <h3>Expired cart</h3>
        <button data-testid="restore-cart" onClick={() => run(() => request(`/api/cart/recover/${cart.id}`, "POST"))}>Restore</button>
        <span data-testid="cart-restore-warning">Only available items are restored.</span>
      </article>)}

      {staff && <>
        <article className="progression-card" id="promotions">
          <h3>Catalog</h3>
          <input data-testid="catalog-name" value={catalog.name} onChange={(e) => setCatalog({ ...catalog, name: e.target.value })} placeholder="Name" />
          <input data-testid="catalog-category" value={catalog.category} onChange={(e) => setCatalog({ ...catalog, category: e.target.value })} placeholder="Category" />
          <input data-testid="catalog-price" value={catalog.price} onChange={(e) => setCatalog({ ...catalog, price: e.target.value })} placeholder="Price" />
          <input data-testid="catalog-variants" value={catalog.variants} onChange={(e) => setCatalog({ ...catalog, variants: e.target.value })} placeholder="Variants" />
          <button data-testid="catalog-save" onClick={() => run(() => request("/api/catalog/products", "POST", catalog))}>Save product</button>
        </article>
        <article className="progression-card" id="activity">
          <h3>Scheduled restock</h3>
          <input data-testid="schedule-restock-item" value={restock.item} onChange={(e) => setRestock({ ...restock, item: e.target.value })} />
          <input data-testid="schedule-restock-warehouse" value={restock.warehouse} onChange={(e) => setRestock({ ...restock, warehouse: e.target.value })} />
          <input data-testid="schedule-restock-qty" value={restock.quantity} onChange={(e) => setRestock({ ...restock, quantity: e.target.value })} />
          <input data-testid="schedule-restock-delay" value={restock.delaySeconds} onChange={(e) => setRestock({ ...restock, delaySeconds: e.target.value })} />
          <button data-testid="schedule-restock-submit" data-action-input={JSON.stringify({
            item: restock.item, warehouse: restock.warehouse, quantity: Number(restock.quantity),
            delaySeconds: Number(restock.delaySeconds),
          })} onClick={() => run(() =>
            request("/api/admin/scheduled-restocks", "POST", { ...restock, quantity: Number(restock.quantity), delaySeconds: Number(restock.delaySeconds) }))}>Schedule</button>
          {(state?.pendingRestocks ?? []).map((item: any) => <div key={item.id}>
            <span data-testid="pending-restock-item">{item.item}</span>
            <span data-testid="pending-restock-remaining">{Math.max(0, Math.ceil((new Date(item.dueAt).valueOf() - Date.now()) / 1000))}</span>
            <button data-testid="pending-restock-cancel" onClick={() => run(() => request(`/api/admin/scheduled-restocks/${item.id}`, "DELETE"))}>Cancel</button>
          </div>)}
          {(state?.stockLedger ?? []).map((item: any) => <div data-testid="stock-ledger-entry" key={item.id}>{item.item} +{item.quantity}</div>)}
        </article>
        <article className="progression-card" id="reorders">
          <h3>Promotions</h3>
          <input data-testid="promotion-code" value={promotion.code} onChange={(e) => setPromotion({ ...promotion, code: e.target.value })} />
          <input data-testid="promotion-discount" value={promotion.discount} onChange={(e) => setPromotion({ ...promotion, discount: e.target.value })} />
          <input data-testid="promotion-start" type="date" value={promotion.start} onChange={(e) => setPromotion({ ...promotion, start: e.target.value })} />
          <input data-testid="promotion-end" type="date" value={promotion.end} onChange={(e) => setPromotion({ ...promotion, end: e.target.value })} />
          <input data-testid="promotion-limit" value={promotion.limit} onChange={(e) => setPromotion({ ...promotion, limit: e.target.value })} />
          <button data-testid="promotion-submit" onClick={() => run(() => request("/api/promotions", "POST", promotion))}>Save promotion</button>
          {(state?.promotions ?? []).map((item: any) => <div data-testid="promotion-item" key={item.id}>
            <span data-testid="promotion-report">{item.code} <span data-testid="promotion-redemptions">{item.redemptions}</span> <span data-testid="promotion-revenue">{item.revenue}</span></span>
          </div>)}
        </article>
        <article className="progression-card">
          <h3>Roles and activity</h3>
          {(state?.roles ?? []).map((role: any) => <div data-testid="staff-role-row" key={role.id}>{role.username}
            <select data-testid="staff-role-select" defaultValue={role.role} id={`role-${role.id}`}><option>support</option><option>catalog</option><option>fulfilment</option></select>
            <button data-testid="staff-role-save" onClick={() => run(() => request(`/api/staff/${role.id}/role`, "PUT", {
              role: (document.getElementById(`role-${role.id}`) as HTMLSelectElement).value,
            }))}>Save</button></div>)}
          {(state?.activity ?? []).map((item: any) => <div data-testid="activity-entry" key={item.id}>
            <span data-testid="activity-actor">{item.actor}</span> <span data-testid="activity-action">{item.action}</span>
            <span data-testid="activity-subject">{item.subject}</span> <span data-testid="activity-time">{item.time}</span>
          </div>)}
        </article>
        <article className="progression-card">
          <h3>Automatic reorder</h3>
          {items.map((item) => <div key={item.id}>
            <span data-testid="reorder-item">{item.name}</span>
            <input data-testid="reorder-threshold" value={reorders[item.id]?.threshold ?? ""} onChange={(e) => setReorders({ ...reorders, [item.id]: { threshold: e.target.value, quantity: reorders[item.id]?.quantity ?? "" } })} />
            <input data-testid="reorder-quantity" value={reorders[item.id]?.quantity ?? ""} onChange={(e) => setReorders({ ...reorders, [item.id]: { threshold: reorders[item.id]?.threshold ?? "", quantity: e.target.value } })} />
            <button data-testid="reorder-submit" onClick={() => run(() => request(`/api/reorders/${item.id}`, "PUT", {
              threshold: Number(reorders[item.id]?.threshold), quantity: Number(reorders[item.id]?.quantity),
            }))}>Save</button>
          </div>)}
          {(state?.reorders ?? []).map((item: any) => <div data-testid="reorder-rule-item" key={item.id}>{item.item}: {item.threshold}/{item.quantity}</div>)}
        </article>
        <article className="progression-card">
          <h3>Completed orders</h3>
          {(state?.completedOrders ?? []).map((order: any) => <div data-testid="completed-order-item" key={order.id}>
            {order.items} <span data-testid="completed-order-status">{order.status}</span>
          </div>)}
        </article>
      </>}
    </div>
  </section>;
}
