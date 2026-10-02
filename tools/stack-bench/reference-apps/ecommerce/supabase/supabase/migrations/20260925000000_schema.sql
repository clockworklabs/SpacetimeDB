-- Storefront tables. The order-data and stock interface tables keep their fixed names and columns.
create table public.order_account (
  id bigint generated always as identity primary key,
  auth_user_id uuid not null unique references auth.users on delete cascade,
  username text not null unique,
  roles text[] not null default '{}',
  profile_name text not null default '',
  profile_address text not null default '',
  order_notifications boolean not null default false,
  stock_notifications boolean not null default false
);

create table public.item (
  id bigint generated always as identity primary key,
  name text not null unique,
  price numeric(10, 2) not null check (price >= 0),
  description text not null default '',
  category text not null,
  variants text[] not null default '{}'
);

create table public.warehouse (
  id bigint generated always as identity primary key,
  name text not null unique
);

create table public.stock (
  item_id bigint not null references public.item,
  warehouse_id bigint not null references public.warehouse,
  quantity integer not null check (quantity >= 0),
  primary key (item_id, warehouse_id)
);

create table public.order_cart (
  id bigint generated always as identity primary key,
  account_id bigint not null references public.order_account,
  item_id bigint not null references public.item,
  quantity integer not null check (quantity > 0),
  unique (account_id, item_id)
);

-- This app does not reserve warehouse stock before checkout.
create table public.order_reservation (
  id bigint generated always as identity primary key,
  account_id bigint not null references public.order_account,
  item_id bigint not null references public.item,
  warehouse_id bigint not null references public.warehouse,
  quantity integer not null check (quantity > 0)
);

create table public.order_header (
  id bigint generated always as identity primary key,
  account_id bigint not null references public.order_account,
  total numeric(10, 2) not null,
  refunded numeric(10, 2) not null default 0,
  status text not null default 'pending',
  created_at timestamptz not null default now()
);

create table public.order_line (
  id bigint generated always as identity primary key,
  order_id bigint not null references public.order_header,
  item_id bigint not null references public.item,
  quantity integer not null check (quantity > 0),
  unit_price numeric(10, 2) not null,
  returned boolean not null default false
);

create table public.order_allocation (
  id bigint generated always as identity primary key,
  order_line_id bigint not null references public.order_line,
  warehouse_id bigint not null references public.warehouse,
  quantity integer not null check (quantity > 0)
);

create table public.review (
  id bigint generated always as identity primary key,
  item_id bigint not null references public.item,
  account_id bigint not null references public.order_account,
  rating integer not null check (rating between 1 and 5),
  comment text not null,
  created_at timestamptz not null default now(),
  unique (item_id, account_id)
);

create table public.support_ticket (
  id bigint generated always as identity primary key,
  account_id bigint references public.order_account,
  email text not null,
  subject text not null,
  message text not null,
  reference text not null default '',
  status text not null default 'new',
  priority text not null default 'normal',
  assignee text not null default '',
  created_at timestamptz not null default now()
);

create table public.support_reply (
  id bigint generated always as identity primary key,
  ticket_id bigint not null references public.support_ticket,
  account_id bigint not null references public.order_account,
  body text not null,
  created_at timestamptz not null default now()
);

create table public.promotion (
  id bigint generated always as identity primary key,
  code text not null,
  discount_percent numeric not null,
  start_micros bigint not null,
  end_micros bigint not null,
  usage_limit integer not null
);

create table public.stock_alert (
  id bigint generated always as identity primary key,
  account_id bigint not null references public.order_account,
  item_id bigint not null references public.item,
  delivered boolean not null default false,
  unique (account_id, item_id)
);

create table public.notification (
  id bigint generated always as identity primary key,
  account_id bigint not null references public.order_account,
  type text not null,
  message text not null,
  created_at timestamptz not null default now()
);

create table public.scheduled_restock (
  id bigint generated always as identity primary key,
  item_id bigint not null references public.item,
  warehouse_id bigint not null references public.warehouse,
  quantity integer not null check (quantity > 0),
  due_at timestamptz not null,
  status text not null default 'pending'
);

create table public.stock_ledger (
  id bigint generated always as identity primary key,
  item_id bigint not null references public.item,
  quantity integer not null,
  created_at timestamptz not null default now()
);

-- Helpers the API does not expose. Row policies call them, so both client roles may use the schema.
create schema private;
grant usage on schema private to anon, authenticated;

-- The caller's account, only while the token's Auth session still exists (sign-out ends it).
create function private.current_account() returns public.order_account
language sql stable security definer set search_path = public as $$
  select account.* from order_account account
  where account.auth_user_id = auth.uid()
    and exists (select 1 from auth.sessions session
      where session.id = (auth.jwt() ->> 'session_id')::uuid and session.user_id = account.auth_user_id)
$$;

create function private.staff_roles(roles text[]) returns boolean
language sql immutable as $$ select coalesce(roles && array['admin', 'staff', 'inventory'], false) $$;

create function private.current_account_id() returns bigint
language sql stable as $$ select (private.current_account()).id $$;

create function private.current_is_staff() returns boolean
language sql stable as $$ select private.staff_roles((private.current_account()).roles) $$;

create function private.current_is_admin() returns boolean
language sql stable as $$ select coalesce('admin' = any((private.current_account()).roles), false) $$;

-- Sign-up derives the email from the username (hex local part), so the stored name comes
-- from that email and never from client-supplied user metadata.
create function private.handle_new_user() returns trigger
language plpgsql security definer set search_path = public as $$
declare
  account_name text;
begin
  if split_part(new.email, '@', 2) <> 'accounts.invalid' then
    raise exception 'Invalid username';
  end if;
  account_name := convert_from(decode(split_part(new.email, '@', 1), 'hex'), 'UTF8');
  if account_name !~ '^[A-Za-z0-9-]{1,48}$' then
    raise exception 'Invalid username';
  end if;
  insert into order_account (auth_user_id, username) values (new.id, account_name);
  return new;
end $$;

create trigger on_auth_user_created after insert on auth.users
  for each row execute function private.handle_new_user();

-- Clients read through the state functions and write through the operation functions.
-- These read policies only decide which changes Realtime delivers to each client.
alter table public.order_account enable row level security;
alter table public.item enable row level security;
alter table public.warehouse enable row level security;
alter table public.stock enable row level security;
alter table public.order_cart enable row level security;
alter table public.order_reservation enable row level security;
alter table public.order_header enable row level security;
alter table public.order_line enable row level security;
alter table public.order_allocation enable row level security;
alter table public.review enable row level security;
alter table public.support_ticket enable row level security;
alter table public.support_reply enable row level security;
alter table public.promotion enable row level security;
alter table public.stock_alert enable row level security;
alter table public.notification enable row level security;
alter table public.scheduled_restock enable row level security;
alter table public.stock_ledger enable row level security;

create policy catalog_read on public.item for select to anon, authenticated using (true);
create policy warehouse_read on public.warehouse for select to anon, authenticated using (true);
create policy stock_read on public.stock for select to anon, authenticated using (true);
create policy review_read on public.review for select to anon, authenticated using (true);
create policy account_read on public.order_account for select to authenticated
  using (id = private.current_account_id() or private.current_is_admin());
create policy cart_read on public.order_cart for select to authenticated
  using (account_id = private.current_account_id());
create policy reservation_read on public.order_reservation for select to authenticated
  using (account_id = private.current_account_id());
create policy order_read on public.order_header for select to authenticated
  using (account_id = private.current_account_id() or private.current_is_staff());
create policy order_line_read on public.order_line for select to authenticated
  using (exists (select 1 from public.order_header o where o.id = order_id));
create policy allocation_read on public.order_allocation for select to authenticated
  using (exists (select 1 from public.order_line l where l.id = order_line_id));
create policy ticket_read on public.support_ticket for select to authenticated
  using (account_id = private.current_account_id() or private.current_is_staff());
create policy reply_read on public.support_reply for select to authenticated
  using (exists (select 1 from public.support_ticket t where t.id = ticket_id));
create policy promotion_read on public.promotion for select to authenticated
  using (private.current_is_staff());
create policy alert_read on public.stock_alert for select to authenticated
  using (account_id = private.current_account_id());
create policy notification_read on public.notification for select to authenticated
  using (account_id = private.current_account_id());
create policy restock_read on public.scheduled_restock for select to authenticated
  using (private.current_is_staff());
create policy ledger_read on public.stock_ledger for select to authenticated
  using (private.current_is_staff());

alter publication supabase_realtime add table public.order_account, public.item, public.stock,
  public.order_cart, public.order_header, public.order_line, public.review, public.support_ticket,
  public.support_reply, public.promotion, public.notification, public.scheduled_restock, public.stock_ledger;
