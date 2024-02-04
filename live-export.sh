#!/usr/bin/env zsh
set -euxo pipefail

VAULT="${VAULT:-"/Users/joshuacoles/Obsidian Sync/My Life"}"

args=(
  --vault "$VAULT"
  --base-folder "Readwise"
  --api-token "$READWISE_TOKEN"
  --library "$VAULT/Readwise/library.json"
  --book-template "$PWD/templates/book.md.tera"
  --highlight-template "$PWD/templates/highlight.md.tera"
  --replacement-strategy "replace"
)

# Update the library
./target/release/obsidian-readwise-rs \
  "${args[@]}" \
  --fetch update \
  --no-export

# Skip articles if there are no highlights
./target/release/obsidian-readwise-rs \
  "${args[@]}" \
  --fetch cache \
  --filter-category "articles" \
  --skip-empty

# For books we want to write a file even if there are no highlights
./target/release/obsidian-readwise-rs \
  "${args[@]}" \
  --fetch cache \
  --filter-category "books"
