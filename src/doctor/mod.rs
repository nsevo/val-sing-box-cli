use serde::Serialize;
use std::path::Path;

use crate::platform::{AppPaths, Platform};

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct DoctorReport {
    pub user: UserInfo,
    pub system: SystemInfo,
    pub sing_box: SingBoxInfo,
    pub paths: PathsInfo,
    pub tun: TunInfo,
    pub checks: Vec<Check>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub uid: u32,
    pub is_root: bool,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct SystemInfo {
    pub os_family: String,
    pub arch: String,
    pub available_backends: Vec<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct SingBoxInfo {
    pub installed: bool,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct PathsInfo {
    pub config_dir: PathCheck,
    pub data_dir: PathCheck,
    pub cache_dir: PathCheck,
    pub generated_config: PathCheck,
    pub state_file: PathCheck,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct PathCheck {
    pub path: String,
    pub exists: bool,
    pub writable: bool,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct TunInfo {
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

impl DoctorReport {
    pub fn run(platform: &Platform, paths: &AppPaths) -> Self {
        let mut checks = Vec::new();

        let user = UserInfo {
            username: platform.username.clone(),
            uid: platform.uid,
            is_root: platform.is_root,
        };

        let system = SystemInfo {
            os_family: format!("{:?}", platform.os_family),
            arch: format!("{:?}", platform.arch),
            available_backends: platform
                .available_backends
                .iter()
                .map(|b| format!("{b:?}"))
                .collect(),
        };

        let sb_path = Platform::sing_box_path();
        let sing_box = SingBoxInfo {
            installed: sb_path.is_some(),
            path: sb_path.clone(),
        };

        if sing_box.installed {
            checks.push(Check {
                name: "sing-box".into(),
                status: CheckStatus::Ok,
                message: format!("found at {}", sb_path.as_deref().unwrap_or("unknown")),
            });
        } else {
            checks.push(Check {
                name: "sing-box".into(),
                status: CheckStatus::Error,
                message: "not found in PATH or managed path".into(),
            });
        }

        build_backend_checks(platform, &mut checks);

        let path_checks = PathsInfo {
            config_dir: check_path(&paths.config_dir),
            data_dir: check_path(&paths.data_dir),
            cache_dir: check_path(&paths.cache_dir),
            generated_config: check_path_file(&paths.generated_config_file()),
            state_file: check_path_file(&paths.state_file()),
        };

        let tun = build_tun_info(platform, &mut checks);

        Self {
            user,
            system,
            sing_box,
            paths: path_checks,
            tun,
            checks,
        }
    }

    pub fn print_human(&self) {
        use crate::ui;

        ui::print_header("Environment");
        ui::print_ok(&format!(
            "User       {} (uid: {})",
            self.user.username, self.user.uid
        ));
        ui::print_ok(&format!(
            "System     {} / {}",
            self.system.os_family, self.system.arch
        ));
        ui::print_ok(&format!(
            "Backends   {}",
            self.system.available_backends.join(", ")
        ));
        println!();

        ui::print_header("Kernel");
        if self.sing_box.installed {
            ui::print_ok(&format!(
                "sing-box found at {}",
                self.sing_box.path.as_deref().unwrap_or("?")
            ));
        } else {
            ui::print_fail("sing-box not found");
            ui::print_hint("run: valsb install");
        }
        println!();

        ui::print_header("Paths");
        print_path_check_tagged("Config", &self.paths.config_dir);
        print_path_check_tagged("Data", &self.paths.data_dir);
        print_path_check_tagged("Cache", &self.paths.cache_dir);
        print_path_check_tagged("Active config", &self.paths.generated_config);
        print_path_check_tagged("State", &self.paths.state_file);
        println!();

        ui::print_header("TUN");
        if self.tun.available {
            ui::print_ok(&self.tun.detail);
        } else {
            ui::print_warn(&self.tun.detail);
        }
        println!();

        ui::print_header("Checks");
        for check in &self.checks {
            let msg = format!("{}: {}", check.name, check.message);
            match check.status {
                CheckStatus::Ok => ui::print_ok(&msg),
                CheckStatus::Warning => ui::print_warn(&msg),
                CheckStatus::Error => ui::print_fail(&msg),
            }
        }
    }
}

fn build_backend_checks(platform: &Platform, checks: &mut Vec<Check>) {
    if platform.available_backends.is_empty() {
        checks.push(Check {
            name: "service backend".into(),
            status: CheckStatus::Warning,
            message: "no service backend detected".into(),
        });
        return;
    }

    for backend in &platform.available_backends {
        checks.push(Check {
            name: format!("{backend:?}"),
            status: CheckStatus::Ok,
            message: "available".into(),
        });
    }
}

fn build_tun_info(platform: &Platform, checks: &mut Vec<Check>) -> TunInfo {
    match platform.os_family {
        crate::platform::OsFamily::Linux | crate::platform::OsFamily::OpenWrt => {
            let device_exists = Platform::has_tun_device();
            let available = device_exists && platform.is_root;

            if device_exists {
                checks.push(Check {
                    name: "/dev/net/tun".into(),
                    status: CheckStatus::Ok,
                    message: "exists".into(),
                });
            } else {
                checks.push(Check {
                    name: "/dev/net/tun".into(),
                    status: CheckStatus::Warning,
                    message: "not found, TUN mode will not work".into(),
                });
            }

            if device_exists && !platform.is_root {
                checks.push(Check {
                    name: "TUN capability".into(),
                    status: CheckStatus::Warning,
                    message: "not running as root, TUN mode requires root or CAP_NET_ADMIN".into(),
                });
            }

            TunInfo {
                available,
                detail: if available {
                    "root with /dev/net/tun".to_string()
                } else if device_exists {
                    "device exists but not root".to_string()
                } else {
                    "/dev/net/tun not found".to_string()
                },
            }
        }
        crate::platform::OsFamily::MacOS => {
            let available = platform.is_root;
            checks.push(Check {
                name: "TUN".into(),
                status: if available {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Warning
                },
                message: if available {
                    "macOS utun available (root)".into()
                } else {
                    "TUN mode requires root on macOS".into()
                },
            });
            TunInfo {
                available,
                detail: "macOS uses utun interfaces".to_string(),
            }
        }
        crate::platform::OsFamily::Windows => {
            checks.push(Check {
                name: "TUN".into(),
                status: CheckStatus::Ok,
                message: "Windows TUN adapter managed by sing-box (wintun)".into(),
            });
            TunInfo {
                available: true,
                detail: "wintun driver".to_string(),
            }
        }
    }
}

fn check_path(path: &Path) -> PathCheck {
    let exists = path.exists();
    let writable = if exists {
        test_writable(path)
    } else if let Some(parent) = path.parent() {
        parent.exists() && test_writable(parent)
    } else {
        false
    };

    PathCheck {
        path: path.to_string_lossy().into_owned(),
        exists,
        writable,
    }
}

fn check_path_file(path: &Path) -> PathCheck {
    let exists = path.exists();
    let writable = if let Some(parent) = path.parent() {
        parent.exists() && test_writable(parent)
    } else {
        false
    };

    PathCheck {
        path: path.to_string_lossy().into_owned(),
        exists,
        writable,
    }
}

fn test_writable(path: &Path) -> bool {
    let test_file = path.join(".valsb_write_test");
    if std::fs::write(&test_file, b"test").is_ok() {
        let _ = std::fs::remove_file(&test_file);
        true
    } else {
        false
    }
}

fn print_path_check_tagged(label: &str, check: &PathCheck) {
    use crate::ui;
    let detail = format!("{label:<14} {}", check.path);
    if check.exists && check.writable {
        ui::print_ok(&detail);
    } else if check.exists {
        ui::print_warn(&format!("{detail} (read-only)"));
    } else {
        ui::print_warn(&format!("{detail} (missing)"));
    }
}
