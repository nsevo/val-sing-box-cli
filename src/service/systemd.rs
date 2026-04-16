use std::process::Command;

use crate::errors::{AppError, AppResult};
use crate::install::DelegatedControl;
use crate::platform::AppPaths;

use super::{ServiceManager, ServiceStatus};

const UNIT_NAME: &str = "valsb-sing-box.service";
const CONTROL_GROUP: &str = "valsb";
const POLKIT_RULE_PATH: &str = "/etc/polkit-1/rules.d/90-valsb-systemd.rules";

fn command_error_text(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }

    format!("command exited with status {}", output.status)
}

fn ensure_systemctl_user(
    args: &[&str],
    action: &str,
    hint: &'static str,
) -> AppResult<std::process::Output> {
    let output = systemctl_user(args)?;
    if !output.status.success() {
        return Err(AppError::runtime_with_hint(
            format!("failed to {action}: {}", command_error_text(&output)),
            hint,
        ));
    }
    Ok(output)
}

fn ensure_systemctl_system(
    args: &[&str],
    action: &str,
    hint: &'static str,
) -> AppResult<std::process::Output> {
    let output = systemctl_system(args)?;
    if !output.status.success() {
        return Err(AppError::runtime_with_hint(
            format!("failed to {action}: {}", command_error_text(&output)),
            hint,
        ));
    }
    Ok(output)
}

pub struct SystemdUserManager {
    unit_file: String,
    config_path: String,
    sing_box_bin: String,
    data_dir: String,
}

impl SystemdUserManager {
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
StandardOutput=append:{stdout_log}
StandardError=append:{stderr_log}

[Install]
WantedBy=default.target
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

fn systemctl_user(args: &[&str]) -> AppResult<std::process::Output> {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|e| AppError::runtime(format!("failed to run systemctl: {e}")))
}

fn systemctl_system(args: &[&str]) -> AppResult<std::process::Output> {
    Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| AppError::runtime(format!("failed to run systemctl: {e}")))
}

pub fn configure_system_delegate(
    paths: &AppPaths,
    delegate_user: &str,
) -> AppResult<DelegatedControl> {
    ensure_group_exists(CONTROL_GROUP)?;
    add_user_to_group(delegate_user, CONTROL_GROUP)?;
    apply_group_access(paths, CONTROL_GROUP)?;
    write_polkit_rule()?;

    Ok(DelegatedControl {
        mode: "systemd_polkit_group".to_string(),
        principal: Some(delegate_user.to_string()),
        group: Some(CONTROL_GROUP.to_string()),
    })
}

fn ensure_group_exists(group: &str) -> AppResult<()> {
    let output = Command::new("getent")
        .args(["group", group])
        .output()
        .map_err(|e| AppError::runtime(format!("failed to check group {group}: {e}")))?;
    if output.status.success() {
        return Ok(());
    }

    let status = Command::new("groupadd")
        .args(["--system", group])
        .status()
        .map_err(|e| AppError::runtime(format!("failed to create group {group}: {e}")))?;
    if !status.success() {
        return Err(AppError::runtime_with_hint(
            format!("failed to create group {group}"),
            "install `groupadd` utilities or create the group manually before retrying",
        ));
    }
    Ok(())
}

fn add_user_to_group(user: &str, group: &str) -> AppResult<()> {
    let status = Command::new("usermod")
        .args(["-a", "-G", group, user])
        .status()
        .map_err(|e| AppError::runtime(format!("failed to add {user} to group {group}: {e}")))?;
    if !status.success() {
        return Err(AppError::runtime_with_hint(
            format!("failed to add {user} to group {group}"),
            "make sure the user exists and the system provides `usermod`",
        ));
    }
    Ok(())
}

fn apply_group_access(paths: &AppPaths, group: &str) -> AppResult<()> {
    let gid = lookup_group_gid(group)?;
    for path in [&paths.config_dir, &paths.cache_dir, &paths.data_dir] {
        if path.exists() {
            apply_group_access_recursive(path, gid)?;
        }
    }
    Ok(())
}

fn lookup_group_gid(group: &str) -> AppResult<u32> {
    let output = Command::new("getent")
        .args(["group", group])
        .output()
        .map_err(|e| AppError::runtime(format!("failed to resolve group {group}: {e}")))?;
    if !output.status.success() {
        return Err(AppError::runtime(format!(
            "group {group} not found after creation"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let gid = stdout
        .trim()
        .split(':')
        .nth(2)
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or_else(|| AppError::runtime(format!("failed to parse gid for group {group}")))?;
    Ok(gid)
}

fn apply_group_access_recursive(path: &std::path::Path, gid: u32) -> AppResult<()> {
    set_group_and_mode(path, gid)?;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            apply_group_access_recursive(&entry.path(), gid)?;
        }
    }
    Ok(())
}

fn set_group_and_mode(path: &std::path::Path, gid: u32) -> AppResult<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        AppError::runtime(format!("path contains invalid bytes: {}", path.display()))
    })?;
    let chown_result = unsafe { libc::chown(c_path.as_ptr(), u32::MAX, gid) };
    if chown_result != 0 {
        return Err(AppError::runtime(format!(
            "failed to update group ownership for {}",
            path.display()
        )));
    }

    let mode = if path.is_dir() { 0o2770 } else { 0o660 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn write_polkit_rule() -> AppResult<()> {
    let rule = format!(
        r#"polkit.addRule(function(action, subject) {{
    if (action.id != "org.freedesktop.systemd1.manage-units") {{
        return;
    }}

    var unit = action.lookup("unit");
    var verb = action.lookup("verb");
    var allowed = ["start", "stop", "restart", "reload"];
    if (unit == "{UNIT_NAME}" && subject.isInGroup("{CONTROL_GROUP}") && allowed.indexOf(verb) >= 0) {{
        return polkit.Result.YES;
    }}
}});"#
    );

    if let Some(parent) = std::path::Path::new(POLKIT_RULE_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(POLKIT_RULE_PATH, rule)?;
    Ok(())
}

impl ServiceManager for SystemdUserManager {
    fn install(&self) -> AppResult<()> {
        let content = self.unit_content();
        std::fs::create_dir_all(self.log_dir())?;
        if let Some(parent) = std::path::Path::new(&self.unit_file).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.unit_file, content)?;
        ensure_systemctl_user(
            &["daemon-reload"],
            "reload the user systemd daemon",
            "make sure this command is run from a logged-in user session with `XDG_RUNTIME_DIR` set",
        )?;
        ensure_systemctl_user(
            &["enable", UNIT_NAME],
            "enable the user service",
            "run `systemctl --user status valsb-sing-box.service` for details, and enable lingering if you need it to start before login",
        )?;
        Ok(())
    }

    fn uninstall(&self) -> AppResult<()> {
        let _ = self.stop();
        let _ = systemctl_user(&["disable", UNIT_NAME]);
        let _ = std::fs::remove_file(&self.unit_file);
        let _ = systemctl_user(&["daemon-reload"]);
        let _ = systemctl_user(&["reset-failed", UNIT_NAME]);
        Ok(())
    }

    fn start(&self) -> AppResult<()> {
        let output = systemctl_user(&["start", UNIT_NAME])?;
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
        let output = systemctl_user(&["stop", UNIT_NAME])?;
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
        let output = systemctl_user(&["restart", UNIT_NAME])?;
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
        let output = systemctl_user(&["reload", UNIT_NAME])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::runtime_with_hint(
                format!("failed to reload service: {}", stderr.trim()),
                "the service unit may not support reload, try `valsb restart`",
            ));
        }
        Ok(())
    }

    fn status(&self) -> AppResult<ServiceStatus> {
        let output = systemctl_user(&["is-active", UNIT_NAME])?;
        let active = output.status.success();
        let state_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let main_pid = systemctl_user(&["show", "--property=MainPID", "--value", UNIT_NAME])
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
        let output = systemctl_user(&["is-active", UNIT_NAME])?;
        Ok(output.status.success())
    }

    fn backend_name(&self) -> &'static str {
        "systemd --user"
    }
}

pub struct SystemdSystemManager {
    unit_file: String,
    config_path: String,
    sing_box_bin: String,
    data_dir: String,
}

impl SystemdSystemManager {
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

impl ServiceManager for SystemdSystemManager {
    fn install(&self) -> AppResult<()> {
        let content = self.unit_content();
        std::fs::create_dir_all(self.log_dir())?;
        if let Some(parent) = std::path::Path::new(&self.unit_file).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.unit_file, content)?;
        ensure_systemctl_system(
            &["daemon-reload"],
            "reload the systemd daemon",
            "run `sudo systemctl daemon-reload` manually to inspect the error",
        )?;
        ensure_systemctl_system(
            &["enable", UNIT_NAME],
            "enable the system service",
            "run `sudo systemctl status valsb-sing-box.service` for details",
        )?;
        Ok(())
    }

    fn uninstall(&self) -> AppResult<()> {
        let _ = self.stop();
        let _ = systemctl_system(&["disable", UNIT_NAME]);
        let _ = std::fs::remove_file(&self.unit_file);
        let _ = systemctl_system(&["daemon-reload"]);
        let _ = systemctl_system(&["reset-failed", UNIT_NAME]);
        Ok(())
    }

    fn start(&self) -> AppResult<()> {
        let output = systemctl_system(&["start", UNIT_NAME])?;
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
        let output = systemctl_system(&["stop", UNIT_NAME])?;
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
        let output = systemctl_system(&["restart", UNIT_NAME])?;
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
        let output = systemctl_system(&["reload", UNIT_NAME])?;
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
        let output = systemctl_system(&["is-active", UNIT_NAME])?;
        let active = output.status.success();
        let state_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

        let main_pid = systemctl_system(&["show", "--property=MainPID", "--value", UNIT_NAME])
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
        let output = systemctl_system(&["is-active", UNIT_NAME])?;
        Ok(output.status.success())
    }

    fn backend_name(&self) -> &'static str {
        "systemd system"
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
    fn user_unit_content_writes_logs_to_files() {
        let manager = SystemdUserManager::new(
            "/tmp/valsb.service",
            "/tmp/config.json",
            "/usr/bin/sing-box",
            "/tmp/valsb-data",
        );
        let content = manager.unit_content();
        assert!(content.contains("StandardOutput=append:/tmp/valsb-data/logs/sing-box.stdout.log"));
        assert!(content.contains("StandardError=append:/tmp/valsb-data/logs/sing-box.stderr.log"));
    }

    #[test]
    fn system_unit_content_writes_logs_to_files() {
        let manager = SystemdSystemManager::new(
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
    }
}
