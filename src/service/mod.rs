#[cfg(target_os = "macos")]
mod launchd;
#[cfg(target_os = "linux")]
mod procd;
#[cfg(target_os = "linux")]
mod systemd;
#[cfg(windows)]
pub mod windows_svc;

use crate::errors::AppResult;
use crate::platform::ServiceBackend;

#[cfg(target_os = "macos")]
pub use self::launchd::{LaunchdAgentManager, LaunchdDaemonManager};
#[cfg(target_os = "linux")]
pub use self::procd::ProcdManager;
#[cfg(target_os = "linux")]
pub use self::systemd::{SystemdSystemManager, SystemdUserManager, configure_system_delegate};
#[cfg(windows)]
pub use self::windows_svc::{WindowsServiceManager, configure_service_delegate};

pub trait ServiceManager {
    fn install(&self) -> AppResult<()>;
    fn uninstall(&self) -> AppResult<()>;
    fn start(&self) -> AppResult<()>;
    fn stop(&self) -> AppResult<()>;
    fn restart(&self) -> AppResult<()>;
    fn reload(&self) -> AppResult<()>;
    fn status(&self) -> AppResult<ServiceStatus>;
    fn logs(&self, follow: bool, lines: u32) -> AppResult<()>;
    fn is_active(&self) -> AppResult<bool>;
    fn backend_name(&self) -> &'static str;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceStatus {
    pub active: bool,
    pub state: String,
    pub main_pid: Option<u32>,
    pub unit_file: Option<String>,
}

pub fn create_manager(
    backend: ServiceBackend,
    #[cfg_attr(windows, allow(unused_variables))] unit_file: &str,
    config_path: &str,
    sing_box_bin: &str,
    data_dir: &str,
) -> Box<dyn ServiceManager> {
    match backend {
        #[cfg(target_os = "linux")]
        ServiceBackend::SystemdUser => Box::new(SystemdUserManager::new(
            unit_file,
            config_path,
            sing_box_bin,
            data_dir,
        )),
        #[cfg(target_os = "linux")]
        ServiceBackend::SystemdSystem => Box::new(SystemdSystemManager::new(
            unit_file,
            config_path,
            sing_box_bin,
            data_dir,
        )),
        #[cfg(target_os = "linux")]
        ServiceBackend::Procd => Box::new(ProcdManager::new(
            unit_file,
            config_path,
            sing_box_bin,
            data_dir,
        )),
        #[cfg(target_os = "macos")]
        ServiceBackend::LaunchdAgent => Box::new(LaunchdAgentManager::new(
            unit_file,
            config_path,
            sing_box_bin,
            data_dir,
        )),
        #[cfg(target_os = "macos")]
        ServiceBackend::LaunchdDaemon => Box::new(LaunchdDaemonManager::new(
            unit_file,
            config_path,
            sing_box_bin,
            data_dir,
        )),
        #[cfg(windows)]
        ServiceBackend::WindowsService => Box::new(WindowsServiceManager::new(
            config_path,
            sing_box_bin,
            data_dir,
        )),
        #[allow(unreachable_patterns)]
        _ => unreachable!("backend {backend:?} not available on this platform"),
    }
}
