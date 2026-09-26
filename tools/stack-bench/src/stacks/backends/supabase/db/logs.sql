-- supabase/supabase docker/volumes/db/logs.sql at e8547352c529ed99545fafbc8619dec42945d74e.
\set pguser `echo "$POSTGRES_USER"`

\c _supabase
create schema if not exists _analytics;
alter schema _analytics owner to :pguser;
\c postgres
