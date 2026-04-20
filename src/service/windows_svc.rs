use std::process::Command;

use crate::errors::{AppError, AppResult};

use super::{ServiceManager, ServiceStatus};

const SERVICE_NAME: &str = "valsb-sing-box";
const DISPLAY_NAME: &str = "sing-box managed by valsb";

pub struct WindowsServiceManager {
    config_path: String,
    sing_box_bin: String,
    data_dir: String,
}

impl WindowsServiceManager {
    pub fn new(config_path: &str, sing_box_bin: &str, data_dir: &str) -> Self {
        Self {
            config_path: config_path.to_string(),
            sing_box_bin: sing_box_bin.to_string(),
            data_dir: data_dir.to_string(),
        }
    }

    fn log_dir(&self) -> String {
        format!("{}\\logs", self.data_dir)
    }

    fn valsb_exe() -> String {
        std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    fn service_bin_path(&self) -> String {
        let valsb_exe = Self::valsb_exe();
        format!(
            "\"{valsb_exe}\" service-worker --sing-box-bin \"{sing_box}\" --config \"{config}\" --log-dir \"{log_dir}\"",
            sing_box = self.sing_box_bin,
            config = self.config_path,
            log_dir = self.log_dir(),
        )
    }
}

fn sc(args: &[&str]) -> AppResult<std::process::Output> {
    Command::new("sc.exe")
        .args(args)
        .output()
        .map_err(|e| AppError::runtime(format!("failed to run sc.exe: {e}")))
}

fn parse_sc_query() -> AppResult<(bool, String, Option<u32>)> {
    let output = sc(&["query", SERVICE_NAME])?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    if !output.status.success() {
        return Ok((false, "not installed".to_string(), None));
    }

    let state = stdout
        .lines()
        .find(|l| l.contains("STATE"))
        .and_then(|l| l.split_whitespace().last())
        .unwrap_or("UNKNOWN")
        .to_string();

    let pid = stdout
        .lines()
        .find(|l| l.contains("PID"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&p| p > 0);

    let active = state == "RUNNING";
    Ok((active, state, pid))
}

fn sc_create_args(bin_path: &str) -> Vec<String> {
    vec![
        "create".to_string(),
        SERVICE_NAME.to_string(),
        "binPath=".to_string(),
        bin_path.to_string(),
        "displayname=".to_string(),
        DISPLAY_NAME.to_string(),
        "start=".to_string(),
        "auto".to_string(),
    ]
}

impl ServiceManager for WindowsServiceManager {
    fn install(&self) -> AppResult<()> {
        let log_dir = self.log_dir();
        std::fs::create_dir_all(&log_dir)?;

        let bin_path = self.service_bin_path();
        let _ = sc(&["stop", SERVICE_NAME]);
        let _ = sc(&["delete", SERVICE_NAME]);
        std::thread::sleep(std::time::Duration::from_secs(1));

        let args = sc_create_args(&bin_path);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = sc(&arg_refs)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AppError::runtime(format!(
                "failed to create service: {} {}",
                stdout.trim(),
                stderr.trim()
            )));
        }

        let _ = sc(&[
            "description",
            SERVICE_NAME,
            "sing-box proxy service managed by valsb",
        ]);

        Ok(())
    }

    fn uninstall(&self) -> AppResult<()> {
        let _ = self.stop();
        let output = sc(&["delete", SERVICE_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AppError::runtime(format!(
                "failed to delete service: {} {}",
                stdout.trim(),
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn start(&self) -> AppResult<()> {
        let output = sc(&["start", SERVICE_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AppError::runtime_with_hint(
                format!(
                    "failed to start service: {} {}",
                    stdout.trim(),
                    stderr.trim()
                ),
                "run `valsb doctor` to check environment, ensure running as Administrator",
            ));
        }
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        let output = sc(&["stop", SERVICE_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime(format!(
                "failed to stop service: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn restart(&self) -> AppResult<()> {
        let _ = self.stop();
        std::thread::sleep(std::time::Duration::from_secs(1));
        self.start()
    }

    fn reload(&self) -> AppResult<()> {
        self.restart()
    }

    fn status(&self) -> AppResult<ServiceStatus> {
        let (active, state, pid) = parse_sc_query()?;
        Ok(ServiceStatus {
            active,
            state,
            main_pid: pid,
            unit_file: None,
        })
    }

    fn logs(&self, _follow: bool, lines: u32) -> AppResult<()> {
        let log_path = format!("{}\\sing-box.stderr.log", self.log_dir());
        if !std::path::Path::new(&log_path).exists() {
            println!("No log file found at {log_path}");
            return Ok(());
        }

        let content = std::fs::read_to_string(&log_path)?;
        let all_lines: Vec<&str> = content.lines().collect();
        let start = all_lines.len().saturating_sub(lines as usize);
        for line in &all_lines[start..] {
            println!("{line}");
        }
        Ok(())
    }

    fn is_active(&self) -> AppResult<bool> {
        let (active, _, _) = parse_sc_query()?;
        Ok(active)
    }

    fn backend_name(&self) -> &'static str {
        "Windows Service"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sc_create_args_match_documented_sc_syntax() {
        let args = sc_create_args(r#""C:\valsb.exe" service-worker --config "C:\cfg.json""#);
        assert_eq!(args[0], "create");
        assert_eq!(args[1], SERVICE_NAME);
        assert_eq!(args[2], "binPath=");
        assert!(args[3].contains("service-worker"));
        assert_eq!(args[4], "displayname=");
        assert_eq!(args[5], DISPLAY_NAME);
        assert_eq!(args[6], "start=");
        assert_eq!(args[7], "auto");
    }
}

/// Entry point called by the Windows SCM. This function blocks until
/// the service is stopped.
#[cfg(windows)]
pub fn run_service_worker(
    sing_box_bin: &str,
    config_path: &str,
    log_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState,
        ServiceStatus as WinServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::{define_windows_service, service_dispatcher};

    static WORKER_ARGS: std::sync::OnceLock<(String, String, String)> = std::sync::OnceLock::new();

    WORKER_ARGS.get_or_init(|| {
        (
            sing_box_bin.to_string(),
            config_path.to_string(),
            log_dir.to_string(),
        )
    });

    define_windows_service!(ffi_service_main, service_main);

    fn service_main(_args: Vec<OsString>) {
        let (sing_box_bin, config_path, log_dir) = WORKER_ARGS.get().unwrap();
        if let Err(e) = run_service_inner(sing_box_bin, config_path, log_dir) {
            eprintln!("service error: {e}");
        }
    }

    fn run_service_inner(
        sing_box_bin: &str,
        config_path: &str,
        log_dir: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let shutdown_tx = Mutex::new(Some(shutdown_tx));

        let status_handle =
            service_control_handler::register(SERVICE_NAME, move |event| match event {
                ServiceControl::Stop => {
                    if let Some(tx) = shutdown_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                _ => ServiceControlHandlerResult::NotImplemented,
            })?;

        status_handle.set_service_status(WinServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StartPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        })?;

        let _ = std::fs::create_dir_all(log_dir);
        let stdout_log = std::fs::File::create(format!("{log_dir}\\sing-box.stdout.log"))?;
        let stderr_log = std::fs::File::create(format!("{log_dir}\\sing-box.stderr.log"))?;

        let mut child = Command::new(sing_box_bin)
            .args(["-c", config_path, "run"])
            .stdout(stdout_log)
            .stderr(stderr_log)
            .spawn()
            .map_err(|e| format!("failed to spawn sing-box: {e}"))?;

        status_handle.set_service_status(WinServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        loop {
            if shutdown_rx.try_recv().is_ok() {
                break;
            }
            if let Some(exit) = child.try_wait()? {
                eprintln!("sing-box exited unexpectedly: {exit}");
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        status_handle.set_service_status(WinServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::StopPending,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 1,
            wait_hint: Duration::from_secs(5),
            process_id: None,
        })?;

        let _ = child.kill();
        let _ = child.wait();

        status_handle.set_service_status(WinServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }

    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}
