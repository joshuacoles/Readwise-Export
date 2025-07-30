#!/bin/bash
set -e

echo "Testing database support..."

# Test SQLite with file path (backward compatibility)
echo "1. Testing SQLite with file path..."
cargo run -- --database-url test-sqlite.db fetch --kind books

# Test SQLite with URL
echo "2. Testing SQLite with URL..."
cargo run -- --database-url sqlite://test-sqlite-url.db fetch --kind books

# Test PostgreSQL (requires running PostgreSQL)
echo "3. Testing PostgreSQL..."
echo "To test PostgreSQL, ensure you have a PostgreSQL server running and execute:"
echo "cargo run -- --database-url postgresql://user:password@localhost/readwise_test fetch --kind books"

echo "Tests completed!"