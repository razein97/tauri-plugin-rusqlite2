// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod migration;
use migration::CREATE_INITIAL_DATA;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_rusqlite2::Builder::default()
                .add_migrations("sqlite:pass:example.db", vec![CREATE_INITIAL_DATA])
                .build(),
        )
        // .invoke_handler()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
