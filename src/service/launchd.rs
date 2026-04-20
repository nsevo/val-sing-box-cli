use std::process::Command;

use crate::errors::{AppError, AppResult};

use super::{ServiceManager, ServiceStatus};

const LABEL: &str = "com.valsb.sing-box";

fn plist_content(sing_box_bin: &str, config_path: &str, data_dir: &str, log_dir: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{sing_box_bin}</string>
        <string>-D</string>
        <string>{data_dir}</string>
        <string>-c</string>
        <string>{config_path}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log_dir}/sing-box.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/sing-box.stderr.log</string>
</dict>
</plist>
"#
    )
}

fn launchctl(args: &[&str]) -> AppResult<std::process::Output> {
    Command::new("launchctl")
        .args(args)
        .output()
        .map_err(|e| AppError::runtime(format!("failed to run launchctl: {e}")))
}

fn parse_launchctl_list() -> AppResult<(bool, Option<u32>)> {
    let output = launchctl(&["list", LABEL])?;
    if !output.status.success() {
        return Ok((false, None));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pid = stdout
        .lines()
        .find(|l| l.contains("\"PID\""))
        .and_then(|l| l.split('=').last())
        .and_then(|s| s.trim().trim_end_matches(';').trim().parse::<u32>().ok())
        .filter(|&p| p > 0);
    Ok((true, pid))
}

pub struct LaunchdDaemonManager {
    plist_path: String,
    config_path: String,
    sing_box_bin: String,
    data_dir: String,
}

impl LaunchdDaemonManager {
    pub fn new(plist_path: &str, config_path: &str, sing_box_bin: &str, data_dir: &str) -> Self {
        Self {
            plist_path: plist_path.to_string(),
            config_path: config_path.to_string(),
            sing_box_bin: sing_box_bin.to_string(),
            data_dir: data_dir.to_string(),
        }
    }

    fn log_dir(&self) -> String {
        format!("{}/logs", self.data_dir)
    }
}

impl ServiceManager for LaunchdDaemonManager {
    fn install(&self) -> AppResult<()> {
        let log_dir = self.log_dir();
        std::fs::create_dir_all(&log_dir)?;

        let content = plist_content(
            &self.sing_box_bin,
            &self.config_path,
            &self.data_dir,
            &log_dir,
        );
        if let Some(parent) = std::path::Path::new(&self.plist_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.plist_path, content)?;

        let output = launchctl(&["load", &self.plist_path])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime(format!(
                "failed to load launch daemon: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn uninstall(&self) -> AppResult<()> {
        let _ = self.stop();
        let _ = launchctl(&["unload", &self.plist_path]);
        let _ = std::fs::remove_file(&self.plist_path);
        Ok(())
    }

    fn start(&self) -> AppResult<()> {
        let output = launchctl(&["start", LABEL])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime_with_hint(
                format!("failed to start service: {}", stderr.trim()),
                "run `valsb doctor` to check environment",
            ));
        }
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        let output = launchctl(&["stop", LABEL])?;
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
        self.start()
    }

    fn reload(&self) -> AppResult<()> {
        self.restart()
    }

    fn status(&self) -> AppResult<ServiceStatus> {
        let (loaded, pid) = parse_launchctl_list()?;
        Ok(ServiceStatus {
            active: pid.is_some(),
            state: if pid.is_some() {
                "running".to_string()
            } else if loaded {
                "loaded (not running)".to_string()
            } else {
                "not loaded".to_string()
            },
            main_pid: pid,
            unit_file: Some(self.plist_path.clone()),
        })
    }

    fn logs(&self, follow: bool, lines: u32) -> AppResult<()> {
        let log_path = format!("{}/sing-box.stderr.log", self.log_dir());
        if !std::path::Path::new(&log_path).exists() {
            println!("No log file found at {log_path}");
            return Ok(());
        }

        let lines_str = lines.to_string();
        let mut cmd = Command::new("tail");
        if follow {
            cmd.args(["-f", "-n", &lines_str, &log_path]);
        } else {
            cmd.args(["-n", &lines_str, &log_path]);
        }
        let status = cmd
            .status()
            .map_err(|e| AppError::runtime(format!("failed to run tail: {e}")))?;
        if !status.success() {
            return Err(AppError::runtime("tail exited with an error"));
        }
        Ok(())
    }

    fn is_active(&self) -> AppResult<bool> {
        let (_, pid) = parse_launchctl_list()?;
        Ok(pid.is_some())
    }

    fn backend_name(&self) -> &'static str {
        "launchd"
    }
}
