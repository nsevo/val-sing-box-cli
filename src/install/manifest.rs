use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::platform::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallScope {
    User,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatedControl {
    pub mode: String,
    pub principal: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub installed_at: DateTime<Utc>,
    pub valsb_version: String,
    pub sing_box_version: Option<String>,
    #[serde(default = "default_install_scope")]
    pub install_scope: InstallScope,
    #[serde(default)]
    pub delegated_control: Option<DelegatedControl>,
    pub managed_paths: ManagedPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedPaths {
    pub valsb_bin: String,
    pub sing_box_bin: Option<String>,
    pub config_dir: String,
    pub cache_dir: String,
    pub data_dir: String,
    pub unit_file: Option<String>,
}

impl Manifest {
    pub fn new(
        paths: &AppPaths,
        sing_box_version: Option<String>,
        install_scope: InstallScope,
        delegated_control: Option<DelegatedControl>,
    ) -> Self {
        Self {
            schema_version: 2,
            installed_at: Utc::now(),
            valsb_version: env!("CARGO_PKG_VERSION").to_string(),
            sing_box_version,
            install_scope,
            delegated_control,
            managed_paths: ManagedPaths {
                valsb_bin: paths.valsb_binary().to_string_lossy().into_owned(),
                sing_box_bin: Some(paths.sing_box_binary().to_string_lossy().into_owned()),
                config_dir: paths.config_dir.to_string_lossy().into_owned(),
                cache_dir: paths.cache_dir.to_string_lossy().into_owned(),
                data_dir: paths.data_dir.to_string_lossy().into_owned(),
                unit_file: Some(paths.unit_file.to_string_lossy().into_owned()),
            },
        }
    }
}

fn default_install_scope() -> InstallScope {
    InstallScope::User
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_manifest_defaults_to_user_scope() {
        let json = r#"{
            "schema_version": 1,
            "installed_at": "2026-04-15T00:00:00Z",
            "valsb_version": "0.2.4",
            "sing_box_version": "1.13.8",
            "managed_paths": {
                "valsb_bin": "/usr/local/bin/valsb",
                "sing_box_bin": "/usr/local/lib/val-sing-box-cli/bin/sing-box",
                "config_dir": "/etc/val-sing-box-cli",
                "cache_dir": "/var/cache/val-sing-box-cli",
                "data_dir": "/var/lib/val-sing-box-cli",
                "unit_file": "/etc/systemd/system/valsb-sing-box.service"
            }
        }"#;

        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.install_scope, InstallScope::User);
        assert!(manifest.delegated_control.is_none());
    }
}
