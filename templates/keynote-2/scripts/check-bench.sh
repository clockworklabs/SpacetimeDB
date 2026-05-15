#!/usr/bin/env bash
# Health-check every backend the bench needs.
# Prints one line per service. Returns 0 even on failures — visual inspection.

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." &> /dev/null && pwd)"
CONVEX_URL=$(grep '^CONVEX_URL=' "$ROOT/convex-app/.env.local" 2>/dev/null | cut -d= -f2)

checks=(
  "sqlite_rpc|http://127.0.0.1:4103/rpc|{\"name\":\"health\",\"args\":{}}"
  "postgres_rpc|http://127.0.0.1:4101/rpc|{\"name\":\"health\",\"args\":{}}"
  "cockroach_rpc|http://127.0.0.1:4102/rpc|{\"name\":\"health\",\"args\":{}}"
  "bun|http://127.0.0.1:4001/rpc|{\"name\":\"health\",\"args\":{}}"
  "supabase_rpc|http://127.0.0.1:4106/rpc|{\"name\":\"health\",\"args\":{}}"
)

for c in "${checks[@]}"; do
  IFS='|' read -r name url body <<<"$c"
  out=$(curl -s --max-time 3 -X POST "$url" -H 'content-type: application/json' -d "$body" 2>&1)
  printf "%-15s %s\n" "$name" "${out:-NO RESPONSE}"
done

echo
printf "%-15s " "convex"
if [ -n "$CONVEX_URL" ]; then
  curl -s --max-time 3 "$CONVEX_URL/instance_name" 2>&1 || echo "(no response)"
  echo
else
  echo "URL not set in convex-app/.env.local"
fi
