#!/usr/bin/env bash
echo "shared_buffers = '1024MB'" >> $PGDATA/postgresql.conf
echo "work_mem = '256MB'" >> $PGDATA/postgresql.conf
