CREATE OR REPLACE FUNCTION updated_at() RETURNS TRIGGER
LANGUAGE plpgsql
AS
$$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$;

CREATE EXTENSION pg_trgm;
CREATE EXTENSION citext;
CREATE SCHEMA registry;

CREATE TABLE registry.module (
  id SERIAL PRIMARY KEY,
  actor_name varchar(64) NOT NULL,
  st_identity char(64) NOT NULL,
  module_version INTEGER NOT NULL,
  module_address char(64) NOT NULL,
  UNIQUE(actor_name, st_identity, module_address, module_version)
);
