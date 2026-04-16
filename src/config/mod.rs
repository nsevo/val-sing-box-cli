use crate::errors::{AppError, AppResult};
use crate::platform::AppPaths;

pub fn init_config(paths: &AppPaths) -> AppResult<()> {
    paths.ensure_dirs().map_err(|e| {
        AppError::env_with_hint(
            format!("failed to create config directories: {e}"),
            "check filesystem permissions",
        )
    })?;

    Ok(())
}

pub fn check_generated_config_exists(paths: &AppPaths) -> AppResult<()> {
    let config_path = paths.generated_config_file();
    if !config_path.exists() {
        return Err(AppError::user_with_hint(
            "no config found",
            "run `valsb sub add <url>` to add a subscription",
        ));
    }
    Ok(())
}

/// Save the raw sing-box JSON config for a profile.
pub fn save_raw_config(
    paths: &AppPaths,
    profile_id: &str,
    config: &serde_json::Value,
) -> AppResult<()> {
    let cache_dir = paths.subscription_cache_dir();
    std::fs::create_dir_all(&cache_dir)?;

    let cache_file = cache_dir.join(format!("{profile_id}.json"));
    let content = serde_json::to_string_pretty(config)?;

    let mut tmp = tempfile::NamedTempFile::new_in(&cache_dir)?;
    std::io::Write::write_all(&mut tmp, content.as_bytes())?;
    tmp.persist(&cache_file).map_err(|e| AppError::Runtime {
        message: format!("failed to persist raw config: {e}"),
        hint: Some("check filesystem permissions".into()),
    })?;

    Ok(())
}

/// Load the raw sing-box JSON config for a profile.
pub fn read_raw_config(paths: &AppPaths, profile_id: &str) -> AppResult<serde_json::Value> {
    let cache_file = paths
        .subscription_cache_dir()
        .join(format!("{profile_id}.json"));
    if !cache_file.exists() {
        return Err(AppError::data_with_hint(
            format!("no cached config for profile {profile_id}"),
            "run `valsb sub update` to fetch subscription data",
        ));
    }

    let content = std::fs::read_to_string(&cache_file)?;
    let config: serde_json::Value = serde_json::from_str(&content)?;
    Ok(config)
}

/// Write a sing-box config to the active config path (atomic).
pub fn write_active_config(paths: &AppPaths, config: &serde_json::Value) -> AppResult<()> {
    let config_path = paths.generated_config_file();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(config)?;

    let dir = config_path
        .parent()
        .ok_or_else(|| AppError::runtime("cannot determine parent directory for config write"))?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    std::io::Write::write_all(&mut tmp, content.as_bytes())?;
    tmp.persist(&config_path).map_err(|e| AppError::Runtime {
        message: format!("failed to persist config: {e}"),
        hint: Some("check filesystem permissions".into()),
    })?;

    Ok(())
}

/// Validate a config file using the sing-box binary.
pub fn validate_config(
    sing_box_bin: &std::path::Path,
    config_path: &std::path::Path,
) -> AppResult<()> {
    if !sing_box_bin.exists() {
        return Err(AppError::env_with_hint(
            "sing-box binary not found",
            format!("expected at: {}", sing_box_bin.display()),
        ));
    }

    let output = std::process::Command::new(sing_box_bin)
        .args(["check", "-c"])
        .arg(config_path)
        .output()
        .map_err(|e| AppError::runtime(format!("failed to run sing-box check: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::data_with_hint(
            format!("config validation failed: {}", stderr.trim()),
            "check your subscription provider's config format",
        ));
    }

    Ok(())
}
