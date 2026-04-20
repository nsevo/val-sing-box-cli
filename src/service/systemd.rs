use std::process::Command;

use crate::errors::{AppError, AppResult};

use super::{ServiceManager, ServiceStatus};

const UNIT_NAME: &str = "valsb-sing-box.service";

pub struct SystemdManager {
    unit_file: String,
    config_path: String,
    sing_box_bin: String,
    data_dir: String,
}

impl SystemdManager {
    pub fn new(unit_file: &str, config_path: &str, sing_box_bin: &str, data_dir: &str) -> Self {
        Self {
            unit_file: unit_file.to_string(),
            config_path: config_path.to_string(),
            sing_box_bin: sing_box_bin.to_string(),
            data_dir: data_dir.to_string(),
        }
    }

    fn unit_content(&self) -> String {
        let stdout_log = format!("{}/logs/sing-box.stdout.log", self.data_dir);
        let stderr_log = format!("{}/logs/sing-box.stderr.log", self.data_dir);
        format!(
            r"[Unit]
Description=sing-box managed by valsb
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={bin} -D {data} -c {config} run
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=5
LimitNOFILE=infinity
StandardOutput=append:{stdout_log}
StandardError=append:{stderr_log}

[Install]
WantedBy=multi-user.target
",
            bin = self.sing_box_bin,
            data = self.data_dir,
            config = self.config_path,
            stdout_log = stdout_log,
            stderr_log = stderr_log,
        )
    }

    fn log_dir(&self) -> String {
        format!("{}/logs", self.data_dir)
    }
}

fn systemctl(args: &[&str]) -> AppResult<std::process::Output> {
    Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| AppError::runtime(format!("failed to run systemctl: {e}")))
}

fn ensure_systemctl(
    args: &[&str],
    action: &str,
    hint: &'static str,
) -> AppResult<std::process::Output> {
    let output = systemctl(args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(AppError::runtime_with_hint(
            format!("failed to {action}: {detail}"),
            hint,
        ));
    }
    Ok(output)
}

impl ServiceManager for SystemdManager {
    fn install(&self) -> AppResult<()> {
        let content = self.unit_content();
        std::fs::create_dir_all(self.log_dir())?;
        if let Some(parent) = std::path::Path::new(&self.unit_file).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.unit_file, content)?;
        ensure_systemctl(
            &["daemon-reload"],
            "reload the systemd daemon",
            "run `sudo systemctl daemon-reload` manually to inspect the error",
        )?;
        ensure_systemctl(
            &["enable", UNIT_NAME],
            "enable the system service",
            "run `sudo systemctl status valsb-sing-box.service` for details",
        )?;
        Ok(())
    }

    fn uninstall(&self) -> AppResult<()> {
        let _ = self.stop();
        let _ = systemctl(&["disable", UNIT_NAME]);
        let _ = std::fs::remove_file(&self.unit_file);
        let _ = systemctl(&["daemon-reload"]);
        let _ = systemctl(&["reset-failed", UNIT_NAME]);
        Ok(())
    }

    fn start(&self) -> AppResult<()> {
        let output = systemctl(&["start", UNIT_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime_with_hint(
                format!("failed to start service: {}", stderr.trim()),
                "run `valsb doctor` to check environment, then inspect `valsb logs`",
            ));
        }
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        let output = systemctl(&["stop", UNIT_NAME])?;
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
        let output = systemctl(&["restart", UNIT_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime(format!(
                "failed to restart service: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn reload(&self) -> AppResult<()> {
        let output = systemctl(&["reload", UNIT_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime_with_hint(
                format!("failed to reload service: {}", stderr.trim()),
                "try `valsb restart` instead",
            ));
        }
        Ok(())
    }

    fn status(&self) -> AppResult<ServiceStatus> {
        let output = systemctl(&["is-active", UNIT_NAME])?;
        let active = output.status.success();
        let state_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let main_pid = systemctl(&["show", "--property=MainPID", "--value", UNIT_NAME])
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u32>()
                    .ok()
            })
            .filter(|&pid| pid > 0);

        Ok(ServiceStatus {
            active,
            state: state_str,
            main_pid,
            unit_file: Some(self.unit_file.clone()),
        })
    }

    fn logs(&self, follow: bool, lines: u32) -> AppResult<()> {
        tail_log_file(
            &format!("{}/sing-box.stderr.log", self.log_dir()),
            follow,
            lines,
        )
    }

    fn is_active(&self) -> AppResult<bool> {
        let output = systemctl(&["is-active", UNIT_NAME])?;
        Ok(output.status.success())
    }

    fn backend_name(&self) -> &'static str {
        "systemd"
    }
}

fn tail_log_file(log_path: &str, follow: bool, lines: u32) -> AppResult<()> {
    if !std::path::Path::new(log_path).exists() {
        println!("No log file found at {log_path}");
        return Ok(());
    }

    let lines_arg = lines.to_string();
    let status = if follow {
        Command::new("tail")
            .args(["-f", "-n", &lines_arg, log_path])
            .status()
    } else {
        Command::new("tail")
            .args(["-n", &lines_arg, log_path])
            .status()
    }
    .map_err(|e| AppError::runtime(format!("failed to run tail: {e}")))?;

    if !status.success() {
        return Err(AppError::runtime("tail exited with an error"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_content_writes_logs_to_files() {
        let manager = SystemdManager::new(
            "/etc/systemd/system/valsb.service",
            "/etc/valsb/config.json",
            "/usr/local/lib/val-sing-box-cli/bin/sing-box",
            "/var/lib/val-sing-box-cli",
        );
        let content = manager.unit_content();
        assert!(
            content.contains(
                "StandardOutput=append:/var/lib/val-sing-box-cli/logs/sing-box.stdout.log"
            )
        );
        assert!(
            content.contains(
                "StandardError=append:/var/lib/val-sing-box-cli/logs/sing-box.stderr.log"
            )
        );
        assert!(content.contains("WantedBy=multi-user.target"));
        assert!(content.contains("LimitNOFILE=infinity"));
    }
}
