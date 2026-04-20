use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::platform::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub installed_at: DateTime<Utc>,
    pub valsb_version: String,
    pub sing_box_version: Option<String>,
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
    pub fn new(paths: &AppPaths, sing_box_version: Option<String>) -> Self {
        Self {
            schema_version: 3,
            installed_at: Utc::now(),
            valsb_version: env!("CARGO_PKG_VERSION").to_string(),
            sing_box_version,
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
