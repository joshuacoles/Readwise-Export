#!/usr/bin/env zsh

./target/release/obsidian-readwise-rs \
  --vault "/Users/joshuacoles/Obsidian Sync/My Life" \
  --base-folder "Readwise" \
  --api-token "$READWISE_TOKEN" \
  --library "/Users/joshuacoles/Obsidian Sync/My Life/Readwise/library.json" \
  --book-template "$PWD/templates/book.md.tera" \
  --highlight-template "$PWD/templates/highlight.md.tera" \
  --replacement-strategy "replace" \
  --refetch
