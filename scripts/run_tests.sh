#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
cd "$ROOT"

echo "1) Start Postgres test container..."
docker-compose -f docker-compose.test.yml up -d

echo "Waiting for Postgres to be healthy..."
# poll pg_isready
for i in $(seq 1 30); do
  docker exec $(docker-compose -f docker-compose.test.yml ps -q pgtest) pg_isready -U ouro -d ouro_db && break
  sleep 1
done

echo "2) Set DATABASE_URL env var for local tests"
export DATABASE_URL="postgres://ouro:ouro_pass@127.0.0.1:15432/ouro_db"

echo "3) Run SQL migrations (sqlx cli must be installed) - skip if you don't have it"
if command -v sqlx >/dev/null 2>&1; then
  echo "Running sqlx migrate run..."
  sqlx migrate run
else
  echo "sqlx not found; please run migrations manually or install sqlx-cli (recommended)."
fi

echo "4) cargo build --release"
cargo build --release

echo "5) Run cargo test"
# Run tests, forward stdout/stderr
cargo test -- --nocapture

TEST_EXIT_CODE=$?

echo "6) Tear down Postgres test container"
docker-compose -f docker-compose.test.yml down -v

exit $TEST_EXIT_CODE
