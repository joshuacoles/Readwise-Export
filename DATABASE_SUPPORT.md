# Database Support

This project now supports both SQLite and PostgreSQL databases at runtime.

## Configuration

Use the `--database-url` parameter or `DATABASE_URL` environment variable to specify your database:

### SQLite

```bash
# Using file path (backward compatible)
obsidian-readwise-export --database-url ./readwise.db fetch

# Using SQLite URL
obsidian-readwise-export --database-url sqlite://path/to/readwise.db fetch
```

### PostgreSQL

```bash
# Using PostgreSQL URL
obsidian-readwise-export --database-url postgresql://user:password@localhost/readwise_db fetch

# With environment variable
export DATABASE_URL="postgresql://user:password@localhost/readwise_db"
obsidian-readwise-export fetch
```

## Migration Notes

1. The project automatically detects the database type from the URL scheme
2. Migrations are stored separately:
    - SQLite: `migrations/sqlite/`
    - PostgreSQL: `migrations/postgres/`
3. The appropriate migrations are run automatically on startup

## Data Type Mappings

| SQLite   | PostgreSQL |
|----------|------------|
| INTEGER  | BIGINT     |
| DATETIME | TIMESTAMP  |
| TEXT     | TEXT       |
| REAL     | REAL       |

## Compatibility

- All existing SQLite databases continue to work
- The default remains SQLite for backward compatibility
- PostgreSQL requires a running PostgreSQL server
