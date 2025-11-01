use tauri_plugin_rusqlite2::{Migration, MigrationKind};

pub const CREATE_INITIAL_DATA: Migration = Migration {
    version: 1,
    description: "create_initial_data",
    sql: r#"
    CREATE TABLE accounts (name text,balance integer,id integer);
    INSERT INTO accounts ("name","balance","id") VALUES ('Bob',10000,1),('Alicia',10000,2);
    "#,
    kind: MigrationKind::Up,
    down_sql: "DROP TABLE accounts;",
};
