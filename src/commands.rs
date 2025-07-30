// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use indexmap::IndexMap;
use rusqlite_migration::Migrations as RusqliteMigrations;
use serde_json::Value as JsonValue;
use tauri::Manager;
use tauri::{command, AppHandle, Runtime, State};

// Updated imports
use crate::{convert, DbInfo, Error, LastInsertId, MigrationList, Rusqlite2Connections}; // Removed DbInfo
use rusqlite::Connection; // Removed params_from_iter, Statement
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex}; // Added missing import
use std::time::Duration;
use uuid::Uuid;

#[command]
pub(crate) fn get_conn_url<R: Runtime>(
    app: AppHandle<R>,
    db: String,
) -> Result<PathBuf, crate::Error> {
    let (kind, path_part) = db
        .split_once(':')
        .ok_or_else(|| Error::InvalidDatabaseUrl(db.clone()))?;

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
    Connection::open(&path)
        .map_err(|e| Error::ConnectionFailed(path.display().to_string(), e.to_string()))?
        .close()
        .map_err(|(_, e)| {
            Error::ConnectionFailed(
                path.display().to_string(),
                format!("Failed to close test connection: {}", e),
            )
        })?;

    Ok(path)
}

// Refactored load command
#[command]
pub(crate) fn load<R: Runtime>(
    app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db: String,
) -> Result<String, crate::Error> {
    let (kind, path_part) = db
        .split_once(':')
        .ok_or_else(|| Error::InvalidDatabaseUrl(db.clone()))?;

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
    Connection::open(&path)
        .map_err(|e| Error::ConnectionFailed(path.display().to_string(), e.to_string()))?
        .close()
        .map_err(|(_, e)| {
            Error::ConnectionFailed(
                path.display().to_string(),
                format!("Failed to close test connection: {}", e),
            )
        })?;

    // Store DbInfo (path) in the manager
    let db_info = DbInfo { path };
    let mut connection_map = connections.inner().connections.0.lock().unwrap();
    if connection_map.contains_key(&db) {
        log::warn!(
            "Database alias '{}' already loaded. Overwriting previous info.",
            db
        );
    }
    connection_map.insert(db.clone(), db_info);

    Ok(db)
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
    let mut connection_map = connections.inner().connections.0.lock().unwrap();

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
        connection_map.remove(&alias);
        // Remove the alias from the connection manager.
        // Note: This does not affect active transactions associated with this alias.
        // Active transactions hold their own connection Arc and will continue until
        // commit or rollback. The connection is closed when the Arc count drops to 0.
        // Attempting to start *new* operations (load, execute, select, begin_transaction)
        // with this alias will fail until it is loaded again.
    }

    Ok(true)
}

// --- Transaction Commands --- Implementation ---

#[command]
pub(crate) fn begin_transaction<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db_alias: String,
) -> Result<String, crate::Error> {
    // Get DbInfo from ConnectionManager
    let db_info = connections
        .inner()
        .connections
        .0
        .lock()
        .unwrap()
        .get(&db_alias)
        .cloned()
        .ok_or_else(|| Error::DatabaseNotLoaded(db_alias.clone()))?;

    // Open a *new* connection specifically for this transaction
    let tx_conn = Connection::open(&db_info.path)
        .map_err(|e| Error::ConnectionFailed(db_info.path.display().to_string(), e.to_string()))?;

    // Set busy timeout for this transaction's connection
    tx_conn
        .busy_timeout(Duration::from_millis(5000))
        .map_err(Error::Rusqlite)?;

    // Begin the transaction on the new connection
    // Use IMMEDIATE (default behavior, allows concurrent reads until first write)
    tx_conn
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(Error::Rusqlite)?;

    // Generate ID and store the new connection (wrapped in Arc<Mutex<_>>) in TransactionManager
    let tx_id = Uuid::new_v4();
    let tx_conn_arc = Arc::new(Mutex::new(tx_conn));

    connections
        .inner()
        .transactions
        .0
        .lock()
        .unwrap()
        .insert(tx_id, tx_conn_arc);

    Ok(tx_id.to_string())
}

#[command]
pub(crate) fn commit_transaction<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    tx_id: String,
) -> Result<(), crate::Error> {
    let uuid = Uuid::from_str(&tx_id).map_err(|_| Error::InvalidUuid(tx_id.clone()))?;

    // Ensure correct State access
    let maybe_conn = connections
        .inner()
        .transactions
        .0
        .lock()
        .unwrap()
        .remove(&uuid);

    if let Some(arc_mutex_conn) = maybe_conn {
        let conn_guard = arc_mutex_conn.lock().unwrap();
        conn_guard
            .execute_batch("COMMIT")
            .map_err(Error::Rusqlite)?;
        Ok(())
    } else {
        Err(Error::TransactionNotFound(tx_id))
    }
}

#[command]
pub(crate) fn rollback_transaction<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    tx_id: String,
) -> Result<(), crate::Error> {
    let uuid = Uuid::from_str(&tx_id).map_err(|_| Error::InvalidUuid(tx_id.clone()))?;

    // Ensure correct State access
    let maybe_conn = connections
        .inner()
        .transactions
        .0
        .lock()
        .unwrap()
        .remove(&uuid);

    if let Some(arc_mutex_conn) = maybe_conn {
        let conn_guard = arc_mutex_conn.lock().unwrap();
        // Log rollback errors but don't propagate them as the transaction state is cleared anyway
        if let Err(e) = conn_guard.execute_batch("ROLLBACK") {
            log::error!("Error rolling back transaction {}: {}", tx_id, e);
        }
        Ok(())
    } else {
        Err(Error::TransactionNotFound(tx_id))
    }
}

// --- Existing Commands to be Refactored (Step 6 & 7) ---

/// Execute a command against the database
#[command]
pub(crate) fn execute<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db_alias: String,
    query: String,
    values: Vec<JsonValue>,
    tx_id: Option<String>,
) -> Result<(u64, LastInsertId), crate::Error> {
    let converted_params = convert::json_to_rusqlite_params(values)?;

    if let Some(tx_id_str) = tx_id {
        // Transactional execution
        let uuid = Uuid::from_str(&tx_id_str).map_err(|_| Error::InvalidUuid(tx_id_str.clone()))?;
        let tx_map = connections.inner().transactions.0.lock().unwrap();
        let conn_arc = tx_map
            .get(&uuid)
            .cloned()
            .ok_or_else(|| Error::TransactionNotFound(tx_id_str))?;

        // Lock the connection and execute
        let conn_guard = conn_arc.lock().unwrap();
        let changes = conn_guard
            .execute(&query, rusqlite::params_from_iter(converted_params))
            .map_err(Error::Rusqlite)?; // Keep TX open on error
        let last_id = conn_guard.last_insert_rowid();
        Ok((changes as u64, LastInsertId::Sqlite(last_id)))
    } else {
        // Non-transactional execution (open, execute, close)
        let db_info = connections
            .inner()
            .connections
            .0
            .lock()
            .unwrap()
            .get(&db_alias)
            .cloned()
            .ok_or_else(|| Error::DatabaseNotLoaded(db_alias.clone()))?;

        let conn = Connection::open(&db_info.path).map_err(|e| {
            Error::ConnectionFailed(db_info.path.display().to_string(), e.to_string())
        })?;
        let changes = conn
            .execute(&query, rusqlite::params_from_iter(converted_params))
            .map_err(Error::Rusqlite)?; // Error during non-TX execute
        let last_id = conn.last_insert_rowid();
        conn.close().map_err(|(_, e)| {
            Error::ConnectionFailed(
                db_info.path.display().to_string(),
                format!("Failed to close connection after non-TX execute: {}", e),
            )
        })?;
        Ok((changes as u64, LastInsertId::Sqlite(last_id)))
    }
}

#[command]
pub(crate) fn select<R: Runtime>(
    _app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    db_alias: String,
    query: String,
    values: Vec<JsonValue>,
    tx_id: Option<String>,
) -> Result<Vec<IndexMap<String, JsonValue>>, crate::Error> {
    let converted_params = convert::json_to_rusqlite_params(values)?;

    if let Some(tx_id_str) = tx_id {
        // Transactional select
        let uuid = Uuid::from_str(&tx_id_str).map_err(|_| Error::InvalidUuid(tx_id_str.clone()))?;
        let tx_map = connections.inner().transactions.0.lock().unwrap();
        let conn_arc = tx_map
            .get(&uuid)
            .cloned()
            .ok_or_else(|| Error::TransactionNotFound(tx_id_str))?;

        // Lock the connection and execute select
        let conn_guard = conn_arc.lock().unwrap();
        let mut stmt = conn_guard.prepare(&query).map_err(Error::Rusqlite)?;
        let col_names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
        let mut rows = stmt
            .query(rusqlite::params_from_iter(converted_params))
            .map_err(Error::Rusqlite)?;

        let mut result_vec = Vec::new();
        while let Some(row) = rows.next().map_err(Error::Rusqlite)? {
            let mut row_map = IndexMap::new();
            for (i, col_name) in col_names.iter().enumerate() {
                let value_ref = row.get_ref(i).map_err(Error::Rusqlite)?;
                let value_json = convert::rusqlite_value_to_json(value_ref)?;
                row_map.insert(col_name.clone(), value_json);
            }
            result_vec.push(row_map);
        }
        Ok(result_vec)
    } else {
        // Non-transactional select (open, select, close)
        let db_info = connections
            .inner()
            .connections
            .0
            .lock()
            .unwrap()
            .get(&db_alias)
            .cloned()
            .ok_or_else(|| Error::DatabaseNotLoaded(db_alias.clone()))?;

        let conn = Connection::open(&db_info.path).map_err(|e| {
            Error::ConnectionFailed(db_info.path.display().to_string(), e.to_string())
        })?;

        let result_vec = {
            // Create a block to scope stmt and rows
            let mut stmt = conn.prepare(&query).map_err(Error::Rusqlite)?;
            let col_names: Vec<String> =
                stmt.column_names().into_iter().map(String::from).collect();
            let mut rows = stmt
                .query(rusqlite::params_from_iter(converted_params))
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
            results // Return results from the block
        }; // stmt and rows are dropped here

        conn.close().map_err(|(_, e)| {
            Error::ConnectionFailed(
                db_info.path.display().to_string(),
                format!("Failed to close connection after non-TX select: {}", e),
            )
        })?;
        Ok(result_vec)
    }
}

/// Execute a command against the database
/// db is the database in sqlite:xyz.db
/// Migrate both up and down using the migration version number
#[command]
pub(crate) fn migrate<R: Runtime>(
    app: AppHandle<R>,
    connections: State<'_, Rusqlite2Connections<R>>,
    version: usize,
    db: String,
) -> Result<(), crate::Error> {
    let db_info = connections
        .inner()
        .connections
        .0
        .lock()
        .unwrap()
        .get(&db)
        .cloned()
        .ok_or_else(|| Error::DatabaseNotLoaded(db.clone()))?;

    let mut conn = Connection::open(&db_info.path)
        .map_err(|e| Error::ConnectionFailed(db_info.path.display().to_string(), e.to_string()))?;

    let migration_list = app.state::<Mutex<MigrationList>>();
    let mig_list = migration_list.lock().unwrap();

    let resolved_migrations = mig_list.clone().resolve();
    let migrations = RusqliteMigrations::new(resolved_migrations);

    let _ = migrations.to_version(&mut conn, version);

    conn.close().map_err(|(_, e)| {
        Error::ConnectionFailed(
            db_info.path.display().to_string(),
            format!("MDQ0NVDT9BZGG: Failed to close connection.{}", e),
        )
    })?;

    Ok(())
}
