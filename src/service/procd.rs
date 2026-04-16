use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::errors::{AppError, AppResult};

use super::{ServiceManager, ServiceStatus};

const SERVICE_NAME: &str = "valsb-sing-box";

pub struct ProcdManager {
    init_script: String,
    config_path: String,
    sing_box_bin: String,
    data_dir: String,
}

impl ProcdManager {
    pub fn new(init_script: &str, config_path: &str, sing_box_bin: &str, data_dir: &str) -> Self {
        Self {
            init_script: init_script.to_string(),
            config_path: config_path.to_string(),
            sing_box_bin: sing_box_bin.to_string(),
            data_dir: data_dir.to_string(),
        }
    }

    fn init_script_content(&self) -> String {
        format!(
            r#"#!/bin/sh /etc/rc.common

START=99
STOP=10

USE_PROCD=1

start_service() {{
    procd_open_instance
    procd_set_param command {bin} -D {data} -c {config} run
    procd_set_param respawn
    procd_set_param stderr 1
    procd_set_param stdout 1
    procd_close_instance
}}

reload_service() {{
    local pid
    pid=$(pgrep -f "{bin}.*run")
    [ -n "$pid" ] && kill -HUP "$pid"
}}
"#,
            bin = self.sing_box_bin,
            config = self.config_path,
            data = self.data_dir,
        )
    }

    fn run_init_action(&self, action: &str) -> AppResult<std::process::Output> {
        Command::new(&self.init_script)
            .arg(action)
            .output()
            .map_err(|e| AppError::runtime(format!("failed to run `{}`: {e}", self.init_script)))
    }

    fn ensure_init_action(&self, action: &str, what: &str) -> AppResult<()> {
        let output = self.run_init_action(action)?;
        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(AppError::runtime(format!(
            "failed to {what}: {}",
            stderr.trim()
        )))
    }
}

impl ServiceManager for ProcdManager {
    fn install(&self) -> AppResult<()> {
        let content = self.init_script_content();
        std::fs::write(&self.init_script, content)?;

        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&self.init_script, perms)?;

        if let Err(err) = self.ensure_init_action("enable", "enable procd service") {
            let _ = std::fs::remove_file(&self.init_script);
            return Err(err);
        }

        self.ensure_init_action("enabled", "verify procd service enable state")?;
        Ok(())
    }

    fn uninstall(&self) -> AppResult<()> {
        if !std::path::Path::new(&self.init_script).exists() {
            return Ok(());
        }

        let _ = self.stop();
        self.ensure_init_action("disable", "disable procd service")?;

        match std::fs::remove_file(&self.init_script) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(AppError::runtime(format!(
                    "failed to remove init script {}: {err}",
                    self.init_script
                )));
            }
        }

        Ok(())
    }

    fn start(&self) -> AppResult<()> {
        let output = Command::new("/etc/init.d/valsb-sing-box")
            .arg("start")
            .output()
            .map_err(|e| AppError::runtime(format!("failed to start service: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime_with_hint(
                format!("failed to start procd service: {}", stderr.trim()),
                "run `valsb doctor` to check environment, or `logread -e sing-box` for logs",
            ));
        }
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        let output = Command::new("/etc/init.d/valsb-sing-box")
            .arg("stop")
            .output()
            .map_err(|e| AppError::runtime(format!("failed to stop service: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime(format!(
                "failed to stop procd service: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn restart(&self) -> AppResult<()> {
        let output = Command::new("/etc/init.d/valsb-sing-box")
            .arg("restart")
            .output()
            .map_err(|e| AppError::runtime(format!("failed to restart service: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime(format!(
                "failed to restart procd service: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }

    fn reload(&self) -> AppResult<()> {
        let output = Command::new("/etc/init.d/valsb-sing-box")
            .arg("reload")
            .output()
            .map_err(|e| AppError::runtime(format!("failed to reload service: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime_with_hint(
                format!("failed to reload procd service: {}", stderr.trim()),
                "try `valsb restart` instead",
            ));
        }
        Ok(())
    }

    fn status(&self) -> AppResult<ServiceStatus> {
        let output = Command::new("pgrep")
            .args(["-f", "sing-box.*run"])
            .output()
            .ok();

        let (active, main_pid) = match output {
            Some(o) if o.status.success() => {
                let pid_str = String::from_utf8_lossy(&o.stdout);
                let pid = pid_str.lines().next().and_then(|l| l.trim().parse().ok());
                (true, pid)
            }
            _ => (false, None),
        };

        Ok(ServiceStatus {
            active,
            state: if active {
                "running".to_string()
            } else {
                "stopped".to_string()
            },
            main_pid,
            unit_file: Some(self.init_script.clone()),
        })
    }

    fn logs(&self, follow: bool, lines: u32) -> AppResult<()> {
        let lines_str = lines.to_string();
        let mut args = vec!["-e", SERVICE_NAME, "-l", &lines_str];
        if follow {
            args.push("-f");
        }

        let status = Command::new("logread")
            .args(&args)
            .status()
            .map_err(|e| AppError::runtime(format!("failed to run logread: {e}")))?;

        if !status.success() {
            return Err(AppError::runtime("logread exited with an error"));
        }
        Ok(())
    }

    fn is_active(&self) -> AppResult<bool> {
        let output = Command::new("pgrep")
            .args(["-f", "sing-box.*run"])
            .output()
            .ok();

        Ok(output.is_some_and(|o| o.status.success()))
    }

    fn backend_name(&self) -> &'static str {
        "OpenWrt procd"
    }
}
