-- Starting catalog, inserted only into an empty store so restarts keep changed data.
do $$
begin
  if exists (select 1 from public.item) then return; end if;
  insert into public.warehouse (name) values ('East'), ('West');
  with catalogue (name, category, price, east, west, description) as (values
    ('Air Purifier', 'Home', 189.00, 60, 40, 'HEPA filtration for cleaner indoor air.'),
    ('Bluetooth Speaker', 'Audio', 79.50, 50, 50, 'Portable speaker with rich, room-filling sound.'),
    ('Coffee Grinder', 'Home', 64.00, 70, 30, 'Burr grinder for consistent, fresh grounds.'),
    ('Desk Lamp', 'Home', 42.00, 55, 45, 'Adjustable LED lamp for any desk setup.'),
    ('Espresso Machine', 'Home', 449.00, 80, 20, 'Café-quality espresso at home.'),
    ('Gaming Mouse', 'Computing', 59.00, 50, 50, 'Precision optical mouse built for gaming.'),
    ('Headphones', 'Audio', 199.00, 60, 40, 'Over-ear headphones with active noise cancelling.'),
    ('Induction Cooktop', 'Home', 329.00, 50, 50, 'Fast, efficient induction cooking surface.'),
    ('Keyboard', 'Computing', 89.00, 70, 30, 'Mechanical keyboard with tactile switches.'),
    ('Laptop Stand', 'Computing', 29.00, 90, 10, 'Ergonomic aluminum stand for laptops.'),
    ('Mirrorless Camera', 'Photo', 1299.00, 2, 1, 'Compact mirrorless camera for enthusiasts.'),
    ('USB Cable', 'Home', 65.00, 0, 0, 'USB cable currently awaiting restock.'),
    ('Webcam', 'Computing', 69.00, 60, 40, '1080p webcam for calls and streaming.')
  ), created as (
    insert into public.item (name, category, price, description)
    select name, category, price, description from catalogue returning id, name
  )
  insert into public.stock (item_id, warehouse_id, quantity)
  select created.id, warehouse.id, case warehouse.name when 'East' then catalogue.east else catalogue.west end
  from created join catalogue using (name) cross join public.warehouse;
end $$;
