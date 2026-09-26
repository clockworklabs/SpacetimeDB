-- supabase/supabase docker/volumes/db/_supabase.sql at e8547352c529ed99545fafbc8619dec42945d74e.
\set pguser `echo "$POSTGRES_USER"`

CREATE DATABASE _supabase WITH OWNER :pguser;
