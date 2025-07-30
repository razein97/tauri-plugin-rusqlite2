use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Rusqlite;
#[cfg(mobile)]
use mobile::Rusqlite;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the rusqlite APIs.
pub trait RusqliteExt<R: Runtime> {
  fn rusqlite(&self) -> &Rusqlite<R>;
}

impl<R: Runtime, T: Manager<R>> crate::RusqliteExt<R> for T {
  fn rusqlite(&self) -> &Rusqlite<R> {
    self.state::<Rusqlite<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("rusqlite")
    .invoke_handler(tauri::generate_handler![commands::ping])
    .setup(|app, api| {
      #[cfg(mobile)]
      let rusqlite = mobile::init(app, api)?;
      #[cfg(desktop)]
      let rusqlite = desktop::init(app, api)?;
      app.manage(rusqlite);
      Ok(())
    })
    .build()
}
