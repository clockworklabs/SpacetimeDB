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

CREATE TABLE registry.st_identity (
  onerow_id bool PRIMARY KEY DEFAULT TRUE,
  num INTEGER,
  CONSTRAINT onerow_uni CHECK (onerow_id)
);

CREATE TABLE registry.email (
  id SERIAL PRIMARY KEY,
  st_identity char(64) NOT NULL,
  email varchar(64) NOT NULL,
  UNIQUE(st_identity)
);
