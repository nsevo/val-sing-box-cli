use std::path::Path;

use crate::install::ManagedPaths;
use crate::service::ServiceManager;

pub fn run_uninstall(
    managed_paths: &ManagedPaths,
    service_mgr: Option<&dyn ServiceManager>,
) -> Vec<UninstallStep> {
    let mut steps = Vec::new();

    let mut service_uninstalled = false;
    if let Some(mgr) = service_mgr {
        if mgr.is_active().unwrap_or(false) {
            match mgr.stop() {
                Ok(()) => steps.push(UninstallStep::ok("stop service")),
                Err(e) => steps.push(UninstallStep::warn("stop service", &e.to_string())),
            }
        }

        match mgr.uninstall() {
            Ok(()) => {
                steps.push(UninstallStep::ok("uninstall service"));
                service_uninstalled = true;
            }
            Err(e) => steps.push(UninstallStep::warn("uninstall service", &e.to_string())),
        }
    }

    remove_file_step(&mut steps, "valsb binary", &managed_paths.valsb_bin);

    if let Some(sb) = &managed_paths.sing_box_bin {
        remove_file_step(&mut steps, "sing-box binary", sb);
        let bin_dir = Path::new(sb).parent();
        if let Some(parent) = bin_dir {
            remove_dir_step(&mut steps, "sing-box bin dir", &parent.to_string_lossy());
            if let Some(grandparent) = parent.parent() {
                remove_empty_dir_step(
                    &mut steps,
                    "sing-box lib dir",
                    &grandparent.to_string_lossy(),
                );
            }
        }
    }

    // Only explicitly remove the unit file if the service manager didn't
    // already handle it (mgr.uninstall() deletes the file + daemon-reload).
    if !service_uninstalled {
        if let Some(unit) = &managed_paths.unit_file {
            remove_file_step(&mut steps, "unit file", unit);
        }
    }

    remove_dir_step(&mut steps, "config dir", &managed_paths.config_dir);
    remove_dir_step(&mut steps, "cache dir", &managed_paths.cache_dir);
    remove_dir_step(&mut steps, "data dir", &managed_paths.data_dir);

    steps
}

fn remove_file_step(steps: &mut Vec<UninstallStep>, name: &str, path: &str) {
    let p = Path::new(path);
    if p.exists() {
        match std::fs::remove_file(p) {
            Ok(()) => steps.push(UninstallStep::ok(&format!("remove {name}: {path}"))),
            Err(e) => steps.push(UninstallStep::error(
                &format!("remove {name}: {path}"),
                &e.to_string(),
            )),
        }
    } else {
        steps.push(UninstallStep::skip(&format!("{name}: {path} (not found)")));
    }
}

/// Remove a directory only if it exists and is empty. Safe for parent dirs
/// that might be shared (e.g. /usr/local/lib).
fn remove_empty_dir_step(steps: &mut Vec<UninstallStep>, name: &str, path: &str) {
    let p = Path::new(path);
    if p.exists() {
        if let Ok(()) = std::fs::remove_dir(p) {
            steps.push(UninstallStep::ok(&format!("remove {name}: {path}")));
        }
    }
}

fn remove_dir_step(steps: &mut Vec<UninstallStep>, name: &str, path: &str) {
    let p = Path::new(path);
    if p.exists() {
        match std::fs::remove_dir_all(p) {
            Ok(()) => steps.push(UninstallStep::ok(&format!("remove {name}: {path}"))),
            Err(e) => steps.push(UninstallStep::error(
                &format!("remove {name}: {path}"),
                &e.to_string(),
            )),
        }
    } else {
        steps.push(UninstallStep::skip(&format!("{name}: {path} (not found)")));
    }
}

#[derive(Debug, serde::Serialize)]
pub struct UninstallStep {
    pub action: String,
    pub status: String,
    pub error: Option<String>,
}

impl UninstallStep {
    fn ok(action: &str) -> Self {
        Self {
            action: action.to_string(),
            status: "ok".to_string(),
            error: None,
        }
    }

    fn warn(action: &str, err: &str) -> Self {
        Self {
            action: action.to_string(),
            status: "warning".to_string(),
            error: Some(err.to_string()),
        }
    }

    fn error(action: &str, err: &str) -> Self {
        Self {
            action: action.to_string(),
            status: "error".to_string(),
            error: Some(err.to_string()),
        }
    }

    fn skip(action: &str) -> Self {
        Self {
            action: action.to_string(),
            status: "skipped".to_string(),
            error: None,
        }
    }
}
