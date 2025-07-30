// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! Interface with SQLite databases using rusqlite.

#![doc(
    html_logo_url = "https://github.com/tauri-apps/tauri/raw/dev/app-icon.png",
    html_favicon_url = "https://github.com/tauri-apps/tauri/raw/dev/app-icon.png"
)]

mod commands;
mod convert; // Added module
mod error;
use futures_core::future::BoxFuture;
use rusqlite::Connection;
use rusqlite_migration::{Migrations as RusqliteMigrations, M};

use std::collections::HashMap;
use std::path::PathBuf; // Added import
use std::sync::{Arc, Mutex};
use uuid::Uuid; // Added

pub use error::Error;

use serde::{Deserialize, Serialize}; // Adjusted imports

use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    Manager, Runtime,
};

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum LastInsertId {
    #[cfg(feature = "sqlite")]
    Sqlite(i64),
    #[cfg(not(feature = "sqlite"))]
    None,
}

struct Migrations(Mutex<HashMap<String, MigrationList>>);

#[derive(Default, Clone, Deserialize)]
pub struct PluginConfig {
    #[serde(default)]
    preload: Vec<String>,
}

#[derive(Debug)]
pub enum MigrationKind {
    Up,
    Down,
}

// impl From<MigrationKind> for MigrationType<'_> {
//     fn from(kind: MigrationKind) -> Self {
//         match kind {
//             MigrationKind::Up => Self::ReversibleUp,
//             MigrationKind::Down => Self::ReversibleDown,
//         }
//     }
// }

/// A migration definition.
#[derive(Debug)]
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub sql: &'static str,
    pub down_sql: &'static str,
    pub kind: MigrationKind,
}

#[derive(Debug)]
struct MigrationList(Vec<Migration>);

impl MigrationList {
    pub fn resolve(self) -> Vec<M<'static>> {
        let mut migrations = Vec::new();
        for migration in self.0 {
            migrations.push(M::up(migration.sql).down(migration.down_sql));
        }

        migrations
    }
}

// --- New State Definitions ---

// Reintroduce DbInfo
#[derive(Clone, Debug)] // Removed Send + Sync from derive
struct DbInfo {
    path: PathBuf,
}

#[derive(Default, Clone)]
// Revert ConnectionManager to hold DbInfo
pub(crate) struct ConnectionManager(pub Arc<Mutex<HashMap<String, DbInfo>>>);

#[derive(Default, Clone)]
pub(crate) struct TransactionManager(
    pub Arc<Mutex<HashMap<Uuid, Arc<Mutex<rusqlite::Connection>>>>>,
);

/// Allows blocking on async code without creating a nested runtime.
fn run_async_command<F: std::future::Future>(cmd: F) -> F::Output {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(cmd))
    } else {
        tauri::async_runtime::block_on(cmd)
    }
}

/// Tauri SQL plugin builder.
#[derive(Default)]
pub struct Builder {
    migrations: Option<HashMap<String, MigrationList>>,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add migrations to a database.
    #[must_use]
    pub fn add_migrations(mut self, db_url: &str, migrations: Vec<Migration>) -> Self {
        self.migrations
            .get_or_insert(Default::default())
            .insert(db_url.to_string(), MigrationList(migrations));
        self
    }

    pub fn build<R: Runtime>(mut self) -> TauriPlugin<R, Option<PluginConfig>> {
        PluginBuilder::<R, Option<PluginConfig>>::new("sql")
            .invoke_handler(tauri::generate_handler![
                commands::load,
                commands::execute,
                commands::select,
                commands::close,
                // Added new transaction commands
                commands::begin_transaction,
                commands::commit_transaction,
                commands::rollback_transaction
            ])
            .setup(|app, api| {
                let config = api.config().clone().unwrap_or_default();

                run_async_command(async move {
                    for db in config.preload {
                        let mut conn = Connection::open(&db).unwrap();
                        if let Some(migrations) =
                            self.migrations.as_mut().and_then(|mm| mm.remove(&db))
                        {
                            let resolved_migrations = migrations.resolve();
                            let migrations = RusqliteMigrations::new(resolved_migrations);

                            migrations.to_latest(&mut conn).unwrap();
                        }
                    }
                    // Register new states
                    app.manage(ConnectionManager::default());
                    app.manage(TransactionManager::default());

                    Ok(())
                })
            })
            .on_event(|_app, _event| {})
            .build()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        commands, Builder as SqlBuilder, ConnectionManager, Error, LastInsertId, TransactionManager,
    };
    use serde_json::{json, Value as JsonValue};
    use tauri::{
        test::{mock_builder, mock_context, noop_assets, MockRuntime},
        AppHandle, Manager,
    };
    use tempfile::tempdir;

    // Updated test setup helper
    fn setup_test_environment() -> (
        AppHandle<MockRuntime>,
        ConnectionManager,
        TransactionManager,
    ) {
        let assets = noop_assets();
        let context = mock_context(assets);
        let app = mock_builder()
            .plugin(SqlBuilder::new().build()) // Keep plugin registered
            .build(context)
            .expect("Failed to build mock app");
        let handle = app.handle().clone();
        let connection_manager = ConnectionManager::default();
        let transaction_manager = TransactionManager::default();
        app.manage(connection_manager.clone());
        app.manage(transaction_manager.clone());
        (handle, connection_manager, transaction_manager)
    }

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_load_in_memory() {
        let (app_handle, _original_connection_manager, _transaction_manager) =
            setup_test_environment();
        let db_alias = "sqlite::memory:".to_string();

        // Call command directly, getting State from AppHandle
        let result = commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        );

        assert!(result.is_ok(), "load failed: {:?}", result.err());
        let returned_alias = result.unwrap();
        assert_eq!(returned_alias, db_alias);

        // Re-fetch state from app_handle to check modifications
        let final_state = app_handle.state::<ConnectionManager>();
        let manager_map = final_state.inner().0.lock().unwrap();
        assert!(manager_map.contains_key(&returned_alias));
    }

    #[test]
    fn test_load_file_db() {
        let (app_handle, _original_connection_manager, _transaction_manager) =
            setup_test_environment();
        let _temp_dir = tempdir().expect("Failed to create temp dir for test");
        let db_relative_path = "test_db.sqlite";
        let db_alias = format!("sqlite:{}", db_relative_path);

        // Call command directly, getting State from AppHandle
        let result = commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        );

        assert!(result.is_ok(), "load failed: {:?}", result.err());
        let returned_alias = result.unwrap();
        assert_eq!(returned_alias, db_alias);

        // Re-fetch state from app_handle to check modifications
        let final_state = app_handle.state::<ConnectionManager>();
        let manager_map = final_state.inner().0.lock().unwrap();
        assert!(manager_map.contains_key(&returned_alias));

        // Check file creation
        let test_app_data_dir = app_handle
            .path()
            .app_data_dir()
            .expect("Failed to get app data dir for test");
        let resolved_expected_path = test_app_data_dir.join(db_relative_path);
        assert!(resolved_expected_path.exists());
        assert!(resolved_expected_path.is_file());
    }

    #[test]
    fn test_basic_execute_select() {
        let (app_handle, _connection_manager, _transaction_manager) = setup_test_environment();
        let db_alias = "sqlite::memory:".to_string();

        // Load DB
        commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        )
        .expect("Failed to load test DB");

        // --- Perform all operations within a single transaction for consistency ---
        let tx_id = commands::begin_transaction(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
        )
        .expect("Begin transaction failed for test setup");
        let tx_id_opt = Some(tx_id.clone());

        // Create table within TX
        let create_table_sql =
            "CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)"
                .to_string();
        let create_result = commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            create_table_sql,
            vec![],
            tx_id_opt.clone(), // Use TX ID
        );
        assert!(
            create_result.is_ok(),
            "Create table failed: {:?}",
            create_result.err()
        );

        // Insert data within TX
        let insert_sql = "INSERT INTO users (name) VALUES (?)".to_string();
        let insert_params = vec![JsonValue::String("Alice".to_string())];
        let insert_result = commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql,
            insert_params,
            tx_id_opt.clone(), // Use TX ID
        );
        assert!(
            insert_result.is_ok(),
            "Insert failed: {:?}",
            insert_result.err()
        );
        let (rows_affected, last_insert_id) = insert_result.unwrap();
        assert_eq!(rows_affected, 1);
        match last_insert_id {
            LastInsertId::Sqlite(id) => assert_eq!(id, 1),
            #[allow(unreachable_patterns)]
            _ => panic!("Unexpected LastInsertId variant"),
        }

        // Select data within TX
        let select_sql = "SELECT id, name FROM users WHERE name = ?".to_string();
        let select_params = vec![JsonValue::String("Alice".to_string())];
        let select_result = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql,
            select_params,
            tx_id_opt.clone(), // Use TX ID
        );
        assert!(
            select_result.is_ok(),
            "Select failed: {:?}",
            select_result.err()
        );

        let selected_data = select_result.unwrap();
        assert_eq!(selected_data.len(), 1);
        let user_row = &selected_data[0];

        let mut expected_row = indexmap::IndexMap::new();
        expected_row.insert("id".to_string(), json!(1));
        expected_row.insert("name".to_string(), json!("Alice"));

        assert_eq!(user_row, &expected_row);

        // Commit the transaction (clean up)
        commands::commit_transaction(app_handle.state::<TransactionManager>(), tx_id)
            .expect("Commit failed for test cleanup");
    }

    #[test]
    fn test_transaction_commit() {
        let (app_handle, _connection_manager, _) = setup_test_environment();
        let temp_db_dir = tempdir().expect("Failed to create temp dir for commit test");
        let db_path = temp_db_dir.path().join("test_tx_commit.sqlite");
        let db_alias = format!("sqlite:{}", db_path.display());

        // Load DB
        commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        )
        .expect("Failed to load test DB");

        // --- Create table in a separate, committed transaction ---
        {
            let setup_tx_id = commands::begin_transaction(
                app_handle.state::<ConnectionManager>(),
                app_handle.state::<TransactionManager>(),
                db_alias.clone(),
            )
            .expect("Begin setup transaction failed");
            let create_table_sql =
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)".to_string();
            commands::execute(
                app_handle.state::<ConnectionManager>(),
                app_handle.state::<TransactionManager>(),
                db_alias.clone(),
                create_table_sql,
                vec![],
                Some(setup_tx_id.clone()),
            )
            .expect("Create table failed in setup transaction");
            commands::commit_transaction(app_handle.state::<TransactionManager>(), setup_tx_id)
                .expect("Commit setup transaction failed");
        }

        // --- Main Test Transaction ---
        let tx_id = commands::begin_transaction(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
        )
        .expect("Begin transaction failed");
        let tx_id_opt = Some(tx_id.clone());
        let tx_uuid = uuid::Uuid::parse_str(&tx_id).unwrap();

        // 1. Insert item 1 within transaction
        let insert_sql = "INSERT INTO items (id, name) VALUES (?, ?)".to_string();
        let insert_params_1 = vec![json!(1), json!("Item 1 Initial")];
        commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql.clone(),
            insert_params_1,
            tx_id_opt.clone(),
        )
        .expect("Insert 1 within TX failed");

        // 2. Insert item 2 within transaction
        let insert_params_2 = vec![json!(2), json!("Item 2")];
        commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql.clone(),
            insert_params_2,
            tx_id_opt.clone(),
        )
        .expect("Insert 2 within TX failed");

        // 3. Update item 1 within transaction
        let update_sql = "UPDATE items SET name = ? WHERE id = ?".to_string();
        let update_params_1 = vec![json!("Item 1 Updated"), json!(1)];
        commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            update_sql.clone(),
            update_params_1,
            tx_id_opt.clone(),
        )
        .expect("Update 1 within TX failed");

        // 4. Select item 1 inside transaction (verify update)
        let select_sql_1 = "SELECT name FROM items WHERE id = ?".to_string();
        let select_params_1 = vec![json!(1)];
        let select_inside_1 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_1.clone(),
            select_params_1.clone(),
            tx_id_opt.clone(),
        )
        .expect("Select 1 inside TX failed");
        assert_eq!(
            select_inside_1.len(),
            1,
            "Item 1 should be visible inside TX"
        );
        assert_eq!(
            select_inside_1[0].get("name").unwrap(),
            &json!("Item 1 Updated")
        );

        // 5. Select item 2 inside transaction (verify insert)
        let select_sql_2 = "SELECT name FROM items WHERE id = ?".to_string();
        let select_params_2 = vec![json!(2)];
        let select_inside_2 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_2.clone(),
            select_params_2.clone(),
            tx_id_opt.clone(),
        )
        .expect("Select 2 inside TX failed");
        assert_eq!(
            select_inside_2.len(),
            1,
            "Item 2 should be visible inside TX"
        );
        assert_eq!(select_inside_2[0].get("name").unwrap(), &json!("Item 2"));

        // 6. Select non-existent item inside transaction
        let select_params_3 = vec![json!(3)];
        let select_inside_3 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_1.clone(),
            select_params_3.clone(),
            tx_id_opt.clone(),
        )
        .expect("Select 3 inside TX failed");
        assert!(
            select_inside_3.is_empty(),
            "Item 3 should not exist inside TX"
        );

        // 7. Select item 1 outside transaction (should not see updated item yet)
        let select_outside_1 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_1.clone(),
            select_params_1.clone(),
            None, // No tx_id
        );
        assert!(
            select_outside_1.is_ok(),
            "Select 1 outside TX failed: {:?}",
            select_outside_1.err()
        );
        assert!(
            select_outside_1.unwrap().is_empty(),
            "Item 1 should not be visible outside TX before commit"
        );

        // 8. Commit transaction
        commands::commit_transaction(app_handle.state::<TransactionManager>(), tx_id.clone())
            .expect("Commit failed");

        // 9. Select item 1 outside transaction again (should see updated item now)
        let select_after_commit_1 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_1.clone(),
            select_params_1.clone(),
            None, // No tx_id
        );
        let data_after_commit_1 = select_after_commit_1.expect("Select 1 after commit failed");
        assert_eq!(
            data_after_commit_1.len(),
            1,
            "Item 1 should be visible outside TX after commit"
        );
        assert_eq!(
            data_after_commit_1[0].get("name").unwrap(),
            &json!("Item 1 Updated")
        );

        // 10. Select item 2 outside transaction again (should see inserted item now)
        let select_after_commit_2 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_2.clone(),
            select_params_2.clone(),
            None, // No tx_id
        );
        let data_after_commit_2 = select_after_commit_2.expect("Select 2 after commit failed");
        assert_eq!(
            data_after_commit_2.len(),
            1,
            "Item 2 should be visible outside TX after commit"
        );
        assert_eq!(
            data_after_commit_2[0].get("name").unwrap(),
            &json!("Item 2")
        );

        // Verify transaction ID is removed from manager
        {
            let current_transaction_manager = app_handle.state::<TransactionManager>();
            let tx_map = current_transaction_manager.0.lock().unwrap();
            assert!(
                !tx_map.contains_key(&tx_uuid),
                "Transaction ID should be removed after commit"
            );
        }
    }

    #[test]
    fn test_transaction_rollback() {
        let (app_handle, _connection_manager, _) = setup_test_environment();
        let temp_db_dir = tempdir().expect("Failed to create temp dir for rollback test");
        let db_path = temp_db_dir.path().join("test_tx_rollback.sqlite");
        let db_alias = format!("sqlite:{}", db_path.display());

        // Load DB & Create table
        commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        )
        .expect("Load failed");
        {
            let setup_tx_id = commands::begin_transaction(
                app_handle.state::<ConnectionManager>(),
                app_handle.state::<TransactionManager>(),
                db_alias.clone(),
            )
            .expect("Begin setup transaction failed");
            let create_sql = "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)".to_string();
            commands::execute(
                app_handle.state::<ConnectionManager>(),
                app_handle.state::<TransactionManager>(),
                db_alias.clone(),
                create_sql,
                vec![],
                Some(setup_tx_id.clone()),
            )
            .expect("Create failed in setup transaction");
            commands::commit_transaction(app_handle.state::<TransactionManager>(), setup_tx_id)
                .expect("Commit setup transaction failed");
        }

        // --- Main Test Transaction ---
        let tx_id = commands::begin_transaction(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
        )
        .expect("Begin failed");
        let tx_id_opt = Some(tx_id.clone());
        let tx_uuid = uuid::Uuid::parse_str(&tx_id).unwrap();

        // Insert item 1 within transaction
        let insert_sql = "INSERT INTO items (id, name) VALUES (?, ?)".to_string();
        let insert_params_1 = vec![json!(1), json!("Item R1")];
        commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql.clone(),
            insert_params_1,
            tx_id_opt.clone(),
        )
        .expect("Insert R1 within TX failed");

        // Insert item 2 within transaction
        let insert_params_2 = vec![json!(2), json!("Item R2")];
        commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql.clone(),
            insert_params_2,
            tx_id_opt.clone(),
        )
        .expect("Insert R2 within TX failed");

        // Select item 1 inside transaction (should see item)
        let select_sql_1 = "SELECT name FROM items WHERE id = ?".to_string();
        let select_params_1 = vec![json!(1)];
        let select_inside_1 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_1.clone(),
            select_params_1.clone(),
            tx_id_opt.clone(),
        );
        assert!(
            select_inside_1.is_ok(),
            "Select R1 inside TX failed: {:?}",
            select_inside_1.err()
        );
        assert_eq!(
            select_inside_1.unwrap().len(),
            1,
            "Item R1 should be visible inside TX before rollback"
        );

        // Select item 2 inside transaction (should see item)
        let select_sql_2 = "SELECT name FROM items WHERE id = ?".to_string();
        let select_params_2 = vec![json!(2)];
        let select_inside_2 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_2.clone(),
            select_params_2.clone(),
            tx_id_opt.clone(),
        );
        assert!(
            select_inside_2.is_ok(),
            "Select R2 inside TX failed: {:?}",
            select_inside_2.err()
        );
        assert_eq!(
            select_inside_2.unwrap().len(),
            1,
            "Item R2 should be visible inside TX before rollback"
        );

        // Rollback transaction
        commands::rollback_transaction(app_handle.state::<TransactionManager>(), tx_id.clone())
            .expect("Rollback failed");

        // Select item 1 outside transaction (should NOT see item)
        let select_after_rollback_1 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_1.clone(),
            select_params_1.clone(),
            None, // No tx_id
        );
        assert!(
            select_after_rollback_1.is_ok(),
            "Select R1 after rollback failed: {:?}",
            select_after_rollback_1.err()
        );
        assert!(
            select_after_rollback_1.unwrap().is_empty(),
            "Item R1 should NOT be visible outside TX after rollback"
        );

        // Select item 2 outside transaction (should NOT see item)
        let select_after_rollback_2 = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql_2.clone(),
            select_params_2.clone(),
            None, // No tx_id
        );
        assert!(
            select_after_rollback_2.is_ok(),
            "Select R2 after rollback failed: {:?}",
            select_after_rollback_2.err()
        );
        assert!(
            select_after_rollback_2.unwrap().is_empty(),
            "Item R2 should NOT be visible outside TX after rollback"
        );

        // Verify transaction ID is removed from manager
        {
            let current_transaction_manager = app_handle.state::<TransactionManager>();
            let tx_map = current_transaction_manager.0.lock().unwrap();
            assert!(
                !tx_map.contains_key(&tx_uuid),
                "Transaction ID should be removed after rollback"
            );
        }
    }

    #[test]
    fn test_transaction_error_rollback() {
        let (app_handle, _connection_manager, _) = setup_test_environment();
        let temp_db_dir = tempdir().expect("Failed to create temp dir for error rollback test");
        let db_path = temp_db_dir.path().join("test_tx_error_rollback.sqlite");
        // Use absolute path for the alias
        let db_alias = format!("sqlite:{}", db_path.display());

        // Load DB & Create table (using app_handle.state)
        commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        )
        .expect("Load failed");
        {
            let setup_tx_id = commands::begin_transaction(
                app_handle.state::<ConnectionManager>(),
                app_handle.state::<TransactionManager>(),
                db_alias.clone(),
            )
            .expect("Begin setup transaction failed");
            let create_sql =
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string();
            commands::execute(
                app_handle.state::<ConnectionManager>(),
                app_handle.state::<TransactionManager>(),
                db_alias.clone(),
                create_sql,
                vec![],
                Some(setup_tx_id.clone()),
            )
            .expect("Create failed in setup transaction");
            commands::commit_transaction(app_handle.state::<TransactionManager>(), setup_tx_id)
                .expect("Commit setup transaction failed");
        }

        // Begin main test transaction (using app_handle.state)
        let tx_id = commands::begin_transaction(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
        )
        .expect("Begin failed");
        let tx_id_opt = Some(tx_id.clone());
        let tx_uuid = uuid::Uuid::parse_str(&tx_id).unwrap();

        // 1. Insert valid data (using app_handle.state)
        let insert_sql = "INSERT INTO items (id, name) VALUES (?, ?)".to_string();
        let insert_params_1 = vec![json!(10), json!("Item E1")];
        let insert_result_1 = commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql.clone(),
            insert_params_1,
            tx_id_opt.clone(),
        );
        assert!(
            insert_result_1.is_ok(),
            "First insert in TX failed: {:?}",
            insert_result_1.err()
        );

        // 2. Attempt invalid insert (using app_handle.state)
        let insert_params_2 = vec![json!(10), json!("Item E2")];
        let insert_result_2 = commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            insert_sql.clone(),
            insert_params_2,
            tx_id_opt.clone(),
        );
        assert!(
            insert_result_2.is_err(),
            "Second (invalid) insert should fail"
        );
        match insert_result_2.err().unwrap() {
            Error::Rusqlite(e) => match e {
                rusqlite::Error::SqliteFailure(f, _) => {
                    assert_eq!(f.code, rusqlite::ErrorCode::ConstraintViolation)
                }
                _ => panic!("Expected SqliteFailure"),
            },
            _ => panic!("Expected Error::Rusqlite"),
        }

        // 3. Verify transaction ID still exists in manager (fetch state from app_handle)
        {
            let current_transaction_manager = app_handle.state::<TransactionManager>();
            let tx_map_before_rollback = current_transaction_manager.0.lock().unwrap();
            assert!(
                tx_map_before_rollback.contains_key(&tx_uuid),
                "Transaction ID should still exist after op error"
            );
        } // Release lock immediately

        // 4. Rollback transaction (using app_handle.state)
        let rollback_result =
            commands::rollback_transaction(app_handle.state::<TransactionManager>(), tx_id.clone());
        assert!(
            rollback_result.is_ok(),
            "Rollback failed: {:?}",
            rollback_result.err()
        );

        // 5. Select outside transaction (using app_handle.state)
        let select_sql = "SELECT name FROM items WHERE id = ?".to_string();
        let select_params = vec![json!(10)];
        let select_after_rollback_result = commands::select(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            select_sql.clone(),
            select_params.clone(),
            None,
        );
        assert!(select_after_rollback_result.is_ok());
        assert!(
            select_after_rollback_result.unwrap().is_empty(),
            "Item from first insert should NOT be visible after rollback"
        );

        // 6. Verify transaction ID is removed from manager (fetch state from app_handle)
        {
            let current_transaction_manager = app_handle.state::<TransactionManager>();
            let tx_map_after_rollback = current_transaction_manager.0.lock().unwrap();
            assert!(
                !tx_map_after_rollback.contains_key(&tx_uuid),
                "Transaction ID should be removed after rollback"
            );
        }
    }

    #[test]
    fn test_close_command() {
        let (app_handle, _, _) = setup_test_environment();
        let temp_db_dir = tempdir().expect("Failed to create temp dir for close test");
        let db_path = temp_db_dir.path().join("test_close.sqlite");
        let db_alias = format!("sqlite:{}", db_path.display());

        // 1. Load the database
        let load_result = commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias.clone(),
        );
        assert!(load_result.is_ok(), "Load failed: {:?}", load_result.err());
        assert_eq!(load_result.unwrap(), db_alias);

        // Verify it's in the manager
        {
            let connection_manager = app_handle.state::<ConnectionManager>();
            let conn_map = connection_manager.0.lock().unwrap();
            assert!(
                conn_map.contains_key(&db_alias),
                "DB alias should be loaded"
            );
        }

        // 2. Close the specific database alias
        let close_result = commands::close(
            app_handle.state::<ConnectionManager>(),
            Some(db_alias.clone()),
        );
        assert!(
            close_result.is_ok(),
            "Close failed: {:?}",
            close_result.err()
        );
        assert!(
            close_result.unwrap(),
            "Close should return true for known alias"
        );

        // Verify it's removed from the manager
        {
            let connection_manager = app_handle.state::<ConnectionManager>();
            let conn_map = connection_manager.0.lock().unwrap();
            assert!(
                !conn_map.contains_key(&db_alias),
                "DB alias should be removed after close"
            );
        }

        // 3. Attempt to execute a command using the closed alias (should fail)
        let execute_sql = "SELECT 1".to_string();
        let execute_result = commands::execute(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
            execute_sql,
            vec![],
            None, // Non-transactional
        );
        assert!(execute_result.is_err(), "Execute should fail after close");
        match execute_result.err().unwrap() {
            Error::DatabaseNotLoaded(alias) => assert_eq!(alias, db_alias),
            e => panic!("Expected DatabaseNotLoaded error, got {:?}", e),
        }

        // 4. Attempt to begin a transaction using the closed alias (should fail)
        let begin_tx_result = commands::begin_transaction(
            app_handle.state::<ConnectionManager>(),
            app_handle.state::<TransactionManager>(),
            db_alias.clone(),
        );
        assert!(
            begin_tx_result.is_err(),
            "Begin transaction should fail after close"
        );
        match begin_tx_result.err().unwrap() {
            Error::DatabaseNotLoaded(alias) => assert_eq!(alias, db_alias),
            e => panic!("Expected DatabaseNotLoaded error, got {:?}", e),
        }

        // 5. Test closing an unknown alias (should fail)
        let close_unknown_result = commands::close(
            app_handle.state::<ConnectionManager>(),
            Some("sqlite:nonexistent.db".to_string()),
        );
        assert!(
            close_unknown_result.is_err(),
            "Closing unknown alias should fail"
        );
        match close_unknown_result.err().unwrap() {
            Error::DatabaseNotLoaded(alias) => assert_eq!(alias, "sqlite:nonexistent.db"),
            e => panic!("Expected DatabaseNotLoaded error, got {:?}", e),
        }

        // 6. Load two databases, close all, verify both are gone
        let db_alias_1 = format!(
            "sqlite:{}",
            temp_db_dir.path().join("test_close_all_1.sqlite").display()
        );
        let db_alias_2 = format!(
            "sqlite:{}",
            temp_db_dir.path().join("test_close_all_2.sqlite").display()
        );
        commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias_1.clone(),
        )
        .expect("Load 1 failed");
        commands::load(
            app_handle.clone(),
            app_handle.state::<ConnectionManager>(),
            db_alias_2.clone(),
        )
        .expect("Load 2 failed");

        {
            // Verify both loaded
            let connection_manager = app_handle.state::<ConnectionManager>();
            let conn_map = connection_manager.0.lock().unwrap();
            assert!(
                conn_map.contains_key(&db_alias_1),
                "DB alias 1 should be loaded"
            );
            assert!(
                conn_map.contains_key(&db_alias_2),
                "DB alias 2 should be loaded"
            );
        }

        let close_all_result = commands::close(
            app_handle.state::<ConnectionManager>(),
            None, // Close all
        );
        assert!(
            close_all_result.is_ok(),
            "Close all failed: {:?}",
            close_all_result.err()
        );
        assert!(close_all_result.unwrap(), "Close all should return true");

        {
            // Verify both closed
            let connection_manager = app_handle.state::<ConnectionManager>();
            let conn_map = connection_manager.0.lock().unwrap();
            assert!(
                conn_map.is_empty(),
                "Connection map should be empty after close all"
            );
        }
    }

    // More tests will be added here...
}
