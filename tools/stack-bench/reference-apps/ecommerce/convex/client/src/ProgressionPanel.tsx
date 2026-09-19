import { mutate, subscribeProgression } from "./request";
import { useEffect, useState } from "react";

type User = { username: string; isAdmin: boolean; isStaff: boolean; roles?: string[] };
type Item = { id: string; name: string };
type Order = { id: string; items: Array<{ name: string }>; total: number };


function nameFor(items: Item[], id: unknown) {
  return items.find(item => item.id === String(id))?.name || String(id || "Unknown item");
}

export function ProgressionPanel({ token, user, items, orders, onSignIn, staffOnly = false }: {
  token: string | null;
  user: User | null;
  items: Item[];
  orders: Order[];
  onSignIn: (username: string, password: string) => Promise<void>;
  staffOnly?: boolean;
}) {
  const [state, setState] = useState<any>({ tickets: [], promotions: [], notifications: [],
    scheduledRestocks: [], ledger: [], staffUsers: [] });
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

  useEffect(() => subscribeProgression(next => {
    setState({ ...next, loadedToken: token });
    if (next.profile) { setProfileName(next.profile.name || ""); setProfileAddress(next.profile.address || ""); }
  }), [token]);


  const act = async (name: string, args: Record<string, unknown> = {}) => {
    setError("");
    try {
      return await mutate(name, args);
    } catch (err: any) {
      setError(err.message);
      return null;
    }
  };

  if (staffOnly) {
    if (!(user?.isStaff || user?.isAdmin)) return null;
    return <StaffTools user={user} state={state} items={items}
      orders={orders} act={act} />;
  }

  const submitSupport = async () => {
    const result = await act("progression:submitSupport", {
      email: supportEmail, subject: supportSubject, message: supportMessage,
    });
    if (result) setSupportReference(result.ticket.reference);
  };

  const saveProfile = () => act("progression:saveProfile", { name: profileName, address: profileAddress });
  const preference = state.preference || { order: false, stock: false };

  return <section className="progression-panel">
    {!user ? <div className="progression-card staff-signin">
      <h3>Staff sign in</h3>
      <input data-role="staff-signin-username" value={staffName}
        onChange={event => setStaffName(event.target.value)} placeholder="Username" />
      <input data-role="staff-signin-password" type="password" value={staffPassword}
        onChange={event => setStaffPassword(event.target.value)} placeholder="Password" />
      <button data-role="staff-signin-submit" className="btn btn-ghost" onClick={() => onSignIn(staffName, staffPassword)}>Sign in</button>
    </div> : <span data-role="staff-current-user" className="progression-current-user">{user.username}</span>}

    <nav className="progression-nav">
      {user && <button data-role="profile-link" className="btn btn-ghost" onClick={() => setProfileOpen(true)}>Profile</button>}
      <button data-role="support-link" className="btn btn-ghost" onClick={() => setSupportOpen(true)}>Support</button>
      {user && <button data-role="notification-settings" className="btn btn-ghost" onClick={() => setNotificationsOpen(true)}>Notification settings</button>}
      {user && <button data-role="notifications-toggle" className="btn btn-ghost" onClick={() => setNotificationsOpen(true)}>Notifications</button>}
    </nav>

    {profileOpen && user && <div className="progression-card">
      <h3>Profile</h3>
      <input data-role="profile-name" value={profileName} onChange={event => setProfileName(event.target.value)} placeholder="Name" />
      <input data-role="profile-address" value={profileAddress} onChange={event => setProfileAddress(event.target.value)} placeholder="Address" />
      <button data-role="profile-save" className="btn btn-primary" onClick={saveProfile}>Save</button>
      <p data-role="profile-address-summary">{state.profile?.address || ""}</p>
    </div>}

    {supportOpen && <div className="progression-card">
      <h3>Support</h3>
      <input data-role="support-email" value={supportEmail} onChange={event => setSupportEmail(event.target.value)} placeholder="Email" />
      <input data-role="support-subject" value={supportSubject} onChange={event => setSupportSubject(event.target.value)} placeholder="Subject" />
      <textarea data-role="support-message" value={supportMessage} onChange={event => setSupportMessage(event.target.value)} placeholder="Message" />
      <button data-role="support-submit" className="btn btn-primary" onClick={submitSupport}>Submit</button>
      {supportReference && <span data-role="support-reference">{supportReference}</span>}
      {(state.tickets || []).map((ticket: any) => <SupportTicket key={ticket.id} ticket={ticket}
        user={user} act={act} />)}
    </div>}

    {notificationsOpen && user && <div className="progression-card"
      data-role="notifications-panel" aria-busy={state.loadedToken !== token || !Array.isArray(state.notifications)}>
      <h3>Notifications</h3>
      <button data-role="notification-order" data-state={preference.order ? "on" : "off"} className="btn btn-ghost"
        onClick={() => setState((value: any) => ({ ...value, preference: { ...preference, order: !preference.order } }))}>
        Order notifications {preference.order ? "on" : "off"}
      </button>
      <button data-role="notification-stock" data-state={preference.stock ? "on" : "off"} className="btn btn-ghost"
        onClick={() => setState((value: any) => ({ ...value, preference: { ...preference, stock: !preference.stock } }))}>
        Stock notifications {preference.stock ? "on" : "off"}
      </button>
      <span data-role="notification-unread-count">{(state.notifications || []).length}</span>
      <button data-role="notification-save" className="btn btn-primary" onClick={() => act("progression:savePreferences", preference)}>Save</button>
      {(state.notifications || []).map((notification: any) =>
        <div data-role="notification-item" key={notification.id}><span data-role={notification.type === 'stock' ? 'stock-alert-delivery' : undefined}>{notification.message}</span></div>)}
    </div>}

    {error && <div className="auth-error">{error}</div>}
  </section>;
}

function SupportTicket({ ticket, user, act }: any) {
  const [assignee, setAssignee] = useState(ticket.assignee || "");
  const [priority, setPriority] = useState(ticket.priority || "normal");
  const [status, setStatus] = useState(ticket.status || "new");
  const [reply, setReply] = useState("");
  const staff = user?.isStaff || user?.isAdmin;
  return <article data-role="support-ticket" data-entity-id={ticket.id} data-refund-input={JSON.stringify({caseId: ticket.id})} className="support-ticket">
    <strong>{ticket.subject}</strong> <span data-role="support-status">{ticket.status}</span>
    <span>{ticket.reference}</span>
    {staff && <>
      <input data-role="support-assignee" value={assignee} onChange={event => setAssignee(event.target.value)} placeholder="Assignee" />
      <input data-role="support-priority" value={priority} onChange={event => setPriority(event.target.value)} placeholder="Priority" />
      <input data-role="support-status-input" value={status} onChange={event => setStatus(event.target.value)} placeholder="Status" />
      <button data-role="support-update" className="btn btn-ghost" onClick={() => act("progression:updateSupport", { ticketId: ticket.id, assignee, priority, status })}>Update</button>
    </>}
    {(ticket.replies || []).map((entry: any) => <div data-role="support-reply-item" key={entry.id || entry.createdAt}>{entry.username}: {entry.body}</div>)}
    <input data-role="support-reply" value={reply} onChange={event => setReply(event.target.value)} placeholder="Reply" />
    <button data-role="support-reply-submit" className="btn btn-ghost" onClick={() => act("api:reply_support", { ticketId: ticket.id, body: reply })}>Reply</button>
  </article>;
}

function StaffTools({ user, state, items, orders, act }: any) {
  const [section, setSection] = useState("support");
  const [catalog, setCatalog] = useState({ name: "", category: "", price: "", variants: "" });
  const [promotion, setPromotion] = useState({ code: "", discount: "", limit: "",
    start: new Date(Date.now() - 60_000).toISOString().slice(0, 10),
    end: new Date(Date.now() + 86_400_000).toISOString().slice(0, 10) });
  const [restock, setRestock] = useState({ item: "", warehouse: "East", quantity: "", delaySeconds: "" });
  const [roles, setRoles] = useState<Record<string, string>>({});
  const [now, setNow] = useState(Date.now);
  const hasPendingRestocks = (state.scheduledRestocks || []).length > 0;
  useEffect(() => {
    if (!hasPendingRestocks) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [hasPendingRestocks]);

  return <div className="progression-staff-tools">
    <nav className="progression-nav">
      <button data-role="promotions-link" onClick={() => setSection("promotions")}>Promotions</button>
    </nav>

    {user.isAdmin && <div className="progression-card">
      <h3>Staff roles</h3>
      {(state.staffUsers || []).map((entry: any) => <div data-role="staff-role-row" id={`staff-role-account-${encodeURIComponent(entry.username)}`} data-account-id={entry.id} key={entry.username}>
        <span>{entry.username} {entry.roles?.join(", ")}</span>
        <select data-role="staff-role-select"
          value={roles[entry.username] ?? entry.roles?.[0] ?? "staff"}
          onChange={event => setRoles(value => ({ ...value, [entry.username]: event.target.value }))}>
          <option>staff</option><option>inventory</option><option>admin</option>
        </select>
        <button data-role="staff-role-save" onClick={() => act("api:assign_staff_role", {
          accountId: entry.id, role: roles[entry.username] ?? entry.roles?.[0] ?? "staff",
        })}>Save</button>
      </div>)}
    </div>}

    {user.isAdmin && <div className="progression-card">
      <h3>Catalog</h3>
      <input data-role="catalog-name" value={catalog.name} placeholder="Name"
        onChange={event => setCatalog(value => ({ ...value, name: event.target.value }))} />
      <input data-role="catalog-category" value={catalog.category} placeholder="Category"
        onChange={event => setCatalog(value => ({ ...value, category: event.target.value }))} />
      <input data-role="catalog-price" value={catalog.price} placeholder="Price"
        onChange={event => setCatalog(value => ({ ...value, price: event.target.value }))} />
      <input data-role="catalog-variants" value={catalog.variants} placeholder="Variants"
        onChange={event => setCatalog(value => ({ ...value, variants: event.target.value }))} />
      <button data-role="catalog-save" onClick={async () => {
        await act("progression:saveCatalog", { ...catalog, price: Number(catalog.price),
          variants: catalog.variants.split(",").map(value => value.trim()).filter(Boolean) });
      }}>Save product</button>
    </div>}

    {(state.tickets || []).map((ticket: any) => <SupportTicket key={ticket.id} ticket={ticket}
      user={user} act={act} />)}

    {section === "promotions" && <div className="progression-card">
      <h3>Promotions</h3>
      <input data-role="promotion-code" value={promotion.code} placeholder="Code"
        onChange={event => setPromotion(value => ({ ...value, code: event.target.value }))} />
      <input data-role="promotion-discount" value={promotion.discount} placeholder="Discount"
        onChange={event => setPromotion(value => ({ ...value, discount: event.target.value }))} />
      <input data-role="promotion-start" type="date" value={promotion.start}
        onChange={event => setPromotion(value => ({ ...value, start: event.target.value }))} />
      <input data-role="promotion-end" type="date" value={promotion.end}
        onChange={event => setPromotion(value => ({ ...value, end: event.target.value }))} />
      <input data-role="promotion-limit" value={promotion.limit} placeholder="Limit"
        onChange={event => setPromotion(value => ({ ...value, limit: event.target.value }))} />
      <button data-role="promotion-submit" data-promotion-code={promotion.code} onClick={() => act("api:create_promotion", {
        code: promotion.code, discountPercent: Number(promotion.discount),
        startMicros: Date.parse(promotion.start) * 1000, endMicros: Date.parse(promotion.end) * 1000,
        usageLimit: Number(promotion.limit),
      })}>Save promotion</button>
      {(state.promotions || []).map((entry: any) => <div data-role="promotion-item" className="promotion-item" key={entry.id}>
        <strong>{entry.code}</strong>
        <span data-role="promotion-discount">{entry.discount}</span>
        <span data-role="promotion-start">{entry.start}</span>
        <span data-role="promotion-end">{entry.end}</span>
        <span data-role="promotion-limit">{entry.limit}</span>
      </div>)}
    </div>}

    <div className="progression-card">
      <h3>Scheduled restocks</h3>
      <input data-role="schedule-restock-item" value={restock.item} placeholder="Item"
        onChange={event => setRestock(value => ({ ...value, item: event.target.value }))} />
      <input data-role="schedule-restock-warehouse" value={restock.warehouse} placeholder="Warehouse"
        onChange={event => setRestock(value => ({ ...value, warehouse: event.target.value }))} />
      <input data-role="schedule-restock-qty" value={restock.quantity} placeholder="Quantity"
        onChange={event => setRestock(value => ({ ...value, quantity: event.target.value }))} />
      <input data-role="schedule-restock-delay" value={restock.delaySeconds} placeholder="Delay"
        onChange={event => setRestock(value => ({ ...value, delaySeconds: event.target.value }))} />
      <button data-role="schedule-restock-submit" data-action-input={JSON.stringify({ ...restock, quantity: Number(restock.quantity), delaySeconds: Number(restock.delaySeconds) })}
        onClick={() => act("api:schedule_restock", {
          item: restock.item, warehouse: restock.warehouse,
          quantity: Number(restock.quantity), delaySeconds: Number(restock.delaySeconds),
        })}>Schedule</button>
      {(state.scheduledRestocks || []).map((entry: any) => <div data-role="pending-restock-item" data-quantity={entry.quantity} data-entity-id={entry.id} key={entry.id}>
        {nameFor(items, entry.itemId)} <span data-role="pending-restock-remaining">{Math.max(0, Math.ceil((new Date(entry.dueAt).getTime() - now) / 1000))}</span>
        <button data-role="pending-restock-cancel" onClick={() => act("api:cancel_scheduled_restock", { restockId: entry.id })}>Cancel</button>
      </div>)}
      {(state.ledger || []).map((entry: any) => <div data-role="stock-ledger-entry" key={entry.id}>{nameFor(items, entry.itemId)} +{entry.quantity}</div>)}
    </div>

    {(orders || []).filter((order: any) => order.status === "delivered").map((order: any) =>
      <div data-role="completed-order-item" key={order.id}>{order.items.map((line: any) => line.name).join(", ")}
        <span data-role="completed-order-status">{order.status}</span></div>)}
  </div>;
}
