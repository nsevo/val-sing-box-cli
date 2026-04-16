mod schema;

pub use schema::{AppState, Profile, RemarkSource, UpdateStatus};
use std::path::Path;

use crate::errors::{AppError, AppResult};

const CURRENT_SCHEMA_VERSION: u32 = 2;

pub fn load_state(path: &Path) -> AppResult<AppState> {
    if !path.exists() {
        return Ok(AppState::default());
    }

    let content = std::fs::read_to_string(path)?;
    let state: AppState = serde_json::from_str(&content)?;

    if state.schema_version > CURRENT_SCHEMA_VERSION {
        return Err(AppError::data_with_hint(
            format!(
                "state file uses schema version {}, but this version of valsb only supports up to {}",
                state.schema_version, CURRENT_SCHEMA_VERSION
            ),
            "upgrade valsb to a newer version",
        ));
    }

    Ok(state)
}

pub fn save_state(path: &Path, state: &AppState) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(state)?;
    atomic_write(path, content.as_bytes())
}

fn atomic_write(path: &Path, data: &[u8]) -> AppResult<()> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::runtime("cannot determine parent directory for atomic write"))?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, data)?;
    tmp.persist(path).map_err(|e| AppError::Runtime {
        message: format!("failed to persist temp file: {e}"),
        hint: Some("check filesystem permissions".into()),
    })?;
    Ok(())
}
