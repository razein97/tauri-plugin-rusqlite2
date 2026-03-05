// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use indexmap::IndexMap;
use log::error;
use rusqlite_migration::Migrations as RusqliteMigrations;
use serde_json::Value as JsonValue;
use tauri::Manager;
use tauri::{command, AppHandle, Runtime, State};

use crate::utils::lock_mutex;
// Updated imports
use crate::{convert, DbInfo, Error, LastInsertId, MigrationList, Rusqlite2Connections}; // Removed DbInfo
use rusqlite::Connection; // Removed params_from_iter, Statement
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex}; // Added missing import
use std::time::Duration;
use uuid::Uuid;

/// Opens and configures a brand-new `Connection` from a `DbInfo`.
/// Used by `begin_transaction` and `migrate` which need their own dedicated connection.
fn open_configured_conn(db_info: &DbInfo) -> Result<Connection, crate::Error> {
    let conn = Connection::open(&db_info.path)
        .map_err(|e| Error::ConnectionFailed(db_info.path.display().to_string(), e.to_string()))?;

    if !db_info.pass.is_empty() {
        conn.pragma_update(None, "KEY", &db_info.pass)
            .map_err(|e| {
                Error::ConnectionFailed(db_info.path.display().to_string(), e.to_string())
            })?;
    }

    load_extensions(&conn, &db_info.extensions)?;

    conn.busy_timeout(Duration::from_millis(5000))
        .map_err(Error::Rusqlite)?;

    Ok(conn)
}

fn load_extensions(conn: &Connection, extensions: &[String]) -> Result<(), crate::Error> {
    // Load extensions
    unsafe {
        conn.load_extension_enable()
            .map_err(|e| Error::ExtensionLoadFailed(e.to_string()))?;

        for ext in extensions {
            if !ext.is_empty() {
                if let Err(e) = conn.load_extension(ext, None::<&str>) {
                    return Err(Error::ExtensionLoadFailed(e.to_string()));
                }
            }
        }

        conn.load_extension_disable()
            .map_err(|e| Error::ExtensionLoadFailed(e.to_string()))?;

        Ok(())
    }
}

#[command]
pub(crate) fn get_conn_url<R: Runtime>(
    app: AppHandle<R>,
    db: &str,
) -> Result<PathBuf, crate::Error> {
    let split_db_conn: Vec<&str> = db.splitn(3, ':').collect();
    let kind = split_db_conn[0];
    let pass = split_db_conn[1];
    let path_part = split_db_conn[2];

    if kind != "sqlite" {
        return Err(Error::UnsupportedDatabaseType(kind.to_string()));
    }

    let path = if path_part == ":memory:" {
        PathBuf::from(":memory:")
    } else {
        let base_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| Error::Io(format!("Failed to get app_data_dir: {}", e)))?;
        let resolved_path = base_dir.join(path_part);
        if let Some(parent_dir) = resolved_path.parent() {
            std::fs::create_dir_all(parent_dir)
                .map_err(|e| Error::Io(format!("Failed to create parent directory: {}", e)))?;
        }
        resolved_path
    };

    // Verify we can open/close a connection, but don't keep it open.
    // This checks permissions and path validity.
    let conn = Connection::open(&path)
        .map_err(|e| Error::ConnectionFailed(path.display().to_string(), e.to_string()))?;

    if !pass.is_empty() {
        let pragma_pass_res = conn.pragma_update(None, "KEY", pass);

        match pragma_pass_res {
            Ok(_) => {
                conn.close().map_err(|(_, e)| {
                    Error::ConnectionFailed(
                        path.display().to_string(),
                        format!("Failed to close test connection: {}", e),
                    )
                })?;

                Ok(path)
            }
            Err(_) => {
                //Convert plaintext sqlite to encrypted
                conn.execute(
                    &format!("ATTACH DATABASE '{path_part}' AS encrypted KEY '{pass}';"),
                    [],
                )
                .map_err(|e| Error::EncryptionFailed(e.to_string()))?;

                conn.execute("SELECT sqlcipher_export('encrypted')", [])
                    .map_err(|e| Error::EncryptionFailed(e.to_string()))?;

                conn.execute("DETACH DATABASE encrypted;", [])
                    .map_err(|e| Error::EncryptionFailed(e.to_string()))?;

                conn.close().map_err(|(_, e)| {
                    Error::ConnectionFailed(
                        path.display().to_string(),
                        format!("Failed to close test connection: {}", e),
                    )
                })?;

                std::fs::copy(path_part, &path)
                    .map_err(|e| Error::EncryptionFailed(e.to_string()))?;

                Ok(path)
            }
        }
    } else {
        conn.close().map_err(|(_, e)| {
            Error::ConnectionFailed(
                path.display().to_string(),
                format!("Failed to close test connection: {}", e),
            )
        })?;

        Ok(path)
    }
}

// Refactored load command
#[command]
pub(crate) fn load<R: Runtime>(
    app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db: &str,
    extensions: Vec<String>,
) -> Result<String, crate::Error> {
    let split_db_conn: Vec<&str> = db.splitn(3, ':').collect();
    let kind = split_db_conn[0];
    let pass = split_db_conn[1];
    let path_part = split_db_conn[2];

    if kind != "sqlite" {
        return Err(Error::UnsupportedDatabaseType(kind.to_string()));
    }

    let path = if path_part == ":memory:" {
        PathBuf::from(":memory:")
    } else {
        let base_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| Error::Io(format!("Failed to get app_data_dir: {}", e)))?;
        let resolved_path = base_dir.join(path_part);
        if let Some(parent_dir) = resolved_path.parent() {
            std::fs::create_dir_all(parent_dir)
                .map_err(|e| Error::Io(format!("Failed to create parent directory: {}", e)))?;
        }
        resolved_path
    };

    let db_info = DbInfo {
        path: path.clone(),
        extensions: extensions.clone(),
        pass: pass.to_string(),
    };

    // Open, configure and keep the connection — this becomes the pool entry.
    // open_configured_conn validates pass, loads extensions, sets busy timeout.
    let conn = open_configured_conn(&db_info).map_err(|e| {
        error!("{e:?}");
        e
    })?;
    let conn_arc = Arc::new(Mutex::new(conn));

    // Store DbInfo and insert the live connection into the pool.
    // If the alias was already loaded the old pool Arc is dropped here,
    // which closes the previous connection once no other thread holds it.
    {
        let mut connection_map = connections.inner().connections.0.lock().unwrap();
        if connection_map.contains_key(db) {
            log::warn!("Database alias '{}' already loaded. Overwriting.", db);
        }
        connection_map.insert(db.to_string(), db_info);
    }
    connections
        .inner()
        .pool
        .0
        .lock()
        .unwrap()
        .insert(db.to_string(), conn_arc);

    Ok(db.to_string())
}

/// Allows the database connection(s) to be closed; if no database
/// name is passed in then _all_ database connection pools will be
/// shut down.
#[command]
pub(crate) fn close<R: Runtime>(
    _app: AppHandle<R>,
    // Removed async as no async ops needed now
    connections: State<'_, Rusqlite2Connections<R>>,
    // transactions: State<'_, TransactionManager>, // TODO: Handle open transactions?
    db: Option<String>,
) -> Result<bool, crate::Error> {
    // Changed return to match old signature (bool)
    let mut connection_map = lock_mutex(&connections.inner().connections.0, "ConnectionManager")?;

    let mut pool = lock_mutex(&connections.inner().pool.0, "ConnectionManager")?;

    let aliases_to_remove = if let Some(db_alias) = db {
        if !connection_map.contains_key(&db_alias) {
            // Return Ok(false) or Error? Old code returned Error::DatabaseNotLoaded.
            // Let's stick to that for now.
            return Err(Error::DatabaseNotLoaded(db_alias));
        }
        vec![db_alias]
    } else {
        connection_map.keys().cloned().collect()
    };

    for alias in aliases_to_remove {
        // Remove the alias from the connection manager.
        // Note: This does not affect active transactions associated with this alias.
        // Active transactions hold their own connection Arc and will continue until
        // commit or rollback. The connection is closed when the Arc count drops to 0.
        // Attempting to start *new* operations (load, execute, select, begin_transaction)
        // with this alias will fail until it is loaded again.
        connection_map.remove(&alias);
        pool.remove(&alias);
    }

    Ok(true)
}

// --- Transaction Commands --- Implementation ---

#[command]
pub(crate) fn begin_transaction<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db_alias: &str,
) -> Result<String, crate::Error> {
    // Get DbInfo from ConnectionManager
    let db_info = connections
        .inner()
        .connections
        .0
        .lock()
        .unwrap()
        .get(db_alias)
        .cloned()
        .ok_or_else(|| Error::DatabaseNotLoaded(db_alias.to_string()))?;

    // Transactions always get their own dedicated connection
    let tx_conn = open_configured_conn(&db_info)?;

    // Begin the transaction on the new connection
    // Use IMMEDIATE (default behavior, allows concurrent reads until first write)
    tx_conn
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(Error::Rusqlite)?;

    // Generate ID and store the new connection (wrapped in Arc<Mutex<_>>) in TransactionManager
    let tx_id = Uuid::new_v4();
    connections
        .inner()
        .transactions
        .0
        .lock()
        .unwrap()
        .insert(tx_id, Arc::new(Mutex::new(tx_conn)));

    Ok(tx_id.to_string())
}

#[command]
pub(crate) fn commit_transaction<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    tx_id: &str,
) -> Result<(), crate::Error> {
    let uuid = Uuid::from_str(tx_id).map_err(|_| Error::InvalidUuid(tx_id.to_string()))?;

    // Ensure correct State access
    let maybe_conn = connections
        .inner()
        .transactions
        .0
        .lock()
        .unwrap()
        .remove(&uuid);

    match maybe_conn {
        Some(conn_arc) => conn_arc
            .lock()
            .unwrap()
            .execute_batch("COMMIT")
            .map_err(Error::Rusqlite),
        None => Err(Error::TransactionNotFound(tx_id.to_string())),
    }
}

#[command]
pub(crate) fn rollback_transaction<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    tx_id: &str,
) -> Result<(), crate::Error> {
    let uuid = Uuid::from_str(tx_id).map_err(|_| Error::InvalidUuid(tx_id.to_string()))?;

    // Ensure correct State access
    let maybe_conn = connections
        .inner()
        .transactions
        .0
        .lock()
        .unwrap()
        .remove(&uuid);

    match maybe_conn {
        Some(conn_arc) => {
            if let Err(e) = lock_mutex(&conn_arc, "ConnectionManager")?.execute_batch("ROLLBACK") {
                log::error!("Error rolling back transaction {}: {}", tx_id, e);
            }
            Ok(())
        }
        None => Err(Error::TransactionNotFound(tx_id.to_string())),
    }
}

// --- Existing Commands to be Refactored (Step 6 & 7) ---

/// Execute a command against the database
#[command]
pub(crate) fn execute<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db_alias: &str,
    query: &str,
    values: Vec<JsonValue>,
    tx_id: Option<String>,
) -> Result<(u64, LastInsertId), crate::Error> {
    let converted_params = convert::json_to_rusqlite_params(values)?;

    if let Some(tx_id_str) = tx_id {
        // --- transactional path: use the transaction's dedicated connection ---
        let uuid = Uuid::from_str(&tx_id_str).map_err(|_| Error::InvalidUuid(tx_id_str.clone()))?;
        let tx_map = lock_mutex(&connections.inner().transactions.0, "ConnectionManager")?;
        let conn_arc = tx_map
            .get(&uuid)
            .cloned()
            .ok_or_else(|| Error::TransactionNotFound(tx_id_str))?;

        let conn = lock_mutex(&conn_arc, "ConnectionManager")?;
        let changes = conn
            .execute(query, rusqlite::params_from_iter(converted_params))
            .map_err(Error::Rusqlite)?;
        let last_id = conn.last_insert_rowid();
        Ok((changes as u64, LastInsertId::Sqlite(last_id)))
    } else {
        // --- non-transactional path: use the pooled persistent connection ---
        let conn_arc = connections.inner().get_conn(db_alias)?;
        let conn = lock_mutex(&conn_arc, "ConnectionManager")?;
        let changes = conn
            .execute(query, rusqlite::params_from_iter(converted_params))
            .map_err(Error::Rusqlite)?;
        let last_id = conn.last_insert_rowid();
        Ok((changes as u64, LastInsertId::Sqlite(last_id)))
    }
}

#[command]
pub(crate) fn select<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db_alias: &str,
    query: &str,
    values: Vec<JsonValue>,
    tx_id: Option<String>,
) -> Result<Vec<IndexMap<String, JsonValue>>, crate::Error> {
    let converted_params = convert::json_to_rusqlite_params(values)?;

    if let Some(tx_id_str) = tx_id {
        // --- transactional path ---
        let uuid = Uuid::from_str(&tx_id_str).map_err(|_| Error::InvalidUuid(tx_id_str.clone()))?;
        let tx_map = lock_mutex(&connections.inner().transactions.0, "ConnectionManager")?;

        let conn_arc = tx_map
            .get(&uuid)
            .cloned()
            .ok_or_else(|| Error::TransactionNotFound(tx_id_str))?;

        let conn = lock_mutex(&conn_arc, "ConnectionManager")?;
        query_rows(&conn, query, converted_params)
    } else {
        // --- non-transactional path: use the pooled persistent connection ---
        let conn_arc = connections.inner().get_conn(db_alias)?;
        let conn = lock_mutex(&conn_arc, "ConnectionManager")?;

        query_rows(&conn, query, converted_params)
    }
}

fn query_rows(
    conn: &Connection,
    query: &str,
    params: Vec<Box<dyn rusqlite::ToSql>>,
) -> Result<Vec<IndexMap<String, JsonValue>>, crate::Error> {
    let mut stmt = conn.prepare(query).map_err(Error::Rusqlite)?;
    let col_names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let mut rows = stmt
        .query(rusqlite::params_from_iter(params))
        .map_err(Error::Rusqlite)?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().map_err(Error::Rusqlite)? {
        let mut row_map = IndexMap::new();
        for (i, col_name) in col_names.iter().enumerate() {
            let value_ref = row.get_ref(i).map_err(Error::Rusqlite)?;
            let value_json = convert::rusqlite_value_to_json(value_ref)?;
            row_map.insert(col_name.clone(), value_json);
        }
        results.push(row_map);
    }
    Ok(results)
}

/// Execute a command against the database
/// db is the database in sqlite:xyz.db
/// Migrate both up and down using the migration version number
#[command]
pub(crate) fn migrate<R: Runtime>(
    app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    version: usize,
    db: &str,
) -> Result<(), crate::Error> {
    let db_info = connections
        .inner()
        .connections
        .0
        .lock()
        .unwrap()
        .get(db)
        .cloned()
        .ok_or_else(|| Error::DatabaseNotLoaded(db.to_string()))?;

    // Migrations need exclusive access, so use a fresh dedicated connection
    let mut conn = open_configured_conn(&db_info)?;

    let migration_list = app.state::<Mutex<MigrationList>>();
    let mig_list = lock_mutex(&migration_list, "MigrationManager")?;

    let resolved_migrations = mig_list.clone().resolve();
    let migrations = RusqliteMigrations::new(resolved_migrations);

    let _ = migrations.to_version(&mut conn, version);

    conn.close().map_err(|(_, e)| {
        Error::ConnectionFailed(
            db_info.path.display().to_string(),
            format!("Failed to close migration connection: {}", e),
        )
    })?;

    // Evict the pool connection so the next query sees the migrated schema
    lock_mutex(&connections.inner().pool.0, "ConnectionManager")?.remove(db);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionManager, ConnectionPool, TransactionManager};
    use serde_json::json;
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tauri::Manager;

    const MEMORY_DB_ALIAS: &str = "sqlite:::memory:";

    fn setup_test_app() -> tauri::App<MockRuntime> {
        let assets = noop_assets();
        let context = mock_context(assets);
        let app = mock_builder()
            .build(context)
            .expect("Failed to build mock app");
        let handle = app.handle().clone();
        app.manage(Mutex::new(MigrationList::default()));
        app.manage(Rusqlite2Connections {
            app: handle,
            connections: ConnectionManager::default(),
            pool: ConnectionPool::default(),
            transactions: TransactionManager::default(),
        });
        app
    }

    fn load_memory_db(app: &tauri::App<MockRuntime>) -> String {
        load(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            MEMORY_DB_ALIAS,
            Vec::new(),
        )
        .expect("Failed to load in-memory database")
    }

    #[test]
    fn load_and_close_memory_db() {
        let app = setup_test_app();
        let db_alias = load_memory_db(&app);

        {
            let connections = app.state::<Rusqlite2Connections<MockRuntime>>();
            let map = connections.connections.0.lock().unwrap();
            assert!(map.contains_key(&db_alias));
        }

        let closed = close(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            Some(db_alias.clone()),
        )
        .expect("Close should succeed");
        assert!(closed);

        let connections = app.state::<Rusqlite2Connections<MockRuntime>>();
        let map = connections.connections.0.lock().unwrap();
        assert!(!map.contains_key(&db_alias));
    }

    #[test]
    fn execute_non_transactional_memory_db() {
        let app = setup_test_app();
        let db_alias = load_memory_db(&app);

        let result = execute(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &db_alias,
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            Vec::new(),
            None,
        );
        assert!(result.is_ok(), "Non-TX execute failed: {:?}", result.err());
    }

    #[test]
    fn transaction_execute_select_commit_memory_db() {
        let app = setup_test_app();
        let db_alias = load_memory_db(&app);

        let tx_id = begin_transaction(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &db_alias,
        )
        .expect("Begin transaction should succeed with empty pass");

        execute(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &db_alias,
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)",
            Vec::new(),
            Some(tx_id.clone()),
        )
        .expect("Create table failed");

        let (changes, _) = execute(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &db_alias,
            "INSERT INTO users (name) VALUES (?)",
            vec![json!("Alice")],
            Some(tx_id.clone()),
        )
        .expect("Insert failed");
        assert_eq!(changes, 1);

        let rows = select(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &db_alias,
            "SELECT id, name FROM users WHERE name = ?",
            vec![json!("Alice")],
            Some(tx_id.clone()),
        )
        .expect("Select failed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("name"), Some(&json!("Alice")));

        commit_transaction(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &tx_id,
        )
        .expect("Commit should succeed");

        let uuid = Uuid::from_str(&tx_id).expect("Invalid tx id");
        let connections = app.state::<Rusqlite2Connections<MockRuntime>>();
        let tx_map = connections.transactions.0.lock().unwrap();
        assert!(!tx_map.contains_key(&uuid));
    }

    #[test]
    fn rollback_transaction_memory_db() {
        let app = setup_test_app();
        let db_alias = load_memory_db(&app);

        let tx_id = begin_transaction(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &db_alias,
        )
        .expect("Begin transaction should succeed");

        rollback_transaction(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            &tx_id,
        )
        .expect("Rollback should succeed");

        let uuid = Uuid::from_str(&tx_id).expect("Invalid tx id");
        let connections = app.state::<Rusqlite2Connections<MockRuntime>>();
        let tx_map = connections.transactions.0.lock().unwrap();
        assert!(!tx_map.contains_key(&uuid));
    }

    #[test]
    fn migrate_memory_db() {
        let app = setup_test_app();
        let db_alias = load_memory_db(&app);

        migrate(
            app.handle().clone(),
            app.state::<Rusqlite2Connections<MockRuntime>>(),
            0,
            &db_alias,
        )
        .expect("Migrate should succeed with empty migration list");
    }
}
