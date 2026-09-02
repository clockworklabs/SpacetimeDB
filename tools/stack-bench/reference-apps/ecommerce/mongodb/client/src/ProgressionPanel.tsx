import React, { useCallback, useEffect, useState } from "react";

type User = { username: string; isAdmin: boolean; isStaff: boolean; roles?: string[] };
type Item = { id: string; name: string };
type Order = { id: string; items: Array<{ name: string }>; total: number };

async function request(path: string, token: string | null, options: RequestInit = {}) {
  const response = await fetch(path, { ...options, headers: {
    "Content-Type": "application/json", ...(token ? { Authorization: `Bearer ${token}` } : {}),
  } });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error || "Request failed");
  return data;
}

function nameFor(items: Item[], id: unknown) {
  return items.find(item => item.id === String(id))?.name || String(id || "Unknown item");
}

export function ProgressionPanel({ token, user, items, orders, onSignIn, onRefreshItems,
  onRefreshCart, staffOnly = false }: {
  token: string | null;
  user: User | null;
  items: Item[];
  orders: Order[];
  onSignIn: (username: string, password: string) => Promise<void>;
  onRefreshItems: () => Promise<void>;
  onRefreshCart: () => Promise<void>;
  staffOnly?: boolean;
}) {
  const [state, setState] = useState<any>({ tickets: [], promotions: [], notifications: [],
    scheduledRestocks: [], ledger: [], reorderRules: [], activities: [], staffUsers: [], orders: [] });
  const [error, setError] = useState("");
  const [staffName, setStaffName] = useState("");
  const [staffPassword, setStaffPassword] = useState("");
  const [profileOpen, setProfileOpen] = useState(false);
  const [supportOpen, setSupportOpen] = useState(false);
  const [notificationsOpen, setNotificationsOpen] = useState(false);
  const [profileName, setProfileName] = useState("");
  const [profileAddress, setProfileAddress] = useState("");
  const [supportEmail, setSupportEmail] = useState("");
  const [supportSubject, setSupportSubject] = useState("");
  const [supportMessage, setSupportMessage] = useState("");
  const [supportReference, setSupportReference] = useState("");
  const [restoreWarning, setRestoreWarning] = useState("");

  const refresh = useCallback(async () => {
    try {
      const next = await request("/api/progression/state", token);
      setState(next);
      if (next.profile) {
        setProfileName(next.profile.name || "");
        setProfileAddress(next.profile.address || "");
      }
    } catch (err: any) {
      setError(err.message);
    }
  }, [token]);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => clearInterval(timer);
  }, [refresh]);

  const act = async (path: string, options: RequestInit = {}) => {
    setError("");
    try {
      const result = await request(path, token, options);
      await refresh();
      return result;
    } catch (err: any) {
      setError(err.message);
      return null;
    }
  };

  if (staffOnly) {
    if (!(user?.isStaff || user?.isAdmin)) return null;
    return <StaffTools user={user} state={state} items={items}
      orders={orders.length ? orders : state.orders || []} act={act}
      onRefreshItems={onRefreshItems} />;
  }

  const submitSupport = async () => {
    const result = await act("/api/progression/support", { method: "POST", body: JSON.stringify({
      email: supportEmail, subject: supportSubject, message: supportMessage,
    }) });
    if (result) setSupportReference(result.ticket.reference);
  };

  const saveProfile = () => act("/api/progression/profile", { method: "PUT",
    body: JSON.stringify({ name: profileName, address: profileAddress }) });
  const preference = state.preference || { order: false, stock: false };

  return <section className="progression-panel">
    {!user ? <div className="progression-card staff-signin">
      <h3>Staff sign in</h3>
      <input data-testid="staff-signin-username" value={staffName}
        onChange={event => setStaffName(event.target.value)} placeholder="Username" />
      <input data-testid="staff-signin-password" type="password" value={staffPassword}
        onChange={event => setStaffPassword(event.target.value)} placeholder="Password" />
      <button data-testid="staff-signin-submit" className="btn btn-ghost" onClick={() => onSignIn(staffName, staffPassword)}>Sign in</button>
    </div> : <span data-testid="staff-current-user" className="progression-current-user">{user.username}</span>}

    <nav className="progression-nav">
      {user && <button data-testid="profile-link" className="btn btn-ghost" onClick={() => setProfileOpen(value => !value)}>Profile</button>}
      <button data-testid="support-link" className="btn btn-ghost" onClick={() => setSupportOpen(value => !value)}>Support</button>
      {user && <button data-testid="notification-settings" className="btn btn-ghost" onClick={() => setNotificationsOpen(value => !value)}>Notification settings</button>}
      {user && <button data-testid="notifications-toggle" className="btn btn-ghost" onClick={() => setNotificationsOpen(value => !value)}>Notifications</button>}
    </nav>

    {profileOpen && user && <div className="progression-card">
      <h3>Profile</h3>
      <input data-testid="profile-name" value={profileName} onChange={event => setProfileName(event.target.value)} placeholder="Name" />
      <input data-testid="profile-address" value={profileAddress} onChange={event => setProfileAddress(event.target.value)} placeholder="Address" />
      <button data-testid="profile-save" className="btn btn-primary" onClick={saveProfile}>Save</button>
      <p data-testid="profile-address-summary">{state.profile?.address || ""}</p>
    </div>}

    {supportOpen && <div className="progression-card">
      <h3>Support</h3>
      <input data-testid="support-email" value={supportEmail} onChange={event => setSupportEmail(event.target.value)} placeholder="Email" />
      <input data-testid="support-subject" value={supportSubject} onChange={event => setSupportSubject(event.target.value)} placeholder="Subject" />
      <textarea data-testid="support-message" value={supportMessage} onChange={event => setSupportMessage(event.target.value)} placeholder="Message" />
      <button data-testid="support-submit" className="btn btn-primary" onClick={submitSupport}>Submit</button>
      {supportReference && <span data-testid="support-reference">{supportReference}</span>}
      {(state.tickets || []).map((ticket: any) => <SupportTicket key={ticket.id} ticket={ticket}
        user={user} orders={state.orders || orders} act={act} />)}
    </div>}

    {notificationsOpen && user && <div className="progression-card">
      <h3>Notifications</h3>
      <button data-testid="notification-order" className="btn btn-ghost"
        onClick={() => setState((value: any) => ({ ...value, preference: { ...preference, order: !preference.order } }))}>
        Order notifications {preference.order ? "on" : "off"}
      </button>
      <button data-testid="notification-stock" className="btn btn-ghost"
        onClick={() => setState((value: any) => ({ ...value, preference: { ...preference, stock: !preference.stock } }))}>
        Stock notifications {preference.stock ? "on" : "off"}
      </button>
      <span data-testid="notification-unread-count">{(state.notifications || []).length}</span>
      <button data-testid="notification-save" className="btn btn-primary" onClick={() => act("/api/progression/preferences", {
        method: "PUT", body: JSON.stringify(preference),
      })}>Save</button>
      {(state.notifications || []).map((notification: any) =>
        <div data-testid="notification-item" key={notification.id}>{notification.message}</div>)}
    </div>}

    {state.archive && <div data-testid="expired-cart" className="progression-card">
      <span data-testid="cart-expired-notice">Your cart expired</span>
      <button data-testid="restore-cart" className="btn btn-primary" onClick={async () => {
        const result = await act("/api/progression/cart/restore", { method: "POST" });
        if (result) {
          setRestoreWarning(result.unavailable.join(", "));
          await onRefreshCart();
        }
      }}>Restore cart</button>
    </div>}
    {restoreWarning && <div data-testid="cart-restore-warning">{restoreWarning}</div>}
    {error && <div className="auth-error">{error}</div>}
  </section>;
}

function SupportTicket({ ticket, user, orders, act }: any) {
  const [assignee, setAssignee] = useState(ticket.assignee || "");
  const [priority, setPriority] = useState(ticket.priority || "normal");
  const [status, setStatus] = useState(ticket.status || "new");
  const [reply, setReply] = useState("");
  const [orderId, setOrderId] = useState("");
  const staff = user?.isStaff || user?.isAdmin;
  const order = ticket.order;
  const actionInput = JSON.stringify({ caseId: ticket.id, orderId });
  return <article data-testid="support-ticket" data-entity-id={ticket.id} className="support-ticket">
    <strong>{ticket.subject}</strong> <span data-testid="support-status">{ticket.status}</span>
    <span>{ticket.reference}</span>
    {staff && <>
      <input data-testid="support-assignee" value={assignee} onChange={event => setAssignee(event.target.value)} placeholder="Assignee" />
      <input data-testid="support-priority" value={priority} onChange={event => setPriority(event.target.value)} placeholder="Priority" />
      <input data-testid="support-status-input" value={status} onChange={event => setStatus(event.target.value)} placeholder="Status" />
      <button data-testid="support-update" className="btn btn-ghost" onClick={() => act(`/api/progression/support/${ticket.id}`, {
        method: "PATCH", body: JSON.stringify({ assignee, priority, status }),
      })}>Update</button>
    </>}
    {!order && !staff && <div>
      {(orders || []).map((entry: any) => <button key={entry.id} data-testid="support-order-option"
        className="btn btn-ghost btn-sm" onClick={() => setOrderId(entry.id)}>
        {entry.items.map((line: any) => line.name).join(", ")}
      </button>)}
      <button data-testid="support-link-order" data-action-input={actionInput} className="btn btn-ghost"
        onClick={() => act(`/api/support/cases/${ticket.id}/order`, { method: "POST", body: JSON.stringify({ orderId }) })}>
        Link order
      </button>
    </div>}
    {order && <div data-testid="support-order">{order.items.map((line: any) => line.name).join(", ")}</div>}
    {staff && order && <button data-testid="support-refund" data-action-input={JSON.stringify({ caseId: ticket.id })}
      className="btn btn-danger" onClick={() => act(`/api/support/cases/${ticket.id}/refund`, { method: "POST" })}>Refund</button>}
    <span data-testid="support-refund-total">{Number(ticket.refundTotal || 0).toFixed(2)}</span>
    {(ticket.replies || []).map((entry: any) => <div data-testid="support-reply-item" key={entry.id || entry.createdAt}>{entry.username}: {entry.body}</div>)}
    <input data-testid="support-reply" value={reply} onChange={event => setReply(event.target.value)} placeholder="Reply" />
    <button data-testid="support-reply-submit" className="btn btn-ghost" onClick={() => act(`/api/progression/support/${ticket.id}/replies`, {
      method: "POST", body: JSON.stringify({ body: reply }),
    })}>Reply</button>
  </article>;
}

function StaffTools({ user, state, items, orders, act, onRefreshItems }: any) {
  const [section, setSection] = useState("support");
  const [catalog, setCatalog] = useState({ name: "", category: "", price: "", variants: "" });
  const [promotion, setPromotion] = useState({ code: "", discount: "", limit: "",
    start: new Date(Date.now() - 60_000).toISOString().slice(0, 16),
    end: new Date(Date.now() + 86_400_000).toISOString().slice(0, 16) });
  const [restock, setRestock] = useState({ item: "", warehouse: "East", quantity: "", delaySeconds: "" });
  const [reorder, setReorder] = useState({ item: "", threshold: "", quantity: "" });
  const [roles, setRoles] = useState<Record<string, string>>({});
  const findItem = (name: string) => items.find((entry: any) => entry.name === name);

  return <div className="progression-staff-tools">
    <nav className="progression-nav">
      <button data-testid="promotions-link" onClick={() => setSection("promotions")}>Promotions</button>
      <button data-testid="reorder-link" onClick={() => setSection("reorder")}>Reorder</button>
      <button data-testid="activity-link" onClick={() => setSection("activity")}>Activity</button>
    </nav>

    {user.isAdmin && <div className="progression-card">
      <h3>Staff roles</h3>
      {(state.staffUsers || []).map((entry: any) => <div data-testid="staff-role-row" data-account-id={entry.id} key={entry.username}>
        <span>{entry.username} {entry.roles?.join(", ")}</span>
        <input data-testid="staff-role-select"
          value={roles[entry.username] ?? entry.roles?.join(", ") ?? ""}
          onChange={event => setRoles(value => ({ ...value, [entry.username]: event.target.value }))} />
        <button data-testid="staff-role-save" onClick={() => act("/api/progression/staff/roles", {
          method: "POST", body: JSON.stringify({ username: entry.username,
            roles: (roles[entry.username] || "").split(",").map(value => value.trim()).filter(Boolean) }),
        })}>Save</button>
      </div>)}
    </div>}

    {user.isAdmin && <div className="progression-card">
      <h3>Catalog</h3>
      <input data-testid="catalog-name" value={catalog.name} placeholder="Name"
        onChange={event => setCatalog(value => ({ ...value, name: event.target.value }))} />
      <input data-testid="catalog-category" value={catalog.category} placeholder="Category"
        onChange={event => setCatalog(value => ({ ...value, category: event.target.value }))} />
      <input data-testid="catalog-price" value={catalog.price} placeholder="Price"
        onChange={event => setCatalog(value => ({ ...value, price: event.target.value }))} />
      <input data-testid="catalog-variants" value={catalog.variants} placeholder="Variants"
        onChange={event => setCatalog(value => ({ ...value, variants: event.target.value }))} />
      <button data-testid="catalog-save" onClick={async () => {
        await act("/api/progression/catalog", { method: "POST", body: JSON.stringify(catalog) });
        await onRefreshItems();
      }}>Save product</button>
    </div>}

    {(state.tickets || []).map((ticket: any) => <SupportTicket key={ticket.id} ticket={ticket}
      user={user} orders={orders} act={act} />)}

    {section === "promotions" && <div className="progression-card">
      <h3>Promotions</h3>
      <input data-testid="promotion-code" value={promotion.code} placeholder="Code"
        onChange={event => setPromotion(value => ({ ...value, code: event.target.value }))} />
      <input data-testid="promotion-discount" value={promotion.discount} placeholder="Discount"
        onChange={event => setPromotion(value => ({ ...value, discount: event.target.value }))} />
      <input data-testid="promotion-start" type="datetime-local" value={promotion.start}
        onChange={event => setPromotion(value => ({ ...value, start: event.target.value }))} />
      <input data-testid="promotion-end" type="datetime-local" value={promotion.end}
        onChange={event => setPromotion(value => ({ ...value, end: event.target.value }))} />
      <input data-testid="promotion-limit" value={promotion.limit} placeholder="Limit"
        onChange={event => setPromotion(value => ({ ...value, limit: event.target.value }))} />
      <button data-testid="promotion-submit" data-promotion-code={promotion.code} onClick={() => act("/api/progression/promotions", {
        method: "POST", body: JSON.stringify(promotion),
      })}>Save promotion</button>
      {(state.promotions || []).map((entry: any) => <div data-testid="promotion-item" className="promotion-item" key={entry.id}>
        <strong>{entry.code}</strong>
        <span data-testid="promotion-discount">{entry.discount}</span>
        <span data-testid="promotion-start">{entry.start}</span>
        <span data-testid="promotion-end">{entry.end}</span>
        <span data-testid="promotion-limit">{entry.limit}</span>
        <span data-testid="promotion-report">
          <span data-testid="promotion-redemptions">{entry.redemptions}</span>
          <span data-testid="promotion-revenue">{Number(entry.revenue).toFixed(2)}</span>
        </span>
      </div>)}
    </div>}

    <div className="progression-card">
      <h3>Scheduled restocks</h3>
      <input data-testid="schedule-restock-item" value={restock.item} placeholder="Item"
        onChange={event => setRestock(value => ({ ...value, item: event.target.value }))} />
      <input data-testid="schedule-restock-warehouse" value={restock.warehouse} placeholder="Warehouse"
        onChange={event => setRestock(value => ({ ...value, warehouse: event.target.value }))} />
      <input data-testid="schedule-restock-qty" value={restock.quantity} placeholder="Quantity"
        onChange={event => setRestock(value => ({ ...value, quantity: event.target.value }))} />
      <input data-testid="schedule-restock-delay" value={restock.delaySeconds} placeholder="Delay"
        onChange={event => setRestock(value => ({ ...value, delaySeconds: event.target.value }))} />
      <button data-testid="schedule-restock-submit" data-action-input={JSON.stringify(restock)}
        onClick={() => act("/api/admin/scheduled-restocks", { method: "POST", body: JSON.stringify({
          itemId: findItem(restock.item)?.id, warehouseId: state.warehouses?.find((entry: any) => entry.name === restock.warehouse)?.id,
          quantity: Number(restock.quantity), delaySeconds: Number(restock.delaySeconds),
        }) })}>Schedule</button>
      {(state.scheduledRestocks || []).map((entry: any) => <div data-testid="pending-restock-item" data-entity-id={entry.id} key={entry.id}>
        {nameFor(items, entry.itemId)} <span data-testid="pending-restock-remaining">{Math.max(0, Math.ceil((new Date(entry.dueAt).getTime() - Date.now()) / 1000))}</span>
        <button data-testid="pending-restock-cancel" onClick={() => act(`/api/admin/scheduled-restocks/${entry.id}`, { method: "DELETE" })}>Cancel</button>
      </div>)}
      {(state.ledger || []).map((entry: any) => <div data-testid="stock-ledger-entry" key={entry.id}>{nameFor(items, entry.itemId)} +{entry.quantity}</div>)}
    </div>

    {section === "reorder" && <div className="progression-card">
      <input data-testid="reorder-item" value={reorder.item} onChange={event => setReorder(value => ({ ...value, item: event.target.value }))} />
      <input data-testid="reorder-threshold" value={reorder.threshold} onChange={event => setReorder(value => ({ ...value, threshold: event.target.value }))} />
      <input data-testid="reorder-quantity" value={reorder.quantity} onChange={event => setReorder(value => ({ ...value, quantity: event.target.value }))} />
      <button data-testid="reorder-submit" onClick={() => act("/api/progression/reorder-rules", { method: "POST", body: JSON.stringify({
        itemId: findItem(reorder.item)?.id, threshold: Number(reorder.threshold), quantity: Number(reorder.quantity),
      }) })}>Save rule</button>
      {(state.reorderRules || []).map((entry: any) => <div data-testid="reorder-rule-item" key={entry.id}>{nameFor(items, entry.itemId)} {entry.threshold} / {entry.quantity}</div>)}
    </div>}

    {section === "activity" && <div className="progression-card">
      {(state.activities || []).map((entry: any) => <div data-testid="activity-entry" key={entry.id}>
        <span data-testid="activity-actor">{entry.actor}</span>
        <span data-testid="activity-action">{entry.action}</span>
        <span data-testid="activity-subject">{entry.subject}</span>
        <time data-testid="activity-time">{entry.createdAt}</time>
      </div>)}
    </div>}

    {(orders || []).filter((order: any) => order.status === "delivered").map((order: any) =>
      <div data-testid="completed-order-item" key={order.id}>{order.items.map((line: any) => line.name).join(", ")}
        <span data-testid="completed-order-status">{order.status}</span></div>)}
  </div>;
}
