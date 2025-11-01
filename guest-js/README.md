# Tauri Plugin SQLite - JavaScript Bindings (@razein/tauri-plugin-rusqlite2)

This package provides the JavaScript/TypeScript bindings for the `@razein97/tauri-plugin-rusqlite2` Tauri plugin.

> **Note:** This is a fork of `tauri-plugin-sqlite` by @razein97 which is a fork of the official `tauri-plugin-sql` by @bspeckco. It has been modified to use `rusqlite` instead of `sqlx`, **supporting only SQLite databases**.

> It adds:
>
> - Transaction support (`beginTransaction`, `commitTransaction`, `rollbackTransaction`)
> - Migrations
> - Extensions support
> - SQLCipher support

## Installation

You need to install both the Rust Core plugin and these JavaScript bindings.

See the [main plugin README](../../README.md) for instructions on setting up the Rust Core plugin in your `Cargo.toml`.

Install the JavaScript bindings using your preferred package manager:

```bash
# Using pnpm
pnpm add @razein97/tauri-plugin-rusqlite2

# Using npm
npm install @razein97/tauri-plugin-rusqlite2

# Using yarn
yarn add @razein97/tauri-plugin-rusqlite2
```

### Rust bindings

Install the rust package using cargo:

```sh
cargo add tauri-plugin-rusqlite2
```

## Usage

Import the `Database` class and use the `load` method to connect to your SQLite database.

```typescript
import Database from '@razein97/tauri-plugin-rusqlite2';

async function initializeDb() {
  try {
    // Load a file-based database (relative to AppData dir)
    //to use without encryption leave the middle field empty. eg: 'sqlite::test.db'
    const db = await Database.load('sqlite:pass:test.db', [
      'path/to/ext_1',
      'path/to/ext_2',
    ]);

    // Or load an in-memory database
    // const db = await Database.load('sqlite::memory:');

    console.log('Database loaded successfully');

    // Example query
    await db.execute(
      'CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT)'
    );
    const result = await db.execute('INSERT INTO users (name) VALUES (?)', [
      'Test User',
    ]);
    console.log('Insert result:', result);

    const users = await db.select('SELECT * FROM users');
    console.log('Selected users:', users);

    // See the main plugin README for transaction examples
  } catch (error) {
    console.error('Failed to initialize database:', error);
  }
}

initializeDb();
```

### Extensions

All downloaded extension need to set read, write, executable permission to run on mac or linux.

```shell
- macos
chmod 755 path/to/ext.dylib

- linux
chmod 755 path/to/ext.so
```

Refer to the [main plugin README](../../README.md) for detailed API documentation, including transaction usage.
