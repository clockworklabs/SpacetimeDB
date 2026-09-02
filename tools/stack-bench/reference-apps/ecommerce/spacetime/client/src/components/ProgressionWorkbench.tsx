import { useEffect, useMemo, useState, type ChangeEvent } from 'react';
import type { DbConnection } from '../module_bindings';
import { formatMoney } from '../types';

interface Item { id: bigint; name: string }
interface Warehouse { id: bigint; name: string }
interface Order { orderId: bigint; items: { name: string }[] }
interface ItemVariant { id: bigint; itemId: bigint; name: string }
interface Profile { name: string; address: string }
interface StaffRole { accountId: bigint; username: string; role: string }
interface SupportTicket {
  id: bigint; reference: string; subject: string; status: string; priority: string;
  assignee: string; orderId?: bigint; refundTotal: number;
}
interface SupportReply { ticketId: bigint; author: string; body: string }
interface Preferences { orderEnabled: boolean; stockEnabled: boolean }
interface Notification { id: bigint; message: string }
interface ExpiredCartItem { itemId: bigint }
interface Restock { id: bigint; itemId: bigint; dueMicros: bigint; status: string; reorderRuleId?: bigint }
interface StockLedgerEntry { itemId: bigint; quantity: number }
interface Activity { actor: string; action: string; subject: string; createdMicros: bigint }
interface PromotionReport { promotionId: bigint; code: string; redemptions: number; revenue: number }
interface Promotion { id: bigint; code: string; discountPercent: number; startMicros: bigint; endMicros: bigint; usageLimit: number }
interface ReorderRule { id: bigint; itemId: bigint; threshold: number; quantity: number }
interface CompletedOrder { orderId: bigint; itemNames: string[]; status: string }

interface Props {
  conn: DbConnection | null;
  items: readonly Item[];
  warehouses: readonly Warehouse[];
  orders: readonly Order[];
  itemVariants: readonly ItemVariant[];
  profile: Profile | null;
  staffRoles: readonly StaffRole[];
  supportTickets: readonly SupportTicket[];
  supportReplies: readonly SupportReply[];
  preferences: Preferences | null;
  notifications: readonly Notification[];
  expiredCart: readonly ExpiredCartItem[];
  restocks: readonly Restock[];
  stockLedger: readonly StockLedgerEntry[];
  activity: readonly Activity[];
  promotionReports: readonly PromotionReport[];
  promotions: readonly Promotion[];
  reorderRules: readonly ReorderRule[];
  completedOrders: readonly CompletedOrder[];
  isSignedIn: boolean;
  isStaff: boolean;
  isAdmin: boolean;
}

const value = (event: ChangeEvent<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>) =>
  event.target.value;

export default function ProgressionWorkbench(props: Props) {
  const {
    conn, items, warehouses, orders, itemVariants, profile, staffRoles, supportTickets,
    supportReplies, preferences, notifications, expiredCart, restocks, stockLedger,
    activity, promotionReports, promotions, reorderRules, completedOrders, isSignedIn, isStaff, isAdmin,
  } = props;
  const reducers = conn?.reducers;
  const [profileName, setProfileName] = useState(profile?.name ?? '');
  const [profileAddress, setProfileAddress] = useState(profile?.address ?? '');
  const [supportEmail, setSupportEmail] = useState('');
  const [supportSubject, setSupportSubject] = useState('');
  const [supportMessage, setSupportMessage] = useState('');
  const [supportSubmission, setSupportSubmission] = useState<{ subject: string; afterId: bigint } | null>(null);
  const [triageByTicket, setTriageByTicket] = useState<Record<string, { assignee: string; priority: string; status: string }>>({});
  const [replyByTicket, setReplyByTicket] = useState<Record<string, string>>({});
  const [orderByTicket, setOrderByTicket] = useState<Record<string, string>>({});
  const [catalog, setCatalog] = useState({ name: '', category: '', price: '', variants: '' });
  const [promotion, setPromotion] = useState({ code: '', discount: '', start: '', end: '', limit: '' });
  const [restock, setRestock] = useState({ item: '', warehouse: '', quantity: '', delay: '' });
  const [reorder, setReorder] = useState({ item: '', warehouse: '', threshold: '', quantity: '' });
  const [orderEnabled, setOrderEnabled] = useState(preferences?.orderEnabled ?? false);
  const [stockEnabled, setStockEnabled] = useState(preferences?.stockEnabled ?? false);
  const [, setClock] = useState(0);
  const itemName = (id: bigint) => items.find(item => item.id === id)?.name ?? `Item ${String(id)}`;
  const orderName = (orderId: bigint) => {
    const order = orders.find(row => row.orderId === orderId);
    return order?.items.map(item => item.name).join(', ') ?? `Order ${String(orderId)}`;
  };
  const variantsByItem = useMemo(() => {
    const map = new Map<bigint, ItemVariant[]>();
    for (const row of itemVariants) map.set(row.itemId, [...(map.get(row.itemId) ?? []), row]);
    return map;
  }, [itemVariants]);
  const supportReference = useMemo(() => {
    if (!supportSubmission) return '';
    return supportTickets
      .filter(ticket => ticket.subject === supportSubmission.subject && ticket.id > supportSubmission.afterId)
      .sort((left, right) => left.id < right.id ? 1 : -1)[0]?.reference ?? '';
  }, [supportSubmission, supportTickets]);

  useEffect(() => {
    setProfileName(profile?.name ?? '');
    setProfileAddress(profile?.address ?? '');
  }, [profile?.name, profile?.address]);

  useEffect(() => {
    setOrderEnabled(preferences?.orderEnabled ?? false);
    setStockEnabled(preferences?.stockEnabled ?? false);
  }, [preferences?.orderEnabled, preferences?.stockEnabled]);

  useEffect(() => {
    const timer = window.setInterval(() => setClock(clock => clock + 1), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const submitSupport = async () => {
    const afterId = supportTickets.reduce((maximum, ticket) => ticket.id > maximum ? ticket.id : maximum, 0n);
    setSupportSubmission({ subject: supportSubject.trim(), afterId });
    await reducers?.createSupportTicket({ email: supportEmail, subject: supportSubject, message: supportMessage });
  };

  return (
    <section className="progression-workbench">
      <nav className="feature-links" aria-label="Account and staff tools">
        {isSignedIn && <button className="btn btn-ghost btn-sm" data-role="profile-link">Profile</button>}
        <button className="btn btn-ghost btn-sm" data-role="support-link">Support</button>
        {(isStaff || isAdmin) && <button className="btn btn-ghost btn-sm" data-role="activity-link">Activity</button>}
        {(isStaff || isAdmin) && <button className="btn btn-ghost btn-sm" data-role="promotions-link">Promotions</button>}
        {isSignedIn && <button className="btn btn-ghost btn-sm" data-role="notification-settings">Notifications</button>}
        {isSignedIn && <button className="btn btn-ghost btn-sm" data-role="notifications-toggle">Alerts</button>}
        {(isStaff || isAdmin) && <button className="btn btn-ghost btn-sm" data-role="reorder-link">Reorder rules</button>}
      </nav>

      {isSignedIn && (
        <section className="feature-card">
          <h2>Customer profile</h2>
          <input data-role="profile-name" value={profileName} onChange={e => setProfileName(value(e))} placeholder="Name" />
          <input data-role="profile-address" value={profileAddress} onChange={e => setProfileAddress(value(e))} placeholder="Shipping address" />
          <button className="btn btn-primary btn-sm" data-role="profile-save" onClick={() => reducers?.saveProfile({ name: profileName, address: profileAddress })}>Save profile</button>
          <div data-role="profile-address-summary">{profile?.name} {profile?.address}</div>
        </section>
      )}

      <section className="feature-card">
        <h2>Support</h2>
        <input data-role="support-email" value={supportEmail} onChange={e => setSupportEmail(value(e))} placeholder="Email" />
        <input data-role="support-subject" value={supportSubject} onChange={e => setSupportSubject(value(e))} placeholder="Subject" />
        <textarea data-role="support-message" value={supportMessage} onChange={e => setSupportMessage(value(e))} placeholder="Message" />
        <button className="btn btn-primary btn-sm" data-role="support-submit" onClick={submitSupport}>Open ticket</button>
        <div data-role="support-reference">{supportReference}</div>
        {supportTickets.map(ticket => {
          const key = String(ticket.id);
          const actionInput = JSON.stringify({ caseId: Number(ticket.id) });
          const triage = triageByTicket[key] ?? {
            assignee: ticket.assignee ?? '',
            priority: ticket.priority,
            status: ticket.status,
          };
          return (
            <article className="feature-row" data-role="support-ticket" data-entity-id={key} key={key}>
              <strong>{ticket.subject}</strong>
              <span>{ticket.reference}</span>
              <span data-role="support-status">{ticket.status}</span>
              {(isStaff || isAdmin) && (
                <>
                  <input data-role="support-assignee" value={triage.assignee} onChange={event => setTriageByTicket(current => ({ ...current, [key]: { ...triage, assignee: value(event) } }))} />
                  <input data-role="support-priority" value={triage.priority} onChange={event => setTriageByTicket(current => ({ ...current, [key]: { ...triage, priority: value(event) } }))} />
                  <input data-role="support-status-input" value={triage.status} onChange={event => setTriageByTicket(current => ({ ...current, [key]: { ...triage, status: value(event) } }))} />
                  <button className="btn btn-ghost btn-sm" data-role="support-update" onClick={() => reducers?.triageSupport({
                    ticketId: ticket.id,
                    assigneeId: staffRoles.find(row => row.username === triage.assignee)?.accountId ?? 0n,
                    priority: triage.priority,
                    status: triage.status,
                  })}>Update</button>
                </>
              )}
              {isSignedIn && orders.map(order => (
                <button className="btn btn-ghost btn-sm" data-role="support-order-option" key={String(order.orderId)} onClick={() => setOrderByTicket(v => ({ ...v, [key]: String(order.orderId) }))}>
                  {orderName(order.orderId)}
                </button>
              ))}
              {isSignedIn && (
                <button className="btn btn-ghost btn-sm" data-role="support-link-order"
                  data-action-input={JSON.stringify({ caseId: Number(ticket.id), orderId: Number(orderByTicket[key] ?? 0) })}
                  onClick={() => reducers?.linkSupportOrder({ ticketId: ticket.id, orderId: BigInt(orderByTicket[key] ?? 0) })}>
                  Link order
                </button>
              )}
              {(isStaff || isAdmin) && ticket.orderId !== undefined && (
                <button className="btn btn-danger btn-sm" data-role="support-refund" data-action-input={actionInput}
                  onClick={() => reducers?.supportRefund({ ticketId: ticket.id })}>Refund order</button>
              )}
              <span data-role="support-order">{ticket.orderId === undefined ? '' : `Order ${String(ticket.orderId)}`}</span>
              <span data-role="support-refund-total">{formatMoney(ticket.refundTotal)}</span>
              {supportReplies.filter(reply => reply.ticketId === ticket.id).map((reply, index) => (
                <div data-role="support-reply-item" key={index}>{reply.author}: {reply.body}</div>
              ))}
              <textarea data-role="support-reply" value={replyByTicket[key] ?? ''} onChange={e => setReplyByTicket(v => ({ ...v, [key]: value(e) }))} />
              <button className="btn btn-ghost btn-sm" data-role="support-reply-submit" onClick={() => reducers?.replySupport({ ticketId: ticket.id, body: replyByTicket[key] ?? '' })}>Reply</button>
            </article>
          );
        })}
      </section>

      {isAdmin && (
        <section className="feature-card">
          <h2>Staff roles</h2>
          {staffRoles.map(row => (
            <div className="feature-row" data-role="staff-role-row" data-account-id={String(row.accountId)} key={String(row.accountId)}>
              <span>{row.username}</span>
              <span data-role="staff-role-value">{row.role}</span>
              <input data-role="staff-role-select" defaultValue={row.role} id={`role-${String(row.accountId)}`} />
              <button className="btn btn-ghost btn-sm" data-role="staff-role-save" onClick={() => reducers?.assignStaffRole({
                accountId: row.accountId,
                role: (document.getElementById(`role-${String(row.accountId)}`) as HTMLSelectElement)?.value ?? 'warehouse',
              })}>Save role</button>
            </div>
          ))}
        </section>
      )}

      {(isStaff || isAdmin) && (
        <section className="feature-card">
          <h2>Catalog management</h2>
          <input data-role="catalog-name" value={catalog.name} onChange={e => setCatalog({ ...catalog, name: value(e) })} placeholder="Name" />
          <input data-role="catalog-category" value={catalog.category} onChange={e => setCatalog({ ...catalog, category: value(e) })} placeholder="Category" />
          <input data-role="catalog-price" value={catalog.price} onChange={e => setCatalog({ ...catalog, price: value(e) })} placeholder="Price" />
          <input data-role="catalog-variants" value={catalog.variants} onChange={e => setCatalog({ ...catalog, variants: value(e) })} placeholder="Variants" />
          <button className="btn btn-primary btn-sm" data-role="catalog-save" onClick={() => reducers?.addCatalogProduct({ name: catalog.name, categoryName: catalog.category, price: Number(catalog.price), variants: catalog.variants })}>Add product</button>
          {items.flatMap(item => (variantsByItem.get(item.id) ?? []).map(variant => <span data-role="item-variant" key={String(variant.id)}>{item.name}: {variant.name}</span>))}
        </section>
      )}

      {(isStaff || isAdmin) && (
        <section className="feature-card">
          <h2>Promotions</h2>
          <input data-role="promotion-code" value={promotion.code} onChange={e => setPromotion({ ...promotion, code: value(e) })} placeholder="Code" />
          <input data-role="promotion-discount" value={promotion.discount} onChange={e => setPromotion({ ...promotion, discount: value(e) })} placeholder="Discount %" />
          <input data-role="promotion-start" value={promotion.start} onChange={e => setPromotion({ ...promotion, start: value(e) })} placeholder="Start date" />
          <input data-role="promotion-end" value={promotion.end} onChange={e => setPromotion({ ...promotion, end: value(e) })} placeholder="End date" />
          <input data-role="promotion-limit" value={promotion.limit} onChange={e => setPromotion({ ...promotion, limit: value(e) })} placeholder="Limit" />
          <button className="btn btn-primary btn-sm" data-role="promotion-submit" data-promotion-code={promotion.code} onClick={() => reducers?.createPromotion({
            code: promotion.code,
            discountPercent: Number(promotion.discount),
            startMicros: BigInt(new Date(promotion.start).getTime()) * 1000n,
            endMicros: BigInt(new Date(promotion.end).getTime()) * 1000n,
            usageLimit: Number(promotion.limit),
          })}>Create promotion</button>
          {promotions.map(row => <div data-role="promotion-item" key={String(row.id)}>{row.code}: <span data-role="promotion-discount">{row.discountPercent}</span>% <span data-role="promotion-start">{new Date(Number(row.startMicros / 1000n)).toISOString().slice(0, 10)}</span> to <span data-role="promotion-end">{new Date(Number(row.endMicros / 1000n)).toISOString().slice(0, 10)}</span>, limit <span data-role="promotion-limit">{row.usageLimit}</span></div>)}
          {promotionReports.map(row => <div data-role="promotion-report" key={String(row.promotionId)}>{row.code}: <span data-role="promotion-redemptions">{row.redemptions}</span> <span data-role="promotion-revenue">{formatMoney(row.revenue)}</span></div>)}
        </section>
      )}

      {isSignedIn && (
        <section className="feature-card">
          <h2>Notification preferences</h2>
          <button className="btn btn-ghost btn-sm" data-role="notification-order" data-state={orderEnabled ? 'on' : 'off'} onClick={() => setOrderEnabled((enabled: boolean) => !enabled)}>Order notifications: {orderEnabled ? 'on' : 'off'}</button>
          <button className="btn btn-ghost btn-sm" data-role="notification-stock" data-state={stockEnabled ? 'on' : 'off'} onClick={() => setStockEnabled((enabled: boolean) => !enabled)}>Stock notifications: {stockEnabled ? 'on' : 'off'}</button>
          <button className="btn btn-primary btn-sm" data-role="notification-save" onClick={() => reducers?.saveNotificationPreferences({ orderEnabled, stockEnabled })}>Save preferences</button>
          {notifications.map(row => <div data-role="notification-item" key={String(row.id)}>{row.message}</div>)}
        </section>
      )}

      {isAdmin && (
        <section className="feature-card">
          <h2>Scheduled restocks</h2>
          <input data-role="schedule-restock-item" value={restock.item} onChange={e => setRestock({ ...restock, item: value(e) })} placeholder="Item" />
          <input data-role="schedule-restock-warehouse" value={restock.warehouse} onChange={e => setRestock({ ...restock, warehouse: value(e) })} placeholder="Warehouse" />
          <input data-role="schedule-restock-qty" value={restock.quantity} onChange={e => setRestock({ ...restock, quantity: value(e) })} placeholder="Quantity" />
          <input data-role="schedule-restock-delay" value={restock.delay} onChange={e => setRestock({ ...restock, delay: value(e) })} placeholder="Delay seconds" />
          <button className="btn btn-primary btn-sm" data-role="schedule-restock-submit"
            data-action-input={JSON.stringify({ item: restock.item, warehouse: restock.warehouse, quantity: Number(restock.quantity), delaySeconds: Number(restock.delay) })}
            onClick={() => reducers?.scheduleRestock({ item: restock.item, warehouse: restock.warehouse, quantity: Number(restock.quantity), delaySeconds: Number(restock.delay) })}>Schedule</button>
          {restocks.filter(row => row.status === 'pending').map(row => <div data-role="pending-restock-item" data-entity-id={String(row.id)} key={String(row.id)}>{itemName(row.itemId)} <span data-role="pending-restock-remaining">{Math.max(0, Number((row.dueMicros - BigInt(Date.now()) * 1000n) / 1_000_000n))}</span><button data-role="pending-restock-cancel" onClick={() => reducers?.cancelScheduledRestock({ restockId: row.id })}>Cancel</button></div>)}
          {stockLedger.map((row, index) => <div data-role="stock-ledger-entry" key={index}>{itemName(row.itemId)} +{row.quantity}</div>)}
        </section>
      )}

      {(isStaff || isAdmin) && (
        <section className="feature-card">
          <h2>Automatic reorder</h2>
          <input data-role="reorder-item" value={reorder.item} onChange={e => setReorder({ ...reorder, item: value(e) })} placeholder="Item" />
          <input data-role="reorder-threshold" value={reorder.threshold} onChange={e => setReorder({ ...reorder, threshold: value(e) })} placeholder="Threshold" />
          <input data-role="reorder-quantity" value={reorder.quantity} onChange={e => setReorder({ ...reorder, quantity: value(e) })} placeholder="Quantity" />
          <button className="btn btn-primary btn-sm" data-role="reorder-submit" onClick={() => reducers?.saveReorderRule({ itemId: items.find(row => row.name === reorder.item)?.id ?? 0n, warehouseId: warehouses.find(row => row.name === reorder.warehouse)?.id ?? warehouses[0]?.id ?? 0n, threshold: Number(reorder.threshold), quantity: Number(reorder.quantity) })}>Save rule</button>
          {reorderRules.map(row => {
            const pending = restocks.some(restock => restock.reorderRuleId === row.id && restock.status === 'pending');
            return <div data-role="reorder-rule-item" key={String(row.id)}>{itemName(row.itemId)} at {row.threshold}: {row.quantity} ({pending ? 'pending' : 'ready'})</div>;
          })}
        </section>
      )}

      {expiredCart.length > 0 && (
        <section className="feature-card" data-role="expired-cart">
          <h2>Expired cart</h2>
          {expiredCart.map(row => <div data-role="cart-item" key={String(row.itemId)}>{itemName(row.itemId)} <span data-role="cart-item-expired">expired</span></div>)}
          <div data-role="cart-expired-notice">Your reserved cart expired.</div>
          <div data-role="cart-restore-warning">Unavailable: {expiredCart.map(row => itemName(row.itemId)).join(', ')}</div>
          <button className="btn btn-primary btn-sm" data-role="restore-cart" onClick={() => reducers?.restoreExpiredCart({})}>Restore cart</button>
        </section>
      )}

      {(isStaff || isAdmin) && activity.map((row, index) => <div className="feature-row" data-role="activity-entry" key={index}><span data-role="activity-actor">{row.actor}</span><span data-role="activity-action">{row.action}</span><span data-role="activity-subject">{row.subject}</span><span data-role="activity-time">{String(row.createdMicros)}</span></div>)}
      {(isStaff || isAdmin) && completedOrders.map(row => <div data-role="completed-order-item" key={String(row.orderId)}>{row.itemNames.join(', ')} <span data-role="completed-order-status">{row.status}</span></div>)}
    </section>
  );
}
