> **Note:** This is a fork of `tauri-plugin-sqlite` by @razein97 which is a fork of the official `tauri-plugin-sql` by @bspeckco. It has been modified to use `rusqlite` instead of `sqlx`, **supporting only SQLite databases**.

> It adds:
>
> - Transaction support (`beginTransaction`, `commitTransaction`, `rollbackTransaction`)
> - Migrations
> - Extensions support
> - SQLCipher support

Interface with SQLite databases using [rusqlite](https://github.com/rusqlite/rusqlite).

| Platform | Supported |
| -------- | --------- |
| Linux    | ✓         |
| Windows  | ✓         |
| macOS    | ✓         |
| Android  | Untested  |
| iOS      | Untested  |

## Install

_This plugin requires a Rust version of at least **1.77.2**_

Install the Core plugin by adding the following to your `Cargo.toml` file:

`src-tauri/Cargo.toml`

```toml
# Point this to your fork's repository and branch/tag/rev
# Example using a GitHub repo:
[dependencies.tauri-plugin-rusqlite2]
git = "https://github.com/razein97/tauri-plugin-rusqlite2"

# Or use a local path if developing locally:
# path = "../path/to/your/fork/tauri-plugin-rusqlite"
```

The package can also be installed by using cargo:

```sh
cargo add tauri-plugin-rusqlite2
```

You can install the JavaScript Guest bindings using your preferred JavaScript package manager:

Install the JavaScript bindings using your preferred package manager:

```bash
# Using pnpm
pnpm add @razein97/tauri-plugin-rusqlite2

# Using npm
npm install @razein97/tauri-plugin-rusqlite2

# Using yarn
yarn add @razein97/tauri-plugin-rusqlite2
```

## Extensions

All downloaded extension need to set read, write, executable permission to run on mac or linux.

```shell
- macos
chmod 755 path/to/ext.dylib

- linux
chmod 755 path/to/ext.so
```

## Usage

First you need to register the core plugin with Tauri:

`src-tauri/src/lib.rs`

```rust
fn main() {
    tauri::Builder::default()
        // Ensure you are using the Builder from *your* forked crate
        .plugin(tauri_plugin_rusqlite2::Builder::default().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Afterwards all the plugin's APIs are available through the JavaScript guest bindings and also via `tauri::AppHandle`:

### JS

```javascript
// Import from your fork's JS bindings
import Database from '@razein97/tauri-plugin-rusqlite2'; // Or the local path

// sqlite. The path can be relative to `tauri::api::path::BaseDirectory::AppConfig` or absolute.

//the pass field can be left empty to disable encryption. 'sqlite::test.db'
const db = await Database.load('sqlite:pass:test.db', [
  'path/to/ext_1',
  'path/to/ext_2',
]);

// In-memory database
const memoryDb = await Database.load('sqlite::memory:', [
  'path/to/ext_1',
  'path/to/ext_2',
]);

await db.execute('INSERT INTO users (name) VALUES (?)', ['Test']);
const users = await db.select('SELECT * FROM users');
```

### Rust

```rust

// sqlite. The path can be relative to `tauri::api::path::BaseDirectory::AppConfig` or absolute.

#[tauri::command]
fn load_database(app: tauri::AppHandle) {
  let db = app.rusqlite2_connection()
            .load("sqlite:pass:test.db", vec!["path/to/ext_1", "path/to/ext2"])
            .unwrap();


let memory_db = app.rusqlite2_connection()
                .load("sqlite::memory:",vec!["path/to/ext_1", "path/to/ext2"])
                    .unwrap();

let result:Result<(u64, LastInsertId), Error> =
    app.rusqlite2_connection().execute(
        db,
        "INSERT into users (name) VALUES (?)".to_string(),
        params![["BOB"].iter().map(|f| f).collect()],
    None,
);

let result:Result<Vec<IndexMap<String, JsonValue>>, Error> =
    app.rusqlite2_connection().select(
        db,
        "SELECT name from items WHERE owner_id = ?".to_string(),
        params![1],
    None,
  );
}
```

## Syntax

Queries use the standard SQLite placeholder syntax (`?`).

### JS

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

### Rust

```rust
//INSERT example
let result =
    app.rusqlite2_connection().execute(
        db,
        "INSERT into todos (id, title, status) VALUES (?, ?, ?)".to_string(),
        params![todos.id, todos.title, todos.status],
    None,
);


//UPDATE example
let result =
    app.rusqlite2_connection().execute(
        db,
        "UPDATE todos SET title = ?, status = ? WHERE id = ?".to_string(),
        params![todos.id, todos.title, todos.status],
    None,
);


//SELECT example
let result:Result<Vec<IndexMap<String, JsonValue>>, Error> =
     app.rusqlite2_connection().select(
         db,
         "SELECT * from users WHERE name = ?".to_string(),
         params!["Alice"],
     None,
 );

```

## Transactions

This plugin supports explicit transaction control via the `beginTransaction`, `commitTransaction`, and `rollbackTransaction` methods.

### JS

```javascript
import Database from '...'; // Your fork's import

const db = await Database.load('sqlite:pass:test.db', []);

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

### Rust

```rust
  let tx = app.rusqlite2_connection().begin_transaction(db).unwrap();

  let txn = app
      .rusqlite2_connection()
       .select(
           db,
           "INSERT into items (name, owner_id) VALUES (?, ?)".to_string(),
           params!["Laptop", 1],
           Some(tx),
       )
       .unwrap();

   let commit = app.rusqlite2_connection().commit_transaction(tx);

   if commit.is_err() {
       app.rusqlite2_connection().rollback_transaction(tx);
   }
```

Queries run outside of an explicit transaction (i.e., without providing a `txId` to `execute` or `select`) are executed on a temporary connection and are implicitly committed individually.

## Migrations

This plugin supports database migrations, allowing you to manage database schema evolution over time.

### Defining Migrations

Migrations are defined in Rust using the `Migration` struct. Each migration should include a unique version number, a description, the SQL to be executed, and the type of migration (Up or Down).

Example of a migration:

```rust
use tauri_plugin_rusqlite2::{Migration, MigrationKind};

let migration = Migration {
    version: 1,
    description: "create_initial_tables",
    sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
    down_sql: "DROP TABLE users;"
    kind: MigrationKind::Up,
};
```

### Adding Migrations to the Plugin Builder

Migrations are registered with the `Builder` struct provided by the plugin. Use the `add_migrations` method to add your migrations to the plugin for a specific database connection.

Example of adding migrations:

```rust
use tauri_plugin_rusqlite2::{Builder, Migration, MigrationKind};

fn main() {
    let migrations = vec![
        // Define your migrations here
        Migration {
            version: 1,
            description: "create_initial_tables",
            sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
            down_sql: "DROP TABLE users",
            //Kind is ignored (Retained for compatibility)
            kind: MigrationKind::Up,
        }
    ];

    tauri::Builder::default()
        .plugin(
            tauri_plugin_rusqlite2::Builder::default()
                .add_migrations("sqlite:pass:test.db", migrations)
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
    "rusqlite2": {
      "preload": ["sqlite:pass:test.db"]
    }
  }
}
```

Alternatively, the client side `load()` also runs the migrations for a given connection string:

```ts
import Database from '@razein97/tauri-plugin-rusqlite2';
const db = await Database.load('sqlite:pass:test.db', []);
```

### Rolling back migrations

Apply any migration version, using method provided by the connection

```javascript
await db.migrate(version);
```

```rust
app.rusqlite2_connection().migrate(1).expect("Could not migrate database");
```

Ensure that the migrations are defined in the correct order and are safe to run multiple times.

### Migration Management

- **Version Control**: Each migration must have a unique version number. This is crucial for ensuring the migrations are applied in the correct order.
- **Idempotency**: Write migrations in a way that they can be safely re-run without causing errors or unintended consequences.
- **Testing**: Thoroughly test migrations to ensure they work as expected and do not compromise the integrity of your database.

## Contributing

PRs accepted to the repository. Please make sure to read the Contributing Guide before making a pull request there.

## License

MIT or MIT/Apache 2.0 where applicable.
