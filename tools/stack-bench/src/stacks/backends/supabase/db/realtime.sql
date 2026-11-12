-- supabase/supabase docker/volumes/db/realtime.sql at e8547352c529ed99545fafbc8619dec42945d74e.
\set pguser `echo "$POSTGRES_USER"`

create schema if not exists _realtime;
alter schema _realtime owner to :pguser;
