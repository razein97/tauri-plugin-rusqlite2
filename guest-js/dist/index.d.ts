export interface QueryResult {
    /** The number of rows affected by the query. */
    rowsAffected: number;
    /**
     * The last inserted `id`.
     *
     * This value is not set for Postgres databases. If the
     * last inserted id is required on Postgres, the `select` function
     * must be used, with a `RETURNING` clause
     * (`INSERT INTO todos (title) VALUES ($1) RETURNING id`).
     */
    lastInsertId?: number;
}
/** Transaction identifier. */
export type TxId = string;
/**
 * **Database**
 *
 * The `Database` class serves as the primary interface for
 * communicating with the rust side of the sql plugin.
 */
export default class Database {
    path: string;
    constructor(path: string);
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
     * const db = await Database.load("sqlite:test.db", ["path/to/ext_1", "path/to/ext_2"]);
     * ```
     */
    static load(path: string, extensions: string[]): Promise<Database>;
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
    static get(path: string): Database;
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
    execute(query: string, bindValues?: unknown[], txId?: TxId): Promise<QueryResult>;
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
    select<T>(query: string, bindValues?: unknown[], txId?: TxId): Promise<T>;
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
    close(dbPath?: string): Promise<boolean>;
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
    beginTransaction(): Promise<TxId>;
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
    commitTransaction(txId: TxId): Promise<void>;
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
    rollbackTransaction(txId: TxId): Promise<void>;
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
    migrate(version: number): Promise<void>;
}
//# sourceMappingURL=index.d.ts.map