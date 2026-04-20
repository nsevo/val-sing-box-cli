use std::path::PathBuf;

use super::detect::OsFamily;

const APP_NAME: &str = "val-sing-box-cli";

/// Filesystem layout for valsb.
///
/// valsb is a root-only tool: every install lives under well-known system
/// paths so that there is exactly one source of truth on each machine. The
/// only exception is `--config-dir <path>`, which redirects state into a
/// custom base directory (used by tests and by anyone who wants to inspect
/// state without root).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub sing_box_bin_dir: PathBuf,
    pub unit_file: PathBuf,
}

impl AppPaths {
    /// Resolve the canonical layout for the host OS, optionally rooted at a
    /// caller-provided base directory (for `--config-dir` overrides).
    pub fn resolve(os_family: OsFamily, override_config_dir: Option<&str>) -> Self {
        let defaults = Self::system(os_family);
        match override_config_dir {
            Some(dir) => Self::from_custom_base(PathBuf::from(dir), defaults),
            None => defaults,
        }
    }

    fn system(os_family: OsFamily) -> Self {
        match os_family {
            OsFamily::Linux => Self::linux(),
            OsFamily::MacOS => Self::macos(),
            OsFamily::OpenWrt => Self::openwrt(),
            OsFamily::Windows => Self::windows(),
        }
    }

    fn linux() -> Self {
        Self {
            config_dir: PathBuf::from("/etc").join(APP_NAME),
            cache_dir: PathBuf::from("/var/cache").join(APP_NAME),
            data_dir: PathBuf::from("/var/lib").join(APP_NAME),
            bin_dir: PathBuf::from("/usr/local/bin"),
            sing_box_bin_dir: PathBuf::from("/usr/local/lib").join(APP_NAME).join("bin"),
            unit_file: PathBuf::from("/etc/systemd/system/valsb-sing-box.service"),
        }
    }

    fn macos() -> Self {
        Self {
            config_dir: PathBuf::from("/etc").join(APP_NAME),
            cache_dir: PathBuf::from("/var/cache").join(APP_NAME),
            data_dir: PathBuf::from("/var/lib").join(APP_NAME),
            bin_dir: PathBuf::from("/usr/local/bin"),
            sing_box_bin_dir: PathBuf::from("/usr/local/lib").join(APP_NAME).join("bin"),
            unit_file: PathBuf::from("/Library/LaunchDaemons/com.valsb.sing-box.plist"),
        }
    }

    fn openwrt() -> Self {
        Self {
            config_dir: PathBuf::from("/etc").join(APP_NAME),
            cache_dir: PathBuf::from("/var/cache").join(APP_NAME),
            data_dir: PathBuf::from("/var/lib").join(APP_NAME),
            bin_dir: PathBuf::from("/usr/bin"),
            sing_box_bin_dir: PathBuf::from("/usr/lib").join(APP_NAME).join("bin"),
            unit_file: PathBuf::from("/etc/init.d/valsb-sing-box"),
        }
    }

    fn windows() -> Self {
        let program_data = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        let program_files = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        let base = program_data.join(APP_NAME);
        let bin_dir = program_files.join(APP_NAME);
        Self {
            cache_dir: base.join("cache"),
            data_dir: base.clone(),
            bin_dir: bin_dir.clone(),
            sing_box_bin_dir: bin_dir,
            config_dir: base,
            unit_file: PathBuf::new(),
        }
    }

    fn from_custom_base(base: PathBuf, defaults: Self) -> Self {
        let data_dir = base.join("data");
        let sing_box_bin_dir = data_dir.join("bin");
        Self {
            config_dir: base.clone(),
            cache_dir: base.join("cache"),
            data_dir,
            sing_box_bin_dir,
            ..defaults
        }
    }

    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    pub fn manifest_file(&self) -> PathBuf {
        self.data_dir.join("manifest.json")
    }

    pub fn generated_config_file(&self) -> PathBuf {
        self.config_dir.join("sing-box.json")
    }

    pub fn subscription_cache_dir(&self) -> PathBuf {
        self.cache_dir.join("subscriptions")
    }

    pub fn sing_box_binary(&self) -> PathBuf {
        self.sing_box_bin_dir
            .join(format!("sing-box{}", std::env::consts::EXE_SUFFIX))
    }

    pub fn valsb_binary(&self) -> PathBuf {
        self.bin_dir
            .join(format!("valsb{}", std::env::consts::EXE_SUFFIX))
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [
            &self.config_dir,
            &self.cache_dir,
            &self.data_dir,
            &self.sing_box_bin_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::create_dir_all(self.subscription_cache_dir())?;
        Ok(())
    }
}
