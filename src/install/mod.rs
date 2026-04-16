mod manifest;

pub use manifest::{DelegatedControl, InstallScope, ManagedPaths, Manifest};
use std::path::Path;

use crate::errors::{AppError, AppResult};

pub fn load_manifest(path: &Path) -> AppResult<Option<Manifest>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)?;
    let manifest: Manifest = serde_json::from_str(&content)?;

    if manifest.schema_version > 2 {
        return Err(AppError::data_with_hint(
            format!(
                "manifest uses schema version {}, but this version only supports up to 2",
                manifest.schema_version
            ),
            "upgrade valsb to a newer version",
        ));
    }

    Ok(Some(manifest))
}

pub fn save_manifest(path: &Path, manifest: &Manifest) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(manifest)?;
    let dir = path
        .parent()
        .ok_or_else(|| AppError::runtime("cannot determine parent directory for manifest write"))?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, content.as_bytes())?;
    tmp.persist(path).map_err(|e| AppError::Runtime {
        message: format!("failed to persist manifest: {e}"),
        hint: Some("check filesystem permissions".into()),
    })?;
    Ok(())
}
