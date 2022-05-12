FROM library/postgres:9.6.5
ADD init-db.sql /docker-entrypoint-initdb.d/
ADD init-stats.sh /docker-entrypoint-initdb.d/
ADD init-settings.sh /docker-entrypoint-initdb.d/
