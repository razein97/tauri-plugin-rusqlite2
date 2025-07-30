> **Note:** This is a fork of `tauri-plugin-sqlite` by @razein97 which is a fork of the official `tauri-plugin-sql`. It has been modified to use `rusqlite` instead of `sqlx`, **supporting only SQLite databases**. It adds explicit transaction support (`beginTransaction`, `commitTransaction`, `rollbackTransaction`).

Interface with SQLite databases using [rusqlite](https://github.com/rusqlite/rusqlite).

| Platform | Supported |
| -------- | --------- |
| Linux    | ✓         |
| Windows  | ✓         |
| macOS    | ✓         |
| Android  | ✓         |
| iOS      | ✓         |

## Install

_This plugin requires a Rust version of at least **1.77.2**_

Install the Core plugin by adding the following to your `Cargo.toml` file:

`src-tauri/Cargo.toml`

```toml
[dependencies.tauri-plugin-sql]
# Point this to your fork's repository and branch/tag/rev
# Example using a GitHub repo:
git = "https://github.com/bspeckco/tauri-v2-plugins-workspace"
branch = "v2"
# Or use a local path if developing locally:
# path = "../path/to/your/fork/tauri-plugin-sql"
```

You can install the JavaScript Guest bindings using your preferred JavaScript package manager:

```sh
# If you publish your fork's JS package:
# pnpm add @your-npm-scope/tauri-plugin-sql-fork
# or npm add @your-npm-scope/tauri-plugin-sql-fork
# or yarn add @your-npm-scope/tauri-plugin-sql-fork

# Alternatively, install directly from the JS directory in your fork:
# (Assuming your fork is checked out locally)
pnpm add ../path/to/your/fork/tauri-plugin-sql/guest-js
# or npm add ../path/to/your/fork/tauri-plugin-sql/guest-js
# or yarn add ../path/to/your/fork/tauri-plugin-sql/guest-js
```

## Usage

First you need to register the core plugin with Tauri:

`src-tauri/src/lib.rs`

```rust
fn main() {
    tauri::Builder::default()
        // Ensure you are using the Builder from *your* forked crate
        .plugin(tauri_plugin_sql::Builder::default().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Afterwards all the plugin's APIs are available through the JavaScript guest bindings:

```javascript
// Import from your fork's JS bindings
import Database from '@your-npm-scope/tauri-plugin-sql-fork'; // Or the local path

// sqlite. The path can be relative to `tauri::api::path::BaseDirectory::AppConfig`
// or absolute.
const db = await Database.load('sqlite:test.db');
// In-memory database
const memoryDb = await Database.load('sqlite::memory:');

await db.execute('INSERT INTO users (name) VALUES (?)', ['Test']);
const users = await db.select('SELECT * FROM users');
```

## Syntax

Queries use the standard SQLite placeholder syntax (`?`).

```javascript
// INSERT example
const result = await db.execute(
  'INSERT into todos (id, title, status) VALUES (?, ?, ?)',
  [todos.id, todos.title, todos.status]
);

// UPDATE example
const result = await db.execute(
  'UPDATE todos SET title = ?, status = ? WHERE id = ?',
  [todos.title, todos.status, todos.id]
);

// SELECT example
const users = await db.select('SELECT * from users WHERE name = ?', ['Alice']);
```

## Transactions

This plugin supports explicit transaction control via the `beginTransaction`, `commitTransaction`, and `rollbackTransaction` methods.

```javascript
import Database from '...'; // Your fork's import

const db = await Database.load('sqlite:my_app_data.db');

async function performAtomicUpdate(userId, newName, newItem) {
  let txId = null;
  try {
    // Start a transaction
    txId = await db.beginTransaction();
    console.log(`Started transaction: ${txId}`);

    // Perform operations within the transaction using the txId
    await db.execute(
      'UPDATE users SET name = ? WHERE id = ?',
      [newName, userId],
      txId // Pass the transaction ID
    );

    await db.execute(
      'INSERT INTO items (name, owner_id) VALUES (?, ?)',
      [newItem, userId],
      txId // Pass the transaction ID
    );

    // Commit the transaction
    await db.commitTransaction(txId);
    console.log(`Committed transaction: ${txId}`);
  } catch (error) {
    console.error('Transaction failed:', error);
    // If an error occurred and we started a transaction, roll it back
    if (txId) {
      try {
        await db.rollbackTransaction(txId);
        console.log(`Rolled back transaction: ${txId}`);
      } catch (rollbackError) {
        console.error('Failed to rollback transaction:', rollbackError);
      }
    }
    // Re-throw the original error or handle it appropriately
    throw error;
  }
}
```

Queries run outside of an explicit transaction (i.e., without providing a `txId` to `execute` or `select`) are executed on a temporary connection and are implicitly committed individually.

## Migrations

This plugin supports database migrations, allowing you to manage database schema evolution over time.

### Defining Migrations

Migrations are defined in Rust using the `Migration` struct. Each migration should include a unique version number, a description, the SQL to be executed, and the type of migration (Up or Down).

Example of a migration:

```rust
use tauri_plugin_sql::{Migration, MigrationKind};

let migration = Migration {
    version: 1,
    description: "create_initial_tables",
    sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
    kind: MigrationKind::Up,
};
```

### Adding Migrations to the Plugin Builder

Migrations are registered with the `Builder` struct provided by the plugin. Use the `add_migrations` method to add your migrations to the plugin for a specific database connection.

Example of adding migrations:

```rust
use tauri_plugin_sql::{Builder, Migration, MigrationKind};

fn main() {
    let migrations = vec![
        // Define your migrations here
        Migration {
            version: 1,
            description: "create_initial_tables",
            sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
            down_sql: "DROP TABLE users",
            //kind is not used in this version
            kind: MigrationKind::Up,
        }
    ];

    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:mydatabase.db", migrations)
                .build(),
        )
        ...
}
```

### Applying Migrations

To apply the migrations when the plugin is initialized, add the connection string to the `tauri.conf.json` file:

```json
{
  "plugins": {
    "sql": {
      "preload": ["sqlite:mydatabase.db"]
    }
  }
}
```

Alternatively, the client side `load()` also runs the migrations for a given connection string:

```ts
import Database from '@tauri-apps/plugin-sql';
const db = await Database.load('sqlite:mydatabase.db');
```

### Rolling back migrations

To roll back transactions, the

```

```

Ensure that the migrations are defined in the correct order and are safe to run multiple times.

### Migration Management

- **Version Control**: Each migration must have a unique version number. This is crucial for ensuring the migrations are applied in the correct order.
- **Idempotency**: Write migrations in a way that they can be safely re-run without causing errors or unintended consequences.
- **Testing**: Thoroughly test migrations to ensure they work as expected and do not compromise the integrity of your database.

## Contributing

PRs accepted to the _original_ Tauri repository. Please make sure to read the Contributing Guide before making a pull request there.

## Partners

<table>
  <tbody>
    <tr>
      <td align="center" valign="middle">
        <a href="https://crabnebula.dev" target="_blank">
          <img src="https://github.com/tauri-apps/plugins-workspace/raw/v2/.github/sponsors/crabnebula.svg" alt="CrabNebula" width="283">
        </a>
      </td>
    </tr>
  </tbody>
</table>

For the complete list of sponsors please visit our [website](https://tauri.app#sponsors) and [Open Collective](https://opencollective.com/tauri).

## License

Code: (c) 2015 - Present - The Tauri Programme within The Commons Conservancy.

MIT or MIT/Apache 2.0 where applicable.
