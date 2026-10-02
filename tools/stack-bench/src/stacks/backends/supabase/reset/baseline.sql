-- Run once as supabase_admin, after activation has started every service and a
-- first Realtime join has created Realtime's own publication. Records every
-- catalog object the platform owns; reset() removes anything created later and
-- empties platform data tables, so the next case starts from bootstrap state.
create schema stackbench_reset;
revoke all on schema stackbench_reset from public;

create table stackbench_reset.baseline (catalog text not null, oid oid not null, primary key (catalog, oid));

-- Platform tables whose rows belong to the previous case. Migration-history
-- tables and Realtime's tenant configuration are kept.
create table stackbench_reset.data_tables (name regclass primary key);
insert into stackbench_reset.data_tables
  select format('%I.%I', n.nspname, c.relname)::regclass
  from pg_class c join pg_namespace n on n.oid = c.relnamespace
  where c.relkind in ('r', 'p') and not c.relispartition
    and n.nspname in ('auth', 'storage', 'realtime', 'vault', 'supabase_functions', 'net')
    and c.relname not in ('schema_migrations', 'migrations');

create function stackbench_reset.reset() returns jsonb language plpgsql as $$
declare
  r record;
  dropped int := 0;
  pass int := 0;
  truncated text;
  new_since_bootstrap constant text := 'not exists (select 1 from stackbench_reset.baseline b where b.catalog = %L and b.oid = %s)';
  -- An extension's own objects go with the extension, in the extension loop.
  not_extension_member constant text := 'not exists (select 1 from pg_depend x where x.classid = %L::regclass and x.objid = %s and x.deptype = ''e'')';
begin
  -- Repeat, because one drop can cascade into or unblock another.
  loop
    pass := pass + 1;
    exit when pass > 5;
    for r in execute format('select nspname from pg_namespace n where nspname not like %L and ' || new_since_bootstrap
                            || ' and ' || not_extension_member, 'pg_%', 'namespace', 'n.oid', 'pg_namespace', 'n.oid') loop
      execute format('drop schema if exists %I cascade', r.nspname); dropped := dropped + 1;
    end loop;
    for r in execute format($q$
        select c.relkind, format('%%I.%%I', n.nspname, c.relname) as name
        from pg_class c join pg_namespace n on n.oid = c.relnamespace
        where c.relkind in ('r','p','v','m','S','f') and not c.relispartition
          and n.nspname not like 'pg_%%' and n.nspname <> 'information_schema'
          and not exists (select 1 from pg_depend d where d.objid = c.oid and d.deptype in ('a', 'i', 'e'))
          and $q$ || new_since_bootstrap, 'class', 'c.oid') loop
      execute format('drop %s if exists %s cascade',
        case r.relkind when 'v' then 'view' when 'm' then 'materialized view' when 'S' then 'sequence'
          when 'f' then 'foreign table' else 'table' end, r.name);
      dropped := dropped + 1;
    end loop;
    -- Indexes an app added to a platform table.
    for r in execute format($q$
        select format('%%I.%%I', n.nspname, c.relname) as name
        from pg_class c join pg_namespace n on n.oid = c.relnamespace join pg_index i on i.indexrelid = c.oid
        join pg_class t on t.oid = i.indrelid
        where c.relkind in ('i','I') and not t.relispartition and not c.relispartition
          and n.nspname not like 'pg_%%' and $q$ || new_since_bootstrap || ' and ' || not_extension_member
          || ' and exists (select 1 from stackbench_reset.baseline b where b.catalog = %L and b.oid = t.oid)',
        'class', 'c.oid', 'pg_class', 'c.oid', 'class') loop
      execute format('drop index if exists %s cascade', r.name); dropped := dropped + 1;
    end loop;
    for r in execute format($q$
        select p.oid::regprocedure::text as sig, p.prokind
        from pg_proc p join pg_namespace n on n.oid = p.pronamespace
        where n.nspname not like 'pg_%%' and n.nspname <> 'information_schema'
          and not exists (select 1 from pg_depend d where d.objid = p.oid and d.deptype = 'e')
          and $q$ || new_since_bootstrap, 'proc', 'p.oid') loop
      execute format('drop %s if exists %s cascade',
        case r.prokind when 'p' then 'procedure' when 'a' then 'aggregate' else 'function' end, r.sig);
      dropped := dropped + 1;
    end loop;
    for r in execute format($q$
        select format('%%I.%%I', n.nspname, t.typname) as name
        from pg_type t join pg_namespace n on n.oid = t.typnamespace
        where t.typrelid = 0 and t.typcategory <> 'A' and n.nspname not like 'pg_%%'
          and n.nspname <> 'information_schema'
          and not exists (select 1 from pg_depend d where d.objid = t.oid and d.deptype = 'e')
          and $q$ || new_since_bootstrap, 'type', 't.oid') loop
      execute format('drop type if exists %s cascade', r.name); dropped := dropped + 1;
    end loop;
    for r in execute format($q$
        select format('%%I on %%s', p.polname, p.polrelid::regclass) as target
        from pg_policy p where $q$ || new_since_bootstrap || ' and ' || not_extension_member,
        'policy', 'p.oid', 'pg_policy', 'p.oid') loop
      execute format('drop policy if exists %s', r.target); dropped := dropped + 1;
    end loop;
    for r in execute format($q$
        select format('%%I on %%s', t.tgname, t.tgrelid::regclass) as target
        from pg_trigger t join pg_class c on c.oid = t.tgrelid
        where not t.tgisinternal and not c.relispartition and $q$ || new_since_bootstrap || ' and ' || not_extension_member,
        'trigger', 't.oid', 'pg_trigger', 't.oid') loop
      execute format('drop trigger if exists %s', r.target); dropped := dropped + 1;
    end loop;
    for r in execute format($q$
        select p.pubname, pr.prrelid::regclass::text as rel
        from pg_publication_rel pr join pg_publication p on p.oid = pr.prpubid
        where $q$ || new_since_bootstrap || ' and ' || not_extension_member,
        'publication_rel', 'pr.oid', 'pg_publication_rel', 'pr.oid') loop
      execute format('alter publication %I drop table %s', r.pubname, r.rel); dropped := dropped + 1;
    end loop;
    for r in execute format('select pubname from pg_publication p where ' || new_since_bootstrap || ' and '
        || not_extension_member, 'publication', 'p.oid', 'pg_publication', 'p.oid') loop
      execute format('drop publication if exists %I', r.pubname); dropped := dropped + 1;
    end loop;
    for r in execute format('select evtname from pg_event_trigger e where ' || new_since_bootstrap || ' and '
        || not_extension_member, 'event_trigger', 'e.oid', 'pg_event_trigger', 'e.oid') loop
      execute format('drop event trigger if exists %I', r.evtname); dropped := dropped + 1;
    end loop;
    for r in execute format('select extname from pg_extension e where ' || new_since_bootstrap, 'extension', 'e.oid') loop
      execute format('drop extension if exists %I cascade', r.extname); dropped := dropped + 1;
    end loop;
    exit when dropped = 0;
    dropped := 0;
  end loop;
  for r in execute format('select rolname from pg_authid a where ' || new_since_bootstrap, 'role', 'a.oid') loop
    execute format('drop owned by %I cascade', r.rolname);
    execute format('drop role if exists %I', r.rolname);
  end loop;
  select string_agg(format('%I.%I', n.nspname, c.relname), ', ') into truncated
    from stackbench_reset.data_tables d join pg_class c on c.oid = d.name join pg_namespace n on n.oid = c.relnamespace;
  execute 'truncate ' || truncated || ' restart identity cascade';
  -- An app can drop the Realtime publication or remove the canary from it. Put
  -- both back and record them, so later resets keep them as platform objects.
  if not exists (select 1 from pg_publication where pubname = 'supabase_realtime') then
    create publication supabase_realtime;
    alter publication supabase_realtime owner to postgres;
    insert into stackbench_reset.baseline
      select 'publication', oid from pg_publication where pubname = 'supabase_realtime';
  end if;
  if not exists (select 1 from pg_publication_rel pr join pg_publication p on p.oid = pr.prpubid
      where p.pubname = 'supabase_realtime' and pr.prrelid = 'stackbench_reset.realtime_canary'::regclass) then
    alter publication supabase_realtime add table stackbench_reset.realtime_canary;
    insert into stackbench_reset.baseline
      select 'publication_rel', pr.oid from pg_publication_rel pr join pg_publication p on p.oid = pr.prpubid
      where p.pubname = 'supabase_realtime' and pr.prrelid = 'stackbench_reset.realtime_canary'::regclass;
  end if;
  notify pgrst, 'reload schema';
  return jsonb_build_object('passes', pass);
end $$;

-- Keeps supabase_realtime non-empty across resets. When a reset drops every app
-- table, Realtime's replication poller sees an empty publication and stops, and
-- only restarts after a later rescan; new subscribers get no changes meanwhile.
create table stackbench_reset.realtime_canary (id int primary key);
alter publication supabase_realtime add table stackbench_reset.realtime_canary;

-- Snapshot last, so this schema and its function count as platform objects.
insert into stackbench_reset.baseline
  select 'namespace', oid from pg_namespace
  union all select 'class', oid from pg_class
  union all select 'proc', oid from pg_proc
  union all select 'type', oid from pg_type
  union all select 'policy', oid from pg_policy
  union all select 'trigger', oid from pg_trigger
  union all select 'extension', oid from pg_extension
  union all select 'event_trigger', oid from pg_event_trigger
  union all select 'publication', oid from pg_publication
  union all select 'publication_rel', oid from pg_publication_rel
  union all select 'role', oid from pg_authid;
