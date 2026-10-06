-- supabase/supabase docker/volumes/db/jwt.sql at e8547352c529ed99545fafbc8619dec42945d74e.
\set jwt_exp `echo "$JWT_EXP"`

ALTER DATABASE postgres SET "app.settings.jwt_exp" TO :'jwt_exp';
