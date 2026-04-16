use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OsFamily {
    Linux,
    OpenWrt,
    MacOS,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    Amd64,
    Arm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceBackend {
    SystemdUser,
    SystemdSystem,
    Procd,
    LaunchdAgent,
    LaunchdDaemon,
    WindowsService,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Platform {
    pub os_family: OsFamily,
    pub arch: Arch,
    pub available_backends: Vec<ServiceBackend>,
    pub is_root: bool,
    pub uid: u32,
    pub username: String,
}

impl Platform {
    pub fn detect() -> Result<Self, String> {
        let os_family = detect_os_family();
        let arch = detect_arch()?;
        let (uid, is_root) = detect_uid_and_root();
        let username = detect_username();
        let available_backends = detect_backends(os_family, is_root);

        Ok(Self {
            os_family,
            arch,
            available_backends,
            is_root,
            uid,
            username,
        })
    }

    pub fn default_backend(&self, tun_mode: bool) -> Option<ServiceBackend> {
        match self.os_family {
            OsFamily::OpenWrt => {
                if self.available_backends.contains(&ServiceBackend::Procd) {
                    Some(ServiceBackend::Procd)
                } else {
                    None
                }
            }
            OsFamily::Linux => {
                if tun_mode {
                    if self
                        .available_backends
                        .contains(&ServiceBackend::SystemdSystem)
                    {
                        Some(ServiceBackend::SystemdSystem)
                    } else {
                        None
                    }
                } else if self
                    .available_backends
                    .contains(&ServiceBackend::SystemdUser)
                {
                    Some(ServiceBackend::SystemdUser)
                } else if self
                    .available_backends
                    .contains(&ServiceBackend::SystemdSystem)
                {
                    Some(ServiceBackend::SystemdSystem)
                } else {
                    None
                }
            }
            OsFamily::MacOS => {
                if self.is_root {
                    Some(ServiceBackend::LaunchdDaemon)
                } else {
                    Some(ServiceBackend::LaunchdAgent)
                }
            }
            OsFamily::Windows => {
                if self
                    .available_backends
                    .contains(&ServiceBackend::WindowsService)
                {
                    Some(ServiceBackend::WindowsService)
                } else {
                    None
                }
            }
        }
    }

    pub fn has_tun_device() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::path::Path::new("/dev/net/tun").exists()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub fn sing_box_path() -> Option<String> {
        which_cmd(sing_box_bin_name())
    }
}

pub fn sing_box_bin_name() -> &'static str {
    if cfg!(windows) {
        "sing-box.exe"
    } else {
        "sing-box"
    }
}

fn detect_os_family() -> OsFamily {
    if cfg!(target_os = "windows") {
        OsFamily::Windows
    } else if cfg!(target_os = "macos") {
        OsFamily::MacOS
    } else if std::path::Path::new("/etc/openwrt_release").exists() {
        OsFamily::OpenWrt
    } else {
        OsFamily::Linux
    }
}

fn detect_arch() -> Result<Arch, String> {
    let raw = std::env::consts::ARCH;
    match raw {
        "x86_64" => Ok(Arch::Amd64),
        "aarch64" => Ok(Arch::Arm64),
        other => Err(format!("unsupported architecture: {other}")),
    }
}

#[cfg(unix)]
fn detect_uid_and_root() -> (u32, bool) {
    let uid = unsafe { libc::geteuid() };
    (uid, uid == 0)
}

#[cfg(windows)]
fn detect_uid_and_root() -> (u32, bool) {
    (0, false)
}

fn detect_username() -> String {
    #[cfg(windows)]
    {
        std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
    }
}

fn detect_backends(os_family: OsFamily, _is_root: bool) -> Vec<ServiceBackend> {
    let mut backends = Vec::new();

    match os_family {
        OsFamily::OpenWrt => {
            if which_cmd("procd").is_some() || std::path::Path::new("/sbin/procd").exists() {
                backends.push(ServiceBackend::Procd);
            }
        }
        OsFamily::Linux => {
            if systemctl_available() {
                backends.push(ServiceBackend::SystemdSystem);
                if has_user_session() {
                    backends.push(ServiceBackend::SystemdUser);
                }
            }
        }
        OsFamily::MacOS => {
            backends.push(ServiceBackend::LaunchdAgent);
            backends.push(ServiceBackend::LaunchdDaemon);
        }
        OsFamily::Windows => {
            backends.push(ServiceBackend::WindowsService);
        }
    }

    backends
}

fn systemctl_available() -> bool {
    Command::new("systemctl")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn has_user_session() -> bool {
    std::env::var("XDG_RUNTIME_DIR").is_ok()
}

fn which_cmd(name: &str) -> Option<String> {
    let cmd = if cfg!(windows) { "where.exe" } else { "which" };
    Command::new(cmd)
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
}
