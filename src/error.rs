// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Rusqlite(#[from] rusqlite::Error),
    #[error("invalid database url format: {0}")]
    InvalidDatabaseUrl(String),
    #[error("database alias \"{0}\" not loaded. Make sure you have called `load` for this alias.")]
    DatabaseNotLoaded(String),
    #[error("database type \"{0}\" is not supported. Only 'sqlite' is supported.")]
    UnsupportedDatabaseType(String),
    #[error("failed to resolve application path")]
    CannotResolvePath,
    #[error(
        "transaction with id \"{0}\" not found. It may have already been committed or rolled back."
    )]
    TransactionNotFound(String),
    #[error("invalid transaction id format: {0}")]
    InvalidUuid(String),
    #[error("failed to connect to database: {0} ({1})")]
    ConnectionFailed(String, String),
    #[error("error converting value: {0}")]
    ValueConversionError(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Extension load error: {0}")]
    ExtensionLoadFailed(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
