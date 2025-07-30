// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import { invoke } from '@tauri-apps/api/core'

export interface QueryResult {
  /** The number of rows affected by the query. */
  rowsAffected: number
  /**
   * The last inserted `id`.
   *
   * This value is not set for Postgres databases. If the
   * last inserted id is required on Postgres, the `select` function
   * must be used, with a `RETURNING` clause
   * (`INSERT INTO todos (title) VALUES ($1) RETURNING id`).
   */
  lastInsertId?: number
}

/** Transaction identifier. */
export type TxId = string

/**
 * **Database**
 *
 * The `Database` class serves as the primary interface for
 * communicating with the rust side of the sql plugin.
 */
export default class Database {
  path: string
  constructor(path: string) {
    this.path = path
  }

  /**
   * **load**
   *
   * A static initializer which connects to the underlying database and
   * returns a `Database` instance once a connection to the database is established.
   *
   * # Sqlite
   *
   * The path is relative to `tauri::path::BaseDirectory::App` and must start with `sqlite:`.
   *
   * @example
   * ```ts
   * const db = await Database.load("sqlite:test.db");
   * ```
   */
  static async load(path: string): Promise<Database> {
    const _path = await invoke<string>('plugin:rusqlite|load', {
      db: path
    })

    return new Database(_path)
  }

  /**
   * **get**
   *
   * A static initializer which synchronously returns an instance of
   * the Database class while deferring the actual database connection
   * until the first invocation or selection on the database.
   * NOTE: This is likely deprecated with the new connection handling.
   *
   * # Sqlite
   *
   * The path is relative to `tauri::path::BaseDirectory::App` and must start with `sqlite:`.
   *
   * @example
   * ```ts
   * const db = Database.get("sqlite:test.db");
   * ```
   */
  static get(path: string): Database {
    // TODO: Revisit if this method is still valid/useful.
    // With the new model, operations always open a connection (temp or TX).
    // There isn't a persistent connection to "defer".
    return new Database(path)
  }

  /**
   * **execute**
   *
   * Passes a SQL expression to the database for execution.
   * Can be used for `INSERT`, `UPDATE`, `DELETE`, `CREATE`, etc.
   * Optionally runs within a transaction identified by `txId`.
   *
   * @param query - The SQL query string.
   * @param bindValues - Optional array of values to bind to placeholders in the query.
   * @param txId - Optional transaction identifier. If provided, the query runs within that transaction.
   * @returns A Promise resolving to the query result.
   *
   * @example
   * ```ts
   * // Simple insert
   * const result = await db.execute(
   *    "INSERT into users (name) VALUES (?)",
   *    [ 'Bob' ]
   * );
   *
   * // Insert within a transaction
   * const tx = await db.beginTransaction();
   * try {
   *   const result = await db.execute(
   *     "INSERT into items (name, owner_id) VALUES (?, ?)",
   *     [ 'Laptop', 1 ],
   *     tx
   *   );
   *   await db.commitTransaction(tx);
   * } catch (e) {
   *   await db.rollbackTransaction(tx);
   * }
   * ```
   */
  async execute(query: string, bindValues?: unknown[], txId?: TxId): Promise<QueryResult> {
    const [rowsAffected, lastInsertId] = await invoke<[number, number]>(
      'plugin:rusqlite|execute',
      {
        dbAlias: this.path,
        query,
        values: bindValues ?? [],
        txId: txId ?? null
      }
    )
    return {
      lastInsertId,
      rowsAffected
    }
  }

  /**
   * **select**
   *
   * Passes in a SELECT query to the database for execution.
   * Optionally runs within a transaction identified by `txId`.
   *
   * @param query - The SQL query string.
   * @param bindValues - Optional array of values to bind to placeholders in the query.
   * @param txId - Optional transaction identifier. If provided, the query runs within that transaction.
   * @returns A Promise resolving to the selected rows.
   *
   * @example
   * ```ts
   * const users = await db.select<Array<{ id: number; name: string }>>(
   *    "SELECT id, name from users WHERE id = ?", [ 1 ]
   * );
   *
   * // Select within a transaction
   * const tx = await db.beginTransaction();
   * const items = await db.select<Array<{ name: string }>>(
   *   "SELECT name FROM items WHERE owner_id = ?",
   *   [ 1 ],
   *   tx
   * );
   * await db.rollbackTransaction(tx); // Or commit
   * ```
   */
  async select<T>(query: string, bindValues?: unknown[], txId?: TxId): Promise<T> {
    const result = await invoke<T>('plugin:rusqlite|select', {
      dbAlias: this.path,
      query,
      values: bindValues ?? [],
      txId: txId ?? null
    })

    return result
  }

  /**
   * **close**
   *
   * Removes the database alias association. This prevents new operations
   * from being started with this alias until `load` is called again.
   * Does not affect currently active transactions, which will continue until
   * committed or rolled back.
   *
   * @example
   * ```ts
   * const success = await db.close()
   * ```
   * @param dbPath - The specific database path/alias to close. If omitted, attempts to close the alias associated with this `Database` instance.
   */
  async close(dbPath?: string): Promise<boolean> {
    const success = await invoke<boolean>('plugin:rusqlite|close', {
      db: dbPath ?? this.path // Use provided path or instance path
    })
    return success
  }

  // --- Transaction Commands ---

  /**
   * **beginTransaction**
   *
   * Starts a new transaction and returns a unique transaction identifier.
   * All subsequent `execute` or `select` calls using this identifier will run
   * within the same transaction context.
   *
   * @returns A Promise resolving to the transaction identifier string.
   *
   * @example
   * ```ts
   * const txId = await db.beginTransaction();
   * ```
   */
  async beginTransaction(): Promise<TxId> {
    return await invoke<TxId>('plugin:rusqlite|begin_transaction', {
      dbAlias: this.path
    })
  }

  /**
   * **commitTransaction**
   *
   * Commits the transaction identified by `txId`.
   *
   * @param txId - The transaction identifier returned by `beginTransaction`.
   *
   * @example
   * ```ts
   * await db.commitTransaction(txId);
   * ```
   */
  async commitTransaction(txId: TxId): Promise<void> {
    await invoke<void>('plugin:rusqlite|commit_transaction', { txId })
  }

  /**
   * **rollbackTransaction**
   *
   * Rolls back the transaction identified by `txId`.
   *
   * @param txId - The transaction identifier returned by `beginTransaction`.
   *
   * @example
   * ```ts
   * await db.rollbackTransaction(txId);
   * ```
   */
  async rollbackTransaction(txId: TxId): Promise<void> {
    await invoke<void>('plugin:rusqlite|rollback_transaction', { txId })
  }

  /**
 * **Migrate To Version**
 *
 * Runs the migrations till the specific migration version defined.
 *
 * @param version - The version to migrate to.
 *
 * @example
 * ```ts
 * await db.migrate(version);
 * ```
 */
  async migrate(version: number): Promise<void> {
    await invoke<void>('plugin:rusqlite|migrate', { version, db: this.path, })
  }

}





