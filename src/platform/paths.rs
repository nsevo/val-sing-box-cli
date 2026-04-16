use std::path::PathBuf;

use super::detect::OsFamily;

const APP_NAME: &str = "val-sing-box-cli";

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
    pub fn resolve(os_family: OsFamily, is_root: bool, override_config_dir: Option<&str>) -> Self {
        if let Some(dir) = override_config_dir {
            let base = PathBuf::from(dir);
            return Self::from_custom_base(base, os_family, is_root);
        }

        match os_family {
            OsFamily::OpenWrt => Self::openwrt(),
            OsFamily::Linux if is_root => Self::unix_root(),
            OsFamily::MacOS if is_root => Self::macos_root(),
            OsFamily::Linux | OsFamily::MacOS | OsFamily::Windows => Self::user_dirs(os_family),
        }
    }

    /// User-level paths. Works on Linux, macOS, and Windows via the `dirs` crate
    /// which resolves to platform-native directories automatically.
    fn user_dirs(os_family: OsFamily) -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| home().join(".config"))
            .join(APP_NAME);
        let (cache_dir, data_dir, bin_dir, sing_box_bin_dir) =
            if matches!(os_family, OsFamily::Windows) {
                let data_dir = config_dir.clone();
                let bin_dir = data_dir.join("bin");
                let cache_dir = data_dir.join("cache");
                let sing_box_bin_dir = bin_dir.clone();
                (cache_dir, data_dir, bin_dir, sing_box_bin_dir)
            } else {
                let cache_dir = dirs::cache_dir()
                    .unwrap_or_else(|| home().join(".cache"))
                    .join(APP_NAME);
                let data_dir = dirs::data_dir()
                    .unwrap_or_else(|| home().join(".local/share"))
                    .join(APP_NAME);
                let bin_dir = home().join(".local/bin");
                let sing_box_bin_dir = data_dir.join("bin");
                (cache_dir, data_dir, bin_dir, sing_box_bin_dir)
            };
        let unit_file = resolve_user_unit_file(os_family);

        Self {
            config_dir,
            cache_dir,
            data_dir,
            bin_dir,
            sing_box_bin_dir,
            unit_file,
        }
    }

    fn unix_root() -> Self {
        Self {
            config_dir: PathBuf::from("/etc").join(APP_NAME),
            cache_dir: PathBuf::from("/var/cache").join(APP_NAME),
            data_dir: PathBuf::from("/var/lib").join(APP_NAME),
            bin_dir: PathBuf::from("/usr/local/bin"),
            sing_box_bin_dir: PathBuf::from("/usr/local/lib").join(APP_NAME).join("bin"),
            unit_file: PathBuf::from("/etc/systemd/system/valsb-sing-box.service"),
        }
    }

    fn macos_root() -> Self {
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

    fn from_custom_base(base: PathBuf, os_family: OsFamily, is_root: bool) -> Self {
        let defaults = match os_family {
            OsFamily::OpenWrt => Self::openwrt(),
            OsFamily::Linux if is_root => Self::unix_root(),
            OsFamily::MacOS if is_root => Self::macos_root(),
            OsFamily::Linux | OsFamily::MacOS | OsFamily::Windows => Self::user_dirs(os_family),
        };
        let data_dir = base.join("data");
        let sing_box_bin_dir = data_dir.join("bin");
        Self {
            config_dir: base.clone(),
            data_dir,
            cache_dir: base.join("cache"),
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
        let sub_cache = self.subscription_cache_dir();
        std::fs::create_dir_all(&sub_cache)?;
        Ok(())
    }
}

fn resolve_user_unit_file(os_family: OsFamily) -> PathBuf {
    match os_family {
        OsFamily::MacOS => home().join("Library/LaunchAgents/com.valsb.sing-box.plist"),
        OsFamily::Windows => PathBuf::new(),
        _ => dirs::config_dir()
            .unwrap_or_else(|| home().join(".config"))
            .join("systemd/user/valsb-sing-box.service"),
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"))
}
