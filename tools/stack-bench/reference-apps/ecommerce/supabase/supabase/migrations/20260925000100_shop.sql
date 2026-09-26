-- Shop operations. Each public function is callable at /rest/v1/rpc/<name> and is also what the
-- visible controls call. Access errors use SQLSTATE 42501; refusals use P0001.

create function private.require_account() returns public.order_account
language plpgsql stable security definer set search_path = public as $$
declare
  me order_account := private.current_account();
begin
  if me.id is null then raise exception 'Sign in required' using errcode = '42501'; end if;
  return me;
end $$;

create function private.require_operator(admin_only boolean) returns public.order_account
language plpgsql stable security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  if not (case when admin_only then 'admin' = any(me.roles) else private.staff_roles(me.roles) end) then
    raise exception 'Access denied' using errcode = '42501';
  end if;
  return me;
end $$;

create function private.check_quantity(amount integer, allow_zero boolean default false) returns void
language plpgsql immutable as $$
begin
  if amount is null or amount < (case when allow_zero then 0 else 1 end) then
    raise exception 'Invalid quantity';
  end if;
end $$;

-- Items with total stock and units held by standing (not cancelled, not returned) order lines.
create view private.catalog as
  select item.id, item.name, item.price, item.description, item.category, item.variants,
    coalesce((select sum(stock.quantity) from public.stock where stock.item_id = item.id), 0)::integer as stock,
    coalesce((select sum(line.quantity) from public.order_line line
      join public.order_header header on header.id = line.order_id
      where line.item_id = item.id and not line.returned and header.status <> 'cancelled'), 0)::integer as purchase_count
  from public.item;

-- Each undelivered alert for the item is delivered exactly once, even under concurrent restocks.
create function private.notify_restock(restocked bigint) returns void
language sql security definer set search_path = public as $$
  with delivered as (
    update stock_alert set delivered = true
    where item_id = restocked and not delivered
    returning account_id, item_id
  )
  insert into notification (account_id, type, message)
  select delivered.account_id, 'stock', item.name || ' is back in stock'
  from delivered join item on item.id = delivered.item_id;
$$;

-- Takes stock from warehouses in a fixed order and records where each unit came from.
create function private.allocate(product bigint, wanted integer, line bigint) returns void
language plpgsql security definer set search_path = public as $$
declare
  holding record;
  take integer;
begin
  perform 1 from stock where item_id = product order by warehouse_id for update;
  if (select coalesce(sum(quantity), 0) from stock where item_id = product) < wanted then
    raise exception 'Not enough stock';
  end if;
  for holding in select warehouse_id, quantity from stock
      where item_id = product and quantity > 0 order by warehouse_id loop
    exit when wanted = 0;
    take := least(holding.quantity, wanted);
    update stock set quantity = quantity - take
      where item_id = product and warehouse_id = holding.warehouse_id;
    insert into order_allocation (order_line_id, warehouse_id, quantity)
      values (line, holding.warehouse_id, take);
    wanted := wanted - take;
  end loop;
end $$;

-- Books an order at current server prices. lines: [{ "item_id": ..., "quantity": ... }].
create function private.place_order(buyer bigint, lines jsonb) returns bigint
language plpgsql security definer set search_path = public as $$
declare
  new_order bigint;
  new_line bigint;
  entry jsonb;
  product item;
  amount integer;
  booked numeric := 0;
begin
  if jsonb_array_length(lines) = 0 then raise exception 'Cart is empty'; end if;
  insert into order_header (account_id, total) values (buyer, 0) returning id into new_order;
  for entry in select value from jsonb_array_elements(lines) loop
    amount := (entry ->> 'quantity')::integer;
    perform private.check_quantity(amount);
    select * into product from item where id = (entry ->> 'item_id')::bigint;
    if not found then raise exception 'Unknown item'; end if;
    insert into order_line (order_id, item_id, quantity, unit_price)
      values (new_order, product.id, amount, product.price) returning id into new_line;
    perform private.allocate(product.id, amount, new_line);
    booked := booked + product.price * amount;
  end loop;
  update order_header set total = booked where id = new_order;
  return new_order;
end $$;

create function public.buy_now("itemId" bigint) returns json
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  return json_build_object('orderId',
    private.place_order(me.id, jsonb_build_array(jsonb_build_object('item_id', "itemId", 'quantity', 1))));
end $$;

create function public.checkout() returns json
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
  lines jsonb;
begin
  -- Deleting the lines claims them: a repeated or racing checkout finds the cart empty.
  with taken as (
    delete from order_cart where account_id = me.id returning id, item_id, quantity
  )
  select coalesce(jsonb_agg(jsonb_build_object('item_id', item_id, 'quantity', quantity) order by id), '[]')
    into lines from taken;
  return json_build_object('orderId', private.place_order(me.id, lines));
end $$;

create function private.change_cart(product bigint, amount integer, adding boolean) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
  next_quantity integer;
begin
  perform private.check_quantity(amount, not adding);
  if not exists (select 1 from item where id = product) then raise exception 'Unknown item'; end if;
  if amount = 0 then
    delete from order_cart where account_id = me.id and item_id = product;
    return;
  end if;
  insert into order_cart (account_id, item_id, quantity) values (me.id, product, amount)
    on conflict (account_id, item_id) do update
    set quantity = case when adding then order_cart.quantity + excluded.quantity else excluded.quantity end
    returning quantity into next_quantity;
  if next_quantity > (select coalesce(sum(quantity), 0) from stock where item_id = product) then
    raise exception 'Not enough stock';
  end if;
end $$;

create function public.add_to_cart("itemId" bigint, quantity integer default 1) returns void
language sql security definer set search_path = public as $$
  select private.change_cart("itemId", quantity, true);
$$;

create function public.update_cart_quantity("itemId" bigint, quantity integer) returns void
language sql security definer set search_path = public as $$
  select private.change_cart("itemId", quantity, false);
$$;

create function public.admin_restock("itemId" bigint, "warehouseId" bigint, quantity integer) returns void
language plpgsql security definer set search_path = public as $$
begin
  perform private.require_operator(true);
  perform private.check_quantity(admin_restock.quantity);
  update stock set quantity = stock.quantity + admin_restock.quantity
    where item_id = "itemId" and warehouse_id = "warehouseId";
  if not found then raise exception 'Unknown stock location'; end if;
  perform private.notify_restock("itemId");
end $$;

create function public.admin_transfer_stock("itemId" bigint, "fromWarehouseId" bigint,
  "toWarehouseId" bigint, quantity integer) returns void
language plpgsql security definer set search_path = public as $$
declare
  available integer;
begin
  perform private.require_operator(true);
  perform private.check_quantity(admin_transfer_stock.quantity);
  if "fromWarehouseId" = "toWarehouseId" then raise exception 'Choose different warehouses'; end if;
  -- Lock both holdings in one order so opposite transfers cannot deadlock.
  perform 1 from stock where item_id = "itemId" and warehouse_id in ("fromWarehouseId", "toWarehouseId")
    order by warehouse_id for update;
  select stock.quantity into available from stock
    where item_id = "itemId" and warehouse_id = "fromWarehouseId";
  if available is null or not exists (select 1 from stock
      where item_id = "itemId" and warehouse_id = "toWarehouseId") then
    raise exception 'Unknown stock location';
  end if;
  if available < admin_transfer_stock.quantity then raise exception 'Not enough stock'; end if;
  update stock set quantity = stock.quantity - admin_transfer_stock.quantity
    where item_id = "itemId" and warehouse_id = "fromWarehouseId";
  update stock set quantity = stock.quantity + admin_transfer_stock.quantity
    where item_id = "itemId" and warehouse_id = "toWarehouseId";
end $$;

create function public.admin_change_price("itemId" bigint, price numeric) returns void
language plpgsql security definer set search_path = public as $$
begin
  perform private.require_operator(true);
  if admin_change_price.price is null or admin_change_price.price < 0 then raise exception 'Invalid price'; end if;
  update item set price = round(admin_change_price.price, 2) where id = "itemId";
  if not found then raise exception 'Unknown item'; end if;
end $$;

create function public.ship_order("orderId" bigint) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_operator(false);
begin
  if 'inventory' = any(me.roles) and not 'admin' = any(me.roles) then
    raise exception 'Fulfilment access required' using errcode = '42501';
  end if;
  update order_header set status = 'shipped' where id = "orderId" and status = 'pending';
  if not found then raise exception 'Order is not pending'; end if;
end $$;

create function private.owned_order(wanted bigint) returns public.order_header
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
  found_order order_header;
begin
  select * into found_order from order_header where id = wanted and account_id = me.id for update;
  if not found then raise exception 'Order not found'; end if;
  return found_order;
end $$;

-- Returns each warehouse's allocated units for the line.
create function private.restore_line(line public.order_line) returns void
language plpgsql security definer set search_path = public as $$
begin
  update stock set quantity = stock.quantity + allocation.quantity
    from order_allocation allocation
    where allocation.order_line_id = line.id
      and stock.item_id = line.item_id and stock.warehouse_id = allocation.warehouse_id;
  update order_line set returned = true where id = line.id;
  perform private.notify_restock(line.item_id);
end $$;

create function public.cancel_order("orderId" bigint) returns void
language plpgsql security definer set search_path = public as $$
declare
  target order_header := private.owned_order("orderId");
  line order_line;
begin
  if target.status <> 'pending' then raise exception 'Only pending orders can be cancelled'; end if;
  for line in select * from order_line where order_id = target.id loop
    perform private.restore_line(line);
  end loop;
  update order_header set status = 'cancelled', refunded = total where id = target.id;
end $$;

create function public.return_order_item("orderId" bigint, "itemId" bigint) returns void
language plpgsql security definer set search_path = public as $$
declare
  target order_header := private.owned_order("orderId");
  line order_line;
begin
  if target.status not in ('shipped', 'delivered') then raise exception 'Order has not shipped'; end if;
  select * into line from order_line where order_id = target.id and item_id = "itemId" and not returned;
  if not found then raise exception 'Item cannot be returned'; end if;
  perform private.restore_line(line);
  update order_header set refunded = refunded + line.quantity * line.unit_price where id = target.id;
end $$;

create function public.submit_review("itemId" bigint, rating integer, comment text) returns void
language plpgsql security definer set search_path = public as $$
declare
  me order_account := private.require_account();
begin
  if submit_review.rating is null or submit_review.rating not between 1 and 5
      or submit_review.comment is null or length(submit_review.comment) > 4000 then
    raise exception 'Invalid review';
  end if;
  if not exists (select 1 from order_line line join order_header header on header.id = line.order_id
      where header.account_id = me.id and header.status <> 'cancelled'
        and line.item_id = "itemId" and not line.returned) then
    raise exception 'Purchase this item before reviewing' using errcode = '42501';
  end if;
  if exists (select 1 from review where item_id = "itemId" and account_id = me.id) then
    raise exception 'Already reviewed';
  end if;
  insert into review (item_id, account_id, rating, comment)
    values ("itemId", me.id, submit_review.rating, submit_review.comment);
end $$;

create function private.public_account(account public.order_account) returns json
language sql immutable as $$
  select case when account.id is null then null else json_build_object('id', account.id,
    'username', account.username, 'roles', account.roles, 'isAdmin', 'admin' = any(account.roles),
    'isStaff', private.staff_roles(account.roles)) end
$$;

create function private.order_json(entry public.order_header) returns json
language sql stable security definer set search_path = public as $$
  select json_build_object('id', entry.id, 'total', entry.total, 'status', entry.status,
    'refundTotal', entry.refunded, 'createdAt', entry.created_at,
    'items', coalesce((select json_agg(json_build_object('itemId', line.item_id, 'name', item.name,
      'price', line.unit_price, 'quantity', line.quantity, 'returned', line.returned,
      'warehouseNames', (select coalesce(json_agg(warehouse.name order by allocation.id), '[]')
        from order_allocation allocation join warehouse on warehouse.id = allocation.warehouse_id
        where allocation.order_line_id = line.id)) order by line.id)
      from order_line line join item on item.id = line.item_id where line.order_id = entry.id), '[]'))
$$;

-- Everything the storefront shows the caller, computed for that caller.
create function public.shop_state() returns json
language plpgsql stable security definer set search_path = public as $$
declare
  me order_account := private.current_account();
  admin boolean := coalesce('admin' = any(me.roles), false);
  fulfils boolean := private.staff_roles(me.roles) and (admin or not 'inventory' = any(me.roles));
  items json;
  details json;
  cart json;
  recommended json;
  pending_depth integer := (select count(*) from order_header where status = 'pending');
begin
  select coalesce(json_agg(json_build_object('id', id, 'name', name, 'price', price,
      'description', description, 'category', category, 'variants', variants, 'stock', stock,
      'purchaseCount', purchase_count) order by purchase_count desc, name), '[]')
    into items from private.catalog;
  select coalesce(json_agg(json_build_object('id', catalog.id, 'name', catalog.name, 'price', catalog.price,
      'description', catalog.description, 'stock', catalog.stock,
      'reviews', coalesce(reviews.list, '[]'), 'average', coalesce(reviews.average, 0))), '[]')
    into details
    from private.catalog left join lateral (
      select json_agg(json_build_object('id', review.id, 'itemId', review.item_id, 'userId', review.account_id,
          'username', author.username, 'rating', review.rating, 'comment', review.comment,
          'createdAt', review.created_at) order by review.id) as list,
        avg(review.rating) as average
      from review join order_account author on author.id = review.account_id
      where review.item_id = catalog.id) reviews on true;
  select json_build_object('items', coalesce(json_agg(json_build_object('itemId', line.item_id,
      'quantity', line.quantity, 'name', catalog.name, 'price', catalog.price, 'stock', catalog.stock)
      order by line.id), '[]'), 'total', coalesce(sum(catalog.price * line.quantity), 0))
    into cart
    from order_cart line join private.catalog on catalog.id = line.item_id
    where line.account_id = me.id;
  if me.id is null then
    select coalesce(json_agg(entry), '[]') into recommended from (select json_build_object('id', id, 'name', name,
        'price', price, 'category', category, 'variants', variants, 'stock', stock, 'purchaseCount', purchase_count) entry
      from private.catalog order by purchase_count desc, name limit 10) best;
  else
    select coalesce(json_agg(json_build_object('id', id, 'name', name, 'price', price, 'category', category,
        'variants', variants, 'stock', stock, 'purchaseCount', purchase_count) order by purchase_count desc, name), '[]')
      into recommended from private.catalog
      where category in (select item.category from order_line line
          join order_header header on header.id = line.order_id join item on item.id = line.item_id
          where header.account_id = me.id and header.status <> 'cancelled' and not line.returned)
        and id not in (select item_id from order_cart where account_id = me.id);
  end if;
  return json_build_object(
    'user', private.public_account(me),
    'items', items,
    'details', details,
    'recommended', recommended,
    'cart', cart,
    'orders', (select coalesce(json_agg(private.order_json(entry) order by entry.id), '[]')
      from order_header entry where entry.account_id = me.id),
    'fulfilment', case when fulfils then json_build_object(
      'orders', (select coalesce(json_agg(private.order_json(entry) order by entry.id), '[]')
        from order_header entry where entry.status = 'pending'),
      'depth', pending_depth) end,
    'admin', case when admin then json_build_object(
      'items', items,
      'warehouses', (select coalesce(json_agg(json_build_object('id', warehouse.id, 'name', warehouse.name,
          'total', (select coalesce(sum(quantity), 0) from stock where warehouse_id = warehouse.id))
          order by warehouse.id), '[]') from warehouse),
      'locations', (select coalesce(json_agg(json_build_object('id', stock.item_id || '-' || stock.warehouse_id,
          'itemId', stock.item_id, 'warehouseId', stock.warehouse_id, 'itemName', item.name,
          'warehouseName', warehouse.name, 'quantity', stock.quantity) order by stock.item_id, stock.warehouse_id), '[]')
        from stock join item on item.id = stock.item_id join warehouse on warehouse.id = stock.warehouse_id),
      'revenue', (select coalesce(sum(total - refunded), 0) from order_header),
      'categories', (select coalesce(json_agg(json_build_object('category', category.name,
          'units', coalesce(sold.units, 0), 'revenue', coalesce(sold.revenue, 0)) order by category.name), '[]')
        from (select distinct item.category as name from item) category
        left join (select item.category, sum(line.quantity) as units, sum(line.quantity * line.unit_price) as revenue
          from order_line line join order_header header on header.id = line.order_id join item on item.id = line.item_id
          where header.status <> 'cancelled' and not line.returned group by item.category) sold
          on sold.category = category.name),
      'lowStock', (select coalesce(json_agg(json_build_object('id', id, 'name', name, 'stock', stock)
          order by stock, name), '[]') from private.catalog where stock <= 10),
      'queueDepth', pending_depth) end);
end $$;
