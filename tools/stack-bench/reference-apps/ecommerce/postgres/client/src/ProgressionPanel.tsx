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
  const [restock, setRestock] = useState({ item: "", warehouse: "East", quantity: "", delaySeconds: "90" });
  const [reply, setReply] = useState<Record<number, string>>({});
  const [supportOrders, setSupportOrders] = useState<Record<number, number>>({});
  const [triage, setTriage] = useState<Record<number, {
    assignee: string; priority: string; status: string;
  }>>({});
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
  return <section className="progression" data-role="progression-panel">
    <h2>Progression features</h2>
    <nav className="progression-links">
      <a data-role="profile-link" href="#profile">Profile</a>
      <a data-role="support-link" href="#support">Support</a>
      {staff && <a data-role="promotions-link" href="#promotions">Promotions</a>}
      {staff && <a data-role="activity-link" href="#activity">Activity</a>}
      {staff && <a data-role="reorder-link" href="#reorders">Reorders</a>}
    </nav>
    {message && <p className="progression-error" data-role="progression-error"><span data-role="promotion-error">{message}</span></p>}

    <div className="progression-grid">
      <article className="progression-card">
        <h3>Staff access</h3>
        <input data-role="staff-signin-username" value={signin.username} placeholder="Username"
          onChange={(event) => setSignin({ ...signin, username: event.target.value })} />
        <input data-role="staff-signin-password" value={signin.password} type="password" placeholder="Password"
          onChange={(event) => setSignin({ ...signin, password: event.target.value })} />
        <button data-role="staff-signin-submit" onClick={() => run(async () => {
          await request("/api/auth/signin", "POST", signin); window.location.reload();
        })}>Sign in</button>
        {account && <span data-role="staff-current-user">{account.username}</span>}
      </article>

      {account && <article className="progression-card" id="profile">
        <h3>Profile</h3>
        <input data-role="profile-name" value={profile.name} placeholder="Name"
          onChange={(event) => setProfile({ ...profile, name: event.target.value })} />
        <input data-role="profile-address" value={profile.address} placeholder="Address"
          onChange={(event) => setProfile({ ...profile, address: event.target.value })} />
        <button data-role="profile-save" onClick={() => run(() => request("/api/profile", "PUT", profile))}>Save profile</button>
        <span data-role="profile-address-summary">{state?.profile?.address}</span>
      </article>}

      <article className="progression-card" id="support">
        <h3>Support</h3>
        <input data-role="support-email" value={support.email} placeholder="Email"
          onChange={(event) => setSupport({ ...support, email: event.target.value })} />
        <input data-role="support-subject" value={support.subject} placeholder="Subject"
          onChange={(event) => setSupport({ ...support, subject: event.target.value })} />
        <textarea data-role="support-message" value={support.message} placeholder="Message"
          onChange={(event) => setSupport({ ...support, message: event.target.value })} />
        <button data-role="support-submit" onClick={() => run(async () => {
          const created = await request("/api/support/cases", "POST", support); setCreatedReference(created.reference);
        })}>Open case</button>
        {createdReference && <strong data-role="support-reference">{createdReference}</strong>}
        {(state?.support ?? []).map((entry: any) => {
          const draft = triage[entry.id] ?? {
            assignee: entry.assignee ?? "", priority: entry.priority, status: entry.status,
          };
          const update = (field: keyof typeof draft, value: string) =>
            setTriage((current) => ({ ...current,
              [entry.id]: { ...draft, [field]: value } }));
          return <div data-role="support-ticket" data-entity-id={entry.id} key={entry.id}>
          <strong data-role="support-reference">{entry.reference}</strong>
          <span>{entry.subject}</span>
          <span data-role="support-status">{entry.status}</span>
          {staff && <>
            <input data-role="support-assignee" value={draft.assignee}
              onChange={(event) => update("assignee", event.target.value)} />
            <select data-role="support-priority" value={draft.priority}
              onChange={(event) => update("priority", event.target.value)}>
              <option>low</option><option>normal</option><option>high</option>
            </select>
            <select data-role="support-status-input" value={draft.status}
              onChange={(event) => update("status", event.target.value)}>
              <option>new</option><option>open</option><option>in progress</option><option>resolved</option>
            </select>
            <button data-role="support-update" onClick={() => run(() =>
              request(`/api/support/cases/${entry.id}`, "PUT", draft))}>Update</button>
          </>}
          {account && orders.map((order) => <button data-role="support-order-option" key={order.id}
            onClick={() => setSupportOrders({ ...supportOrders, [entry.id]: order.id })}>
            {order.items?.map((item) => item.name).join(", ") || `Order ${order.id}`}
          </button>)}
          {account && supportOrders[entry.id] && <button data-role="support-link-order"
            data-action-input={JSON.stringify({ caseId: entry.id, orderId: supportOrders[entry.id] })}
            onClick={() => run(() => request(`/api/support/cases/${entry.id}/order`, "POST", { orderId: supportOrders[entry.id] }))}>Link order</button>}
          {entry.orderId && <span data-role="support-order">Order {entry.orderId}</span>}
          {staff && entry.orderId && <button data-role="support-refund"
            data-action-input={JSON.stringify({ caseId: entry.id })}
            onClick={() => run(() => request(`/api/support/cases/${entry.id}/refund`, "POST"))}>Refund order</button>}
          <span data-role="support-refund-total">{entry.refundTotal}</span>
          {entry.replies.map((item: any) => <div data-role="support-reply-item" key={item.id}>{item.username}: {item.message}</div>)}
          {account && <><input data-role="support-reply" value={reply[entry.id] ?? ""}
            onChange={(event) => setReply({ ...reply, [entry.id]: event.target.value })} />
            <button data-role="support-reply-submit" onClick={() => run(() =>
              request(`/api/support/cases/${entry.id}/replies`, "POST", { message: reply[entry.id] }))}>Reply</button></>}
        </div>})}
      </article>

      {account && <article className="progression-card" data-role="notification-settings">
        <h3>Notifications</h3>
        <button data-role="notification-order" data-state={preferences.order ? "on" : "off"} onClick={() => setPreferences({ ...preferences, order: !preferences.order })}>
          Order updates: {preferences.order ? "on" : "off"}
        </button>
        <button data-role="notification-stock" data-state={preferences.stock ? "on" : "off"} onClick={() => setPreferences({ ...preferences, stock: !preferences.stock })}>
          Stock updates: {preferences.stock ? "on" : "off"}
        </button>
        <button data-role="notification-save" onClick={() => run(() =>
          request("/api/notifications/preferences", "PUT", preferences))}>Save</button>
        <span data-role="notifications-toggle">{state?.notifications?.length ?? 0}</span>
        <span data-role="notification-unread-count">{state?.notifications?.filter((item: any) => !item.read).length ?? 0}</span>
        {(state?.notifications ?? []).map((item: any) => <div data-role="notification-item" key={item.id}>{item.message}</div>)}
      </article>}

      {account && !staff && <article className="progression-card" data-role="recommendations">
        <h3>Recommendations</h3>
        {(state?.recommendations ?? []).map((item: any) => <div data-role="recommended-item" key={item.id}>
          <span data-role="recommendation-rank">{item.rank}</span> {item.name}
          <button data-role="dismiss-recommendation" onClick={() => run(() =>
            request(`/api/recommendations/${item.id}/dismiss`, "POST"))}>Dismiss</button>
        </div>)}
      </article>}

      {account && <article className="progression-card">
        <h3>Orders and payment</h3>
        {orders.map((order) => <div data-role="payment-record" key={order.id}>
          <span data-role="payment-status">{order.paymentStatus}</span>
          <span data-role="payment-amount">{order.paymentAmount?.toFixed(2)}</span>
          <span data-role="order-discount">{order.discount?.toFixed(2)}</span>
          <span data-role="order-refund-total">{order.refundTotal?.toFixed(2)}</span>
          {order.refundTotal ? <span data-role="refund-entry">{order.items?.map((item) => item.name).join(", ")} refund {order.refundTotal.toFixed(2)}</span> : null}
        </div>)}
      </article>}

      {(state?.expiredCarts ?? []).map((cart: any) => <article className="progression-card" data-role="expired-cart" key={cart.id}>
        <h3>Expired cart</h3>
        <button data-role="restore-cart" onClick={() => run(() => request(`/api/cart/recover/${cart.id}`, "POST"))}>Restore</button>
        <span data-role="cart-restore-warning">Only available items are restored.</span>
      </article>)}

      {staff && <>
        <article className="progression-card" id="promotions">
          <h3>Catalog</h3>
          <input data-role="catalog-name" value={catalog.name} onChange={(e) => setCatalog({ ...catalog, name: e.target.value })} placeholder="Name" />
          <input data-role="catalog-category" value={catalog.category} onChange={(e) => setCatalog({ ...catalog, category: e.target.value })} placeholder="Category" />
          <input data-role="catalog-price" value={catalog.price} onChange={(e) => setCatalog({ ...catalog, price: e.target.value })} placeholder="Price" />
          <input data-role="catalog-variants" value={catalog.variants} onChange={(e) => setCatalog({ ...catalog, variants: e.target.value })} placeholder="Variants" />
          <button data-role="catalog-save" onClick={() => run(() => request("/api/catalog/products", "POST", catalog))}>Save product</button>
        </article>
        <article className="progression-card" id="activity">
          <h3>Scheduled restock</h3>
          <input data-role="schedule-restock-item" value={restock.item} onChange={(e) => setRestock({ ...restock, item: e.target.value })} />
          <input data-role="schedule-restock-warehouse" value={restock.warehouse} onChange={(e) => setRestock({ ...restock, warehouse: e.target.value })} />
          <input data-role="schedule-restock-qty" value={restock.quantity} onChange={(e) => setRestock({ ...restock, quantity: e.target.value })} />
          <input data-role="schedule-restock-delay" value={restock.delaySeconds} onChange={(e) => setRestock({ ...restock, delaySeconds: e.target.value })} />
          <button data-role="schedule-restock-submit" data-action-input={JSON.stringify({
            item: restock.item, warehouse: restock.warehouse, quantity: Number(restock.quantity),
            delaySeconds: Number(restock.delaySeconds),
          })} onClick={() => run(() =>
            request("/api/admin/scheduled-restocks", "POST", { ...restock, quantity: Number(restock.quantity), delaySeconds: Number(restock.delaySeconds) }))}>Schedule</button>
          {(state?.pendingRestocks ?? []).map((item: any) => <div data-role="pending-restock-item" data-entity-id={String(item.id)} key={item.id}>
            <span>{item.item}</span>
            <span data-role="pending-restock-remaining">{Math.max(0, Math.ceil((new Date(item.dueAt).valueOf() - Date.now()) / 1000))}</span>
            <button data-role="pending-restock-cancel" onClick={() => run(() => request(`/api/admin/scheduled-restocks/${item.id}`, "DELETE"))}>Cancel</button>
          </div>)}
          {(state?.stockLedger ?? []).map((item: any) => <div data-role="stock-ledger-entry" key={item.id}>{item.item} +{item.quantity}</div>)}
        </article>
        <article className="progression-card" id="reorders">
          <h3>Promotions</h3>
          <input data-role="promotion-code" value={promotion.code} onChange={(e) => setPromotion({ ...promotion, code: e.target.value })} />
          <input data-role="promotion-discount" value={promotion.discount} onChange={(e) => setPromotion({ ...promotion, discount: e.target.value })} />
          <input data-role="promotion-start" type="date" value={promotion.start} onChange={(e) => setPromotion({ ...promotion, start: e.target.value })} />
          <input data-role="promotion-end" type="date" value={promotion.end} onChange={(e) => setPromotion({ ...promotion, end: e.target.value })} />
          <input data-role="promotion-limit" value={promotion.limit} onChange={(e) => setPromotion({ ...promotion, limit: e.target.value })} />
          <button data-role="promotion-submit" data-promotion-code={promotion.code} onClick={() => run(() => request("/api/promotions", "POST", promotion))}>Save promotion</button>
          {(state?.promotions ?? []).map((item: any) => <div data-role="promotion-item" key={item.id}>
            <span>{item.code}</span>
            <span data-role="promotion-discount">{item.discount}</span>
            <span data-role="promotion-start">{item.start}</span>
            <span data-role="promotion-end">{item.end}</span>
            <span data-role="promotion-limit">{item.limit}</span>
            <span data-role="promotion-report"><span data-role="promotion-redemptions">{item.redemptions}</span> <span data-role="promotion-revenue">{item.revenue}</span></span>
          </div>)}
        </article>
        <article className="progression-card">
          <h3>Roles and activity</h3>
          {account?.isAdmin && (state?.roles ?? []).map((role: any) => <div data-role="staff-role-row" data-account-id={role.id} key={role.id}>{role.username}
            <select data-role="staff-role-select" defaultValue={role.role} id={`role-${role.id}`}>
              <option>support</option><option>catalog</option><option>inventory</option><option>fulfilment</option>
            </select>
            <button data-role="staff-role-save" onClick={() => run(() => request(`/api/staff/${role.id}/role`, "PUT", {
              role: (document.getElementById(`role-${role.id}`) as HTMLSelectElement).value,
            }))}>Save</button></div>)}
          {(state?.activity ?? []).map((item: any) => <div data-role="activity-entry" key={item.id}>
            <span data-role="activity-actor">{item.actor}</span> <span data-role="activity-action">{item.action}</span>
            <span data-role="activity-subject">{item.subject}</span> <span data-role="activity-time">{item.time}</span>
          </div>)}
        </article>
        <article className="progression-card">
          <h3>Automatic reorder</h3>
          {items.map((item) => <div key={item.id}>
            <span data-role="reorder-item">{item.name}</span>
            <input data-role="reorder-threshold" value={reorders[item.id]?.threshold ?? ""} onChange={(e) => setReorders({ ...reorders, [item.id]: { threshold: e.target.value, quantity: reorders[item.id]?.quantity ?? "" } })} />
            <input data-role="reorder-quantity" value={reorders[item.id]?.quantity ?? ""} onChange={(e) => setReorders({ ...reorders, [item.id]: { threshold: reorders[item.id]?.threshold ?? "", quantity: e.target.value } })} />
            <button data-role="reorder-submit" onClick={() => run(() => request(`/api/reorders/${item.id}`, "PUT", {
              threshold: Number(reorders[item.id]?.threshold), quantity: Number(reorders[item.id]?.quantity),
            }))}>Save</button>
          </div>)}
          {(state?.reorders ?? []).map((item: any) => <div data-role="reorder-rule-item" key={item.id}>{item.item}: {item.threshold}/{item.quantity}</div>)}
        </article>
        <article className="progression-card">
          <h3>Completed orders</h3>
          {(state?.completedOrders ?? []).map((order: any) => <div data-role="completed-order-item" key={order.id}>
            {order.items} <span data-role="completed-order-status">{order.status}</span>
          </div>)}
        </article>
      </>}
    </div>
  </section>;
}
