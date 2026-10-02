-- Profiles, support, staff tools, promotions, stock alerts and scheduled restocks.

create function public.save_profile(name text, address text) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  if coalesce(trim(name), '') = '' or coalesce(trim(address), '') = '' then
    raise exception 'Name and address are required';
  end if;
  update order_account set profile_name = save_profile.name, profile_address = save_profile.address
    where id = me.id;
end $$;

create function public.save_preferences("order" boolean, stock boolean) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  update order_account set order_notifications = coalesce("order", false),
    stock_notifications = coalesce(save_preferences.stock, false) where id = me.id;
end $$;

-- Signed-out visitors may open a ticket too; it then belongs to no account.
create function public.submit_support(email text, subject text, message text) returns json
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.current_account();
  ticket support_ticket;
begin
  if coalesce(trim(email), '') = '' or coalesce(trim(subject), '') = '' or coalesce(trim(message), '') = '' then
    raise exception 'Complete the support form';
  end if;
  insert into support_ticket (account_id, email, subject, message)
    values (me.id, submit_support.email, submit_support.subject, submit_support.message) returning * into ticket;
  update support_ticket set reference = 'SUP-' || lpad(ticket.id::text, 6, '0') where id = ticket.id
    returning * into ticket;
  return json_build_object('ticket', json_build_object('id', ticket.id, 'reference', ticket.reference));
end $$;

create function public.update_support("ticketId" bigint, assignee text, priority text, status text) returns void
language plpgsql security definer set search_path = public as $$
begin
  perform private.require_operator(false);
  update support_ticket set assignee = coalesce(update_support.assignee, ''),
    priority = coalesce(update_support.priority, ''), status = coalesce(update_support.status, '')
    where id = "ticketId";
  if not found then raise exception 'Ticket not found'; end if;
end $$;

create function public.reply_support("ticketId" bigint, body text) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  if not exists (select 1 from support_ticket where id = "ticketId"
      and (account_id = me.id or private.staff_roles(me.roles))) then
    raise exception 'Ticket not found';
  end if;
  if coalesce(trim(body), '') = '' then raise exception 'Reply is empty'; end if;
  insert into support_reply (ticket_id, account_id, body) values ("ticketId", me.id, reply_support.body);
end $$;

create function public.assign_staff_role("accountId" bigint, role text) returns void
language plpgsql security definer set search_path = public as $$
begin
  perform private.require_operator(true);
  if role is null or role not in ('staff', 'inventory', 'admin') then raise exception 'Invalid role'; end if;
  update order_account set roles = array[role] where id = "accountId";
  if not found then raise exception 'Account not found'; end if;
end $$;

create function public.create_promotion(code text, "discountPercent" numeric, "startMicros" bigint,
  "endMicros" bigint, "usageLimit" integer) returns void
language plpgsql security definer set search_path = public as $$
begin
  perform private.require_operator(false);
  if coalesce(trim(code), '') = '' or "discountPercent" is null or "discountPercent" <= 0 or "discountPercent" > 100
      or "startMicros" is null or "endMicros" is null or "endMicros" <= "startMicros"
      or "usageLimit" is null or "usageLimit" <= 0 then
    raise exception 'Invalid promotion';
  end if;
  insert into promotion (code, discount_percent, start_micros, end_micros, usage_limit)
    values (create_promotion.code, "discountPercent", "startMicros", "endMicros", "usageLimit");
end $$;

create function public.save_catalog_item(name text, category text, price numeric, variants text[]) returns void
language plpgsql security definer set search_path = public as $$
declare
  product bigint;
begin
  perform private.require_operator(true);
  if coalesce(trim(name), '') = '' or coalesce(trim(category), '') = ''
      or save_catalog_item.price is null or save_catalog_item.price < 0 then
    raise exception 'Invalid product';
  end if;
  insert into item (name, category, price, description, variants)
    values (save_catalog_item.name, save_catalog_item.category, round(save_catalog_item.price, 2),
      save_catalog_item.name, coalesce(save_catalog_item.variants, '{}'))
    returning id into product;
  insert into stock (item_id, warehouse_id, quantity) select product, id, 0 from warehouse;
end $$;

create function public.request_stock_alert("itemId" bigint) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  if not exists (select 1 from item where id = "itemId") then raise exception 'Item not found'; end if;
  insert into stock_alert (account_id, item_id) values (me.id, "itemId") on conflict do nothing;
end $$;

create function public.schedule_restock(item text, warehouse text, quantity integer, "delaySeconds" integer) returns void
language plpgsql security definer set search_path = public as $$
declare
  product bigint;
  location bigint;
begin
  perform private.require_operator(true);
  perform private.check_quantity(schedule_restock.quantity);
  perform private.check_quantity("delaySeconds", true);
  select id into product from public.item where name = schedule_restock.item;
  select id into location from public.warehouse where name = schedule_restock.warehouse;
  if product is null or location is null then raise exception 'Unknown item or warehouse'; end if;
  insert into scheduled_restock (item_id, warehouse_id, quantity, due_at)
    values (product, location, schedule_restock.quantity, now() + make_interval(secs => "delaySeconds"));
end $$;

create function public.cancel_scheduled_restock("restockId" bigint) returns void
language plpgsql security definer set search_path = public as $$
begin
  perform private.require_operator(true);
  update scheduled_restock set status = 'cancelled' where id = "restockId" and status = 'pending';
  if not found then raise exception 'Restock is not pending'; end if;
end $$;

-- Runs every second from pg_cron. The status change commits with the stock change, so a
-- restock applies once even across restarts, and never before its due time.
create function private.apply_due_restocks() returns void
language plpgsql security definer set search_path = public as $$
declare
  due scheduled_restock;
begin
  for due in select * from scheduled_restock where status = 'pending' and due_at <= now()
      order by due_at for update skip locked loop
    update stock set quantity = quantity + due.quantity
      where item_id = due.item_id and warehouse_id = due.warehouse_id;
    update scheduled_restock set status = 'completed' where id = due.id;
    insert into stock_ledger (item_id, quantity) values (due.item_id, due.quantity);
    perform private.notify_restock(due.item_id);
  end loop;
end $$;

create extension if not exists pg_cron;
select cron.schedule('apply-due-restocks', '1 seconds', 'select private.apply_due_restocks()');

create function public.progression_state() returns json
language plpgsql stable security definer set search_path = public as $$
declare
  me order_account := private.current_account();
  staff boolean := private.staff_roles(me.roles);
begin
  return json_build_object(
    'user', private.public_account(me),
    'profile', case when me.id is not null then
      json_build_object('name', me.profile_name, 'address', me.profile_address) end,
    'preference', json_build_object('order', coalesce(me.order_notifications, false),
      'stock', coalesce(me.stock_notifications, false)),
    'tickets', (select coalesce(json_agg(json_build_object('id', ticket.id, 'reference', ticket.reference,
        'email', ticket.email, 'subject', ticket.subject, 'message', ticket.message, 'status', ticket.status,
        'priority', ticket.priority, 'assignee', ticket.assignee,
        'replies', (select coalesce(json_agg(json_build_object('id', reply.id, 'username', author.username,
            'body', reply.body, 'createdAt', reply.created_at) order by reply.id), '[]')
          from support_reply reply join order_account author on author.id = reply.account_id
          where reply.ticket_id = ticket.id)) order by ticket.id), '[]')
      from support_ticket ticket where staff or ticket.account_id = me.id),
    'notifications', (select coalesce(json_agg(json_build_object('id', id, 'type', type, 'message', message)
      order by id), '[]') from notification where account_id = me.id),
    'promotions', (select coalesce(json_agg(json_build_object('id', id, 'code', code, 'discount', discount_percent,
        'start', to_char(to_timestamp(start_micros / 1e6) at time zone 'UTC', 'YYYY-MM-DD'),
        'end', to_char(to_timestamp(end_micros / 1e6) at time zone 'UTC', 'YYYY-MM-DD'),
        'limit', usage_limit) order by id), '[]') from promotion where staff),
    'scheduledRestocks', (select coalesce(json_agg(json_build_object('id', id, 'itemId', item_id,
        'warehouseId', warehouse_id, 'quantity', quantity, 'dueAt', due_at, 'status', status) order by id), '[]')
      from scheduled_restock where staff and status = 'pending'),
    'ledger', (select coalesce(json_agg(json_build_object('id', id, 'itemId', item_id, 'quantity', quantity)
      order by id), '[]') from stock_ledger where staff),
    'staffUsers', (select coalesce(json_agg(private.public_account(account) order by account.id), '[]')
      from order_account account where 'admin' = any(me.roles) and private.staff_roles(account.roles)));
end $$;
