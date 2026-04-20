use chrono::Utc;
use serde_json::json;
#[cfg(windows)]
use std::process::Command;

use crate::cli::{Commands, ConfigCommands, NodeCommands, SubCommands};
use crate::config;
use crate::doctor::DoctorReport;
use crate::errors::{AppError, AppResult};
use crate::install::{self, Manifest};
use crate::output::{JsonOutput, Renderer};
use crate::platform::{AppPaths, Platform};
use crate::state::{self, AppState, Profile, RemarkSource, UpdateStatus};
use crate::subscription;
use crate::ui;
use crate::uninstall;

#[derive(Clone)]
pub struct App {
    pub platform: Platform,
    pub paths: AppPaths,
    pub renderer: Renderer,
    pub yes: bool,
    pub config_dir_override: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocationSnapshot {
    pub country: String,
    pub city: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatusSnapshot {
    pub state: String,
    pub kernel_version: String,
    pub profile: Option<String>,
    pub node: Option<String>,
    pub exit_ip: Option<String>,
    pub location: Option<LocationSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionSnapshot {
    pub valsb_version: String,
    pub sing_box_version: String,
    pub platform: String,
}

impl App {
    pub fn new(config_dir: Option<&str>, json: bool, verbose: bool, yes: bool) -> AppResult<Self> {
        if verbose {
            tracing_subscriber::fmt()
                .with_env_filter("valsb=debug")
                .with_writer(std::io::stderr)
                .init();
        }

        let platform = Platform::detect().map_err(AppError::env)?;
        let paths = AppPaths::resolve(platform.os_family, config_dir);
        let renderer = Renderer::new(json);

        Ok(Self {
            platform,
            paths,
            renderer,
            yes,
            config_dir_override: config_dir.map(str::to_string),
        })
    }

    fn build_manifest(&self, sing_box_version: Option<String>) -> Manifest {
        Manifest::new(&self.paths, sing_box_version)
    }

    fn load_state(&self) -> AppResult<AppState> {
        let mut state = state::load_state(&self.paths.state_file())?;
        if state.normalize_active_profile() {
            self.save_state(&state)?;
        }
        Ok(state)
    }

    fn save_state(&self, s: &AppState) -> AppResult<()> {
        state::save_state(&self.paths.state_file(), s)
    }

    fn mark_profile_update(
        &self,
        profile_id: &str,
        status: UpdateStatus,
        error: Option<String>,
        node_count: Option<usize>,
    ) -> AppResult<()> {
        let mut state = self.load_state()?;
        if let Some(p) = state.profiles.iter_mut().find(|p| p.id == profile_id) {
            p.last_update_at = Some(Utc::now());
            p.last_update_status = Some(status);
            p.last_update_error = error;
            if let Some(count) = node_count {
                p.node_count = count;
            }
        }
        self.save_state(&state)
    }

    fn sing_box_version(&self) -> String {
        self.resolve_sing_box_bin()
            .map_or_else(|| "unknown".to_string(), |p| parse_sing_box_version(&p))
    }

    fn resolve_sing_box_bin(&self) -> Option<std::path::PathBuf> {
        let managed = self.paths.sing_box_binary();
        if managed.exists() {
            return Some(managed);
        }
        Platform::sing_box_path().map(std::path::PathBuf::from)
    }

    fn get_service_manager(&self) -> AppResult<Box<dyn crate::service::ServiceManager>> {
        let backend = self.platform.default_backend().ok_or_else(|| {
            AppError::env_with_hint(
                "no supported service backend available",
                "run `valsb doctor` to check your environment",
            )
        })?;

        let config_path = self.paths.generated_config_file();
        let data_dir_str = self.paths.data_dir.to_string_lossy().into_owned();

        let sing_box_bin = self.resolve_sing_box_bin().map_or_else(
            || self.paths.sing_box_binary().to_string_lossy().into_owned(),
            |p| p.to_string_lossy().into_owned(),
        );

        Ok(crate::service::create_manager(
            backend,
            &self.paths.unit_file.to_string_lossy(),
            &config_path.to_string_lossy(),
            &sing_box_bin,
            &data_dir_str,
        ))
    }

    fn clash_client(&self) -> AppResult<crate::clash::ClashClient> {
        let state = self.load_state()?;
        let addr = state
            .clash_api_addr
            .as_deref()
            .unwrap_or(crate::clash::DEFAULT_EXTERNAL_CONTROLLER);
        crate::clash::ClashClient::new(addr, None)
    }

    async fn sync_groups_with_clash_api(&self, groups: &mut [OutboundGroup]) -> AppResult<()> {
        if !self.is_service_running() {
            clear_group_currents(groups);
            return Ok(());
        }

        let client = self.clash_client()?;
        let proxies = client.fetch_proxy_groups().await.map_err(|e| {
            AppError::runtime_with_hint(
                format!("failed to query Clash API proxy state: {e}"),
                "ensure sing-box is running, then retry or inspect `valsb logs`",
            )
        })?;
        apply_clash_proxy_groups(groups, &proxies);
        Ok(())
    }

    async fn current_selector_node(&self) -> Option<String> {
        if !self.is_service_running() {
            return None;
        }

        let (raw_config, _) = self.load_active_raw_config_for_node().ok()?;
        let mut groups = extract_groups_from_config(&raw_config);
        self.sync_groups_with_clash_api(&mut groups).await.ok()?;
        groups
            .into_iter()
            .find(|g| g.group_type == "selector")
            .and_then(|g| g.current)
    }

    pub async fn status_snapshot(&self, include_exit_info: bool) -> AppResult<StatusSnapshot> {
        let mgr = self.get_service_manager()?;
        let status = mgr.status()?;
        let state = self.load_state()?;
        let is_running = status.active;

        let (exit_ip, location) = if include_exit_info && is_running {
            match crate::ip::detect_exit_ip().await {
                Some(ip_info) => (
                    Some(ip_info.ip),
                    Some(LocationSnapshot {
                        country: ip_info.country,
                        city: ip_info.city,
                    }),
                ),
                None => (None, None),
            }
        } else {
            (None, None)
        };

        Ok(StatusSnapshot {
            state: if is_running {
                "running".to_string()
            } else {
                "stopped".to_string()
            },
            kernel_version: self.sing_box_version(),
            profile: state.active_profile().map(|p| p.remark.clone()),
            node: self.current_selector_node().await,
            exit_ip,
            location,
        })
    }

    pub fn version_snapshot(&self) -> VersionSnapshot {
        VersionSnapshot {
            valsb_version: env!("CARGO_PKG_VERSION").to_string(),
            sing_box_version: self.sing_box_version(),
            platform: format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        }
    }

    /// Re-execute the current command as root if needed.
    ///
    /// valsb is a root-only tool. Any state lives under system paths
    /// (`/etc`, `/var/lib`, `/var/cache`) or `%PROGRAMDATA%`, and the
    /// service runs as root because TUN mode requires it. When a regular
    /// user invokes a command that touches state or the service we just
    /// re-exec ourselves under sudo / UAC and propagate the exit code.
    ///
    /// Pure-local commands (`version`, `completion`, the Windows service
    /// worker entry point) skip elevation. Passing `--config-dir <path>`
    /// also bypasses elevation so tests and scratch installs can run as a
    /// regular user.
    pub fn maybe_elevate(&self, command: &Commands) -> AppResult<()> {
        if self.config_dir_override.is_some()
            || self.platform.is_root
            || !command_requires_root(command)
        {
            return Ok(());
        }

        #[cfg(not(windows))]
        {
            relaunch_with_sudo()?;
            std::process::exit(0);
        }

        #[cfg(windows)]
        {
            relaunch_as_admin()?;
            std::process::exit(0);
        }
    }

    // ── Command dispatch ──────────────────────────────────────────────

    pub async fn run(&self, command: Commands) -> AppResult<()> {
        match command {
            Commands::Start => self.cmd_start().await,
            Commands::Stop => self.cmd_stop(),
            Commands::Restart => self.cmd_restart().await,
            Commands::Status => self.cmd_status().await,
            Commands::Reload => self.cmd_reload(),
            Commands::Logs { follow, lines } => self.cmd_logs(follow, lines),
            Commands::Install => self.cmd_install().await,
            Commands::Update => self.cmd_update().await,
            Commands::Uninstall => self.cmd_uninstall(),
            Commands::Sub(sub) => self.cmd_sub(sub).await,
            Commands::Node(sub) => self.cmd_node(sub).await,
            Commands::Config(sub) => self.cmd_config(sub),
            Commands::Completion { .. } => Ok(()),
            Commands::Doctor => {
                self.cmd_doctor();
                Ok(())
            }
            Commands::Version => {
                self.cmd_version();
                Ok(())
            }
            Commands::ServiceWorker {
                sing_box_bin,
                config,
                log_dir,
            } => Self::cmd_service_worker(&sing_box_bin, &config, &log_dir),
        }
    }

    fn cmd_service_worker(sing_box_bin: &str, config: &str, log_dir: &str) -> AppResult<()> {
        #[cfg(windows)]
        {
            crate::service::windows_svc::run_service_worker(sing_box_bin, config, log_dir)
                .map_err(|e| AppError::runtime(format!("service worker failed: {e}")))
        }
        #[cfg(not(windows))]
        {
            let _ = (sing_box_bin, config, log_dir);
            Err(AppError::user_with_hint(
                "service-worker is only available on Windows",
                "this command is called by the Windows Service Control Manager",
            ))
        }
    }

    // ── start / stop / restart / status / reload / logs ───────────────

    async fn cmd_start(&self) -> AppResult<()> {
        if !self.renderer.is_json() {
            self.preflight_checks()?;
        }

        let mgr = self.get_service_manager()?;

        if mgr.is_active().unwrap_or(false) {
            if self.renderer.is_json() {
                return Err(AppError::runtime("sing-box is already running"));
            }
            ui::print_warn("sing-box is already running");
            ui::print_hint("run: valsb restart   to restart");
            ui::print_hint("run: valsb status    to view status");
            return Ok(());
        }

        if self.renderer.is_json() {
            config::check_generated_config_exists(&self.paths)?;
            if let Some(bin) = self.resolve_sing_box_bin() {
                config::validate_config(&bin, &self.paths.generated_config_file())?;
            }
            mgr.start()?;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let ip_info = crate::ip::detect_exit_ip().await;
            let node = self.current_selector_node().await;
            self.renderer.print_json(&JsonOutput::success(
                "start",
                json!({
                    "started": true,
                    "node": node,
                    "exit_ip": ip_info.as_ref().map(|i| &i.ip),
                    "location": ip_info.as_ref().map(|i| json!({"country": i.country, "city": i.city})),
                }),
            ));
            return Ok(());
        }

        let sp = ui::spinner("Starting sing-box...");
        mgr.start()?;
        ui::finish_ok(&sp, "sing-box started");

        println!();
        self.show_exit_info().await;

        Ok(())
    }

    fn cmd_stop(&self) -> AppResult<()> {
        let mgr = self.get_service_manager()?;

        if !mgr.is_active().unwrap_or(false) {
            if self.renderer.is_json() {
                return Err(AppError::runtime("sing-box is not running"));
            }
            ui::print_warn("sing-box is not running");
            return Ok(());
        }

        if self.renderer.is_json() {
            mgr.stop()?;
            self.renderer
                .print_json(&JsonOutput::success("stop", json!({"stopped": true})));
            return Ok(());
        }

        let sp = ui::spinner("Stopping sing-box...");
        mgr.stop()?;
        ui::finish_ok(&sp, "sing-box stopped");

        Ok(())
    }

    async fn cmd_restart(&self) -> AppResult<()> {
        if !self.renderer.is_json() {
            config::check_generated_config_exists(&self.paths)?;
        }

        let mgr = self.get_service_manager()?;

        if self.renderer.is_json() {
            config::check_generated_config_exists(&self.paths)?;
            mgr.restart()?;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let ip_info = crate::ip::detect_exit_ip().await;
            let node = self.current_selector_node().await;
            self.renderer.print_json(&JsonOutput::success(
                "restart",
                json!({
                    "restarted": true,
                    "node": node,
                    "exit_ip": ip_info.as_ref().map(|i| &i.ip),
                    "location": ip_info.as_ref().map(|i| json!({"country": i.country, "city": i.city})),
                }),
            ));
            return Ok(());
        }

        let sp = ui::spinner("Restarting sing-box...");
        mgr.restart()?;
        ui::finish_ok(&sp, "sing-box restarted");

        println!();
        self.show_exit_info().await;

        Ok(())
    }

    async fn cmd_status(&self) -> AppResult<()> {
        let mgr = self.get_service_manager()?;
        let status = mgr.status()?;
        let state = self.load_state()?;
        let sb_version = self.sing_box_version();
        let is_running = status.active;

        if self.renderer.is_json() {
            let snapshot = self.status_snapshot(true).await?;
            self.renderer
                .print_json(&JsonOutput::success("status", &snapshot));
            return Ok(());
        }

        if is_running {
            ui::print_status_running();
        } else {
            ui::print_status_stopped();
        }

        ui::print_kv("Kernel", &format!("sing-box {sb_version}"));
        ui::print_kv("Backend", mgr.backend_name());

        if let Some(profile) = state.active_profile() {
            ui::print_kv(
                "Profile",
                &console::style(&profile.remark).bold().to_string(),
            );
            if let Some(t) = &profile.last_update_at {
                ui::print_kv_colored(
                    "Updated",
                    &t.format("%Y-%m-%d %H:%M").to_string(),
                    console::Color::Green,
                );
            }
        }

        if is_running {
            println!();
            let node = self.current_selector_node().await;
            if let Some(ref n) = node {
                ui::print_kv_highlight("Node", n);
            }

            let sp = ui::spinner("Detecting exit IP...");
            match crate::ip::detect_exit_ip().await {
                Some(ip_info) => {
                    ui::finish_ok(&sp, "Exit IP detected");
                    ui::print_kv_highlight("Exit IP", &ip_info.ip);
                    ui::print_kv_highlight("Location", &ip_info.location_display());
                }
                None => {
                    ui::finish_fail(&sp, "Exit IP detection timed out");
                }
            }
        }

        Ok(())
    }

    fn cmd_reload(&self) -> AppResult<()> {
        config::check_generated_config_exists(&self.paths)?;

        let mgr = self.get_service_manager()?;

        if !mgr.is_active()? {
            return Err(AppError::runtime_with_hint(
                "service is not running",
                "start the service first with `valsb start`",
            ));
        }

        if self.renderer.is_json() {
            mgr.reload()?;
            self.renderer
                .print_json(&JsonOutput::success("reload", json!({"reloaded": true})));
            return Ok(());
        }

        let sp = ui::spinner("Reloading configuration...");
        mgr.reload()?;
        ui::finish_ok(&sp, "Configuration reloaded");

        Ok(())
    }

    fn cmd_logs(&self, follow: bool, lines: u32) -> AppResult<()> {
        let mgr = self.get_service_manager()?;
        mgr.logs(follow, lines)?;
        Ok(())
    }

    // ── version / doctor ──────────────────────────────────────────────

    fn cmd_version(&self) {
        let snapshot = self.version_snapshot();

        if self.renderer.is_json() {
            self.renderer
                .print_json(&JsonOutput::success("version", &snapshot));
        } else {
            ui::print_kv(
                "valsb",
                &console::style(&snapshot.valsb_version)
                    .green()
                    .bold()
                    .to_string(),
            );
            ui::print_kv("sing-box", &snapshot.sing_box_version);
            ui::print_kv("platform", &snapshot.platform);
        }
    }

    fn cmd_doctor(&self) {
        let report = DoctorReport::run(&self.platform, &self.paths);

        if self.renderer.is_json() {
            self.renderer
                .print_json(&JsonOutput::success("doctor", &report));
        } else {
            report.print_human();
        }
    }

    // ── config ────────────────────────────────────────────────────────

    fn cmd_config(&self, sub: ConfigCommands) -> AppResult<()> {
        match sub {
            ConfigCommands::Init => {
                config::init_config(&self.paths)?;
                if self.renderer.is_json() {
                    self.renderer.print_json(&JsonOutput::success(
                        "config init",
                        json!({
                            "config_dir": self.paths.config_dir.to_string_lossy(),
                            "data_dir": self.paths.data_dir.to_string_lossy(),
                            "cache_dir": self.paths.cache_dir.to_string_lossy(),
                        }),
                    ));
                } else {
                    ui::print_ok("Config directories initialized");
                }
                Ok(())
            }
            ConfigCommands::Path => {
                if self.renderer.is_json() {
                    self.renderer.print_json(&JsonOutput::success(
                        "config path",
                        json!({
                            "config_dir": self.paths.config_dir.to_string_lossy(),
                            "cache_dir": self.paths.cache_dir.to_string_lossy(),
                            "data_dir": self.paths.data_dir.to_string_lossy(),
                            "generated_config": self.paths.generated_config_file().to_string_lossy().into_owned(),
                            "state_file": self.paths.state_file().to_string_lossy().into_owned(),
                            "kernel": self.paths.sing_box_binary().to_string_lossy().into_owned(),
                            "unit_file": self.paths.unit_file.to_string_lossy().into_owned(),
                        }),
                    ));
                } else {
                    ui::print_kv("Config", &self.paths.config_dir.to_string_lossy());
                    ui::print_kv("Data", &self.paths.data_dir.to_string_lossy());
                    ui::print_kv("Cache", &self.paths.cache_dir.to_string_lossy());
                    ui::print_kv("Kernel", &self.paths.sing_box_binary().to_string_lossy());
                    ui::print_kv("Service", &self.paths.unit_file.to_string_lossy());
                    ui::print_kv("State", &self.paths.state_file().to_string_lossy());
                }
                Ok(())
            }
            ConfigCommands::List => self.sub_list(),
        }
    }

    fn activate_profile(&self, target: &str) -> AppResult<()> {
        let mut state = self.load_state()?;
        let idx = state.resolve_target(target).ok_or_else(|| {
            AppError::user_with_hint(
                format!("profile not found: {target}"),
                "run `valsb sub list` to see available subscriptions",
            )
        })?;

        let old_active_id = state.active_profile_id.clone();
        let profile = &state.profiles[idx];
        let new_profile_id = profile.id.clone();
        let remark = profile.remark.clone();
        let raw_config = config::read_raw_config(&self.paths, &new_profile_id).map_err(|_| {
            AppError::user_with_hint(
                format!("profile '{remark}' has no cached config"),
                format!(
                    "run `valsb sub update {}` to fetch subscription data",
                    profile.remark
                ),
            )
        })?;

        state.active_profile_id = Some(new_profile_id.clone());
        self.save_state(&state)?;

        if self.renderer.is_json() {
            let mut applied = false;
            let mut reloaded = false;
            let mut rolled_back = false;
            let mut apply_error: Option<String> = None;
            match self.apply_config_and_reload(&raw_config) {
                Ok(r) => {
                    applied = true;
                    reloaded = r;
                }
                Err(e) => {
                    rolled_back = true;
                    apply_error = Some(e.to_string());
                    let _ = self.rollback_active_profile(old_active_id.as_deref());
                }
            }
            let final_state = self.load_state()?;
            let activated = final_state.active_profile_id.as_deref() == Some(&new_profile_id);
            let mut payload = json!({
                "profile_id": new_profile_id,
                "remark": remark,
                "applied": applied,
                "reloaded": reloaded,
                "rolled_back": rolled_back,
                "activated": activated,
                "active_profile_id": final_state.active_profile_id,
            });
            if let Some(err) = apply_error {
                payload["error"] = json!(err);
            }
            self.renderer
                .print_json(&JsonOutput::success("sub use", payload));
            return Ok(());
        }

        match self.apply_config_and_reload(&raw_config) {
            Ok(_) => {
                ui::print_ok(&format!("Active profile: {remark}"));
            }
            Err(e) => {
                ui::print_warn(&format!("Switch failed: {e}"));
                self.rollback_active_profile(old_active_id.as_deref())?;
                ui::print_warn("Rolled back to previous active profile");
                return Err(AppError::runtime(format!("profile switch failed: {e}")));
            }
        }

        Ok(())
    }

    // ── sub (subscription) ────────────────────────────────────────────

    async fn cmd_sub(&self, sub: SubCommands) -> AppResult<()> {
        match sub {
            SubCommands::Add { url, remark } => {
                let url = match url {
                    Some(url) => url,
                    None => self.prompt_subscription_url()?,
                };
                self.sub_add(&url, remark.as_deref()).await
            }
            SubCommands::List => self.sub_list(),
            SubCommands::Update { target } => self.sub_update(target.as_deref()).await,
            SubCommands::Use { target } => {
                let resolved = match target {
                    Some(t) => t,
                    None => self.interactive_select_profile("Select subscription to activate:")?,
                };
                self.activate_profile(&resolved)
            }
            SubCommands::Remove { target } => {
                let resolved = match target {
                    Some(t) => t,
                    None => self.interactive_select_profile("Select subscription to remove:")?,
                };
                self.sub_remove(&resolved)
            }
        }
    }

    fn resolve_add_profile(
        &self,
        url: &str,
        remark: Option<&str>,
    ) -> AppResult<(String, bool, String)> {
        let mut state = self.load_state()?;
        let normalized = subscription::normalize_url(url);

        let existing = state
            .find_profile_mut_by_normalized_url(&normalized)
            .map(|p| p.id.clone());

        let result = if let Some(existing_id) = existing {
            if let Some(new_remark) = remark {
                let current_remark = state
                    .profiles
                    .iter()
                    .find(|p| p.id == existing_id)
                    .map(|p| p.remark.clone())
                    .unwrap_or_default();

                if new_remark != current_remark
                    && state
                        .profiles
                        .iter()
                        .any(|p| p.remark == new_remark && p.id != existing_id)
                {
                    return Err(AppError::user_with_hint(
                        format!("remark '{new_remark}' is already in use"),
                        "choose a different remark with --remark",
                    ));
                }
                if new_remark != current_remark {
                    let profile = state
                        .profiles
                        .iter_mut()
                        .find(|p| p.id == existing_id)
                        .ok_or_else(|| {
                            AppError::runtime(format!(
                                "profile '{existing_id}' disappeared from state"
                            ))
                        })?;
                    profile.remark = new_remark.to_string();
                    profile.remark_source = RemarkSource::Manual;
                }
            }

            let r = state
                .profiles
                .iter()
                .find(|p| p.id == existing_id)
                .map(|p| p.remark.clone())
                .unwrap_or_default();
            (existing_id, false, r)
        } else {
            let remark_val = if let Some(r) = remark {
                if state.remark_exists(r) {
                    return Err(AppError::user_with_hint(
                        format!("remark '{r}' is already in use"),
                        "choose a different remark with --remark",
                    ));
                }
                (r.to_string(), RemarkSource::Manual)
            } else {
                let base = subscription::derive_remark(url);
                let unique = state.generate_unique_remark(&base);
                (unique, RemarkSource::Auto)
            };

            let id = format!("prof_{}", uuid::Uuid::new_v4().as_simple());
            let profile = Profile {
                id: id.clone(),
                subscription_url: url.to_string(),
                subscription_url_normalized: normalized.clone(),
                remark: remark_val.0.clone(),
                remark_source: remark_val.1,
                last_update_at: None,
                last_update_status: None,
                last_update_error: None,
                node_count: 0,
            };
            state.profiles.push(profile);

            if state.active_profile_id.is_none() {
                state.active_profile_id = Some(id.clone());
            }

            (id, true, remark_val.0)
        };

        self.save_state(&state)?;
        Ok(result)
    }

    async fn sub_add(&self, url: &str, remark: Option<&str>) -> AppResult<()> {
        config::init_config(&self.paths)?;
        let (profile_id, created, remark_used) = self.resolve_add_profile(url, remark)?;

        let sb_version = self.sing_box_version();

        let sp = self.maybe_spinner("Fetching subscription...");

        let fetch_result = subscription::fetch_subscription(url, &sb_version)
            .await
            .and_then(|content| subscription::parse_subscription_content(&content));

        let mut node_count: usize = 0;
        let mut fetch_error: Option<String> = None;
        let mut reloaded = false;

        match fetch_result {
            Ok(data) => {
                node_count = data.nodes.len();
                config::save_raw_config(&self.paths, &profile_id, &data.raw_config)?;
                self.mark_profile_update(
                    &profile_id,
                    UpdateStatus::Success,
                    None,
                    Some(data.nodes.len()),
                )?;

                let state = self.load_state()?;
                let is_active = state.active_profile_id.as_deref() == Some(&profile_id);
                if is_active {
                    let mut state = state;
                    state.clash_api_addr = Some(data.clash_api_addr.clone());
                    self.save_state(&state)?;
                }

                let label = if created { "Added" } else { "Updated" };
                if let Some(ref pb) = sp {
                    ui::finish_ok(pb, &format!("{label}: {remark_used} ({node_count} nodes)"));
                }

                if is_active {
                    match self.apply_config_and_reload(&data.raw_config) {
                        Ok(r) => reloaded = r,
                        Err(e) => {
                            if !self.renderer.is_json() {
                                ui::print_warn(&format!("Config apply failed: {e}"));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                fetch_error = Some(e.to_string());
                self.mark_profile_update(
                    &profile_id,
                    UpdateStatus::Failed,
                    Some(e.to_string()),
                    None,
                )?;
                if let Some(ref pb) = sp {
                    ui::finish_fail(pb, &format!("{remark_used}: {e}"));
                }
            }
        }

        if self.renderer.is_json() {
            if let Some(ref err) = fetch_error {
                self.renderer.print_json(&JsonOutput::error(
                    "sub add",
                    "fetch_failed",
                    err.clone(),
                    Some("subscription was saved but content fetch failed".to_string()),
                ));
                return Err(AppError::runtime(err.clone()));
            }
            self.renderer.print_json(&JsonOutput::success(
                "sub add",
                json!({
                    "created": created,
                    "updated_existing": !created,
                    "profile": {
                        "id": profile_id,
                        "remark": remark_used,
                        "subscription_url": url,
                    },
                    "node_count": node_count,
                    "reloaded": reloaded,
                }),
            ));
            return Ok(());
        }

        if let Some(fetch_error) = fetch_error {
            return Err(AppError::runtime(format!(
                "subscription saved but fetch failed: {}",
                fetch_error
            )));
        }

        Ok(())
    }

    fn sub_list(&self) -> AppResult<()> {
        let state = self.load_state()?;

        if self.renderer.is_json() {
            self.renderer.print_json(&JsonOutput::success(
                "sub list",
                json!({
                    "profiles": state.profiles,
                    "active_profile_id": state.active_profile_id,
                }),
            ));
            return Ok(());
        }

        if state.profiles.is_empty() {
            ui::print_info("No subscriptions");
            ui::print_hint("run: valsb sub add <url>");
            return Ok(());
        }

        ui::print_header("Subscriptions");
        println!();

        for (i, p) in state.profiles.iter().enumerate() {
            let is_active = state.active_profile_id.as_deref() == Some(&p.id);
            let mark = if is_active {
                format!(" {}", console::style("*").cyan().bold())
            } else {
                String::new()
            };
            let name_style = if is_active {
                console::style(&p.remark).cyan().bold()
            } else {
                console::style(&p.remark).bold()
            };
            println!("  [{i}] {name_style}{mark}");
            println!(
                "      {}",
                console::style(truncate_url(&p.subscription_url, 60)).dim()
            );
            let time = p.last_update_at.map_or_else(
                || console::style("-".to_string()).dim(),
                |t| console::style(t.format("%Y-%m-%d %H:%M").to_string()).green(),
            );
            println!(
                "      {}  {}    {}  {}",
                console::style("Updated").fg(console::Color::Color256(245)),
                time,
                console::style("Nodes").fg(console::Color::Color256(245)),
                console::style(p.node_count).white().bold()
            );
            if i < state.profiles.len() - 1 {
                println!();
            }
        }

        println!();
        ui::print_hint("run: valsb sub use <index|remark>    to switch active subscription");
        ui::print_hint("run: valsb sub remove <index|remark> to remove a subscription");

        Ok(())
    }

    async fn sub_update(&self, target: Option<&str>) -> AppResult<()> {
        let state = self.load_state()?;

        if state.profiles.is_empty() {
            return Err(AppError::user_with_hint(
                "no subscriptions registered",
                "run `valsb sub add <url>` first",
            ));
        }

        let targets: Vec<(String, String, String)> = if let Some(t) = target {
            let idx = state.resolve_target(t).ok_or_else(|| {
                AppError::user_with_hint(
                    format!("subscription not found: {t}"),
                    "run `valsb sub list` to see available subscriptions",
                )
            })?;
            let p = &state.profiles[idx];
            vec![(p.id.clone(), p.remark.clone(), p.subscription_url.clone())]
        } else {
            state
                .profiles
                .iter()
                .map(|p| (p.id.clone(), p.remark.clone(), p.subscription_url.clone()))
                .collect()
        };

        let (results, latest_active_config) = self.fetch_all_targets(&targets).await?;
        let updated_count = results.iter().filter(|r| r["status"] == "success").count();
        let failed_count = results.len() - updated_count;

        let mut reloaded_result = false;
        let mut apply_error: Option<String> = None;
        if let Some(cfg) = latest_active_config {
            match self.apply_config_and_reload(&cfg) {
                Ok(r) => reloaded_result = r,
                Err(e) => {
                    apply_error = Some(e.to_string());
                    if !self.renderer.is_json() {
                        ui::print_warn(&format!("Config apply failed: {e}"));
                    }
                }
            }
        }

        if self.renderer.is_json() {
            let mut payload = json!({
                "updated_count": updated_count,
                "failed_count": failed_count,
                "results": results,
                "reloaded": reloaded_result,
            });
            if let Some(ref err) = apply_error {
                payload["apply_error"] = json!(err);
            }
            self.renderer
                .print_json(&JsonOutput::success("sub update", payload));
        }

        Ok(())
    }

    async fn fetch_all_targets(
        &self,
        targets: &[(String, String, String)],
    ) -> AppResult<(Vec<serde_json::Value>, Option<serde_json::Value>)> {
        let sb_version = self.sing_box_version();
        let active_id = self.load_state()?.active_profile_id;
        let mut results: Vec<serde_json::Value> = Vec::new();
        let mut latest_active_config: Option<serde_json::Value> = None;

        for (id, remark, url) in targets {
            let sp = self.maybe_spinner(&format!("Updating {remark}..."));

            let fetch_result = subscription::fetch_subscription(url, &sb_version)
                .await
                .and_then(|content| subscription::parse_subscription_content(&content));

            match fetch_result {
                Ok(data) => {
                    config::save_raw_config(&self.paths, id, &data.raw_config)?;
                    self.mark_profile_update(
                        id,
                        UpdateStatus::Success,
                        None,
                        Some(data.nodes.len()),
                    )?;

                    if let Some(ref pb) = sp {
                        ui::finish_ok(pb, &format!("{remark}: {} nodes", data.nodes.len()));
                    }
                    results.push(json!({"remark": remark, "status": "success", "node_count": data.nodes.len()}));

                    if active_id.as_deref() == Some(id.as_str()) {
                        let mut st = self.load_state()?;
                        st.clash_api_addr = Some(data.clash_api_addr.clone());
                        self.save_state(&st)?;
                        latest_active_config = Some(data.raw_config);
                    }
                }
                Err(e) => {
                    self.mark_profile_update(id, UpdateStatus::Failed, Some(e.to_string()), None)?;
                    if let Some(ref pb) = sp {
                        ui::finish_fail(pb, &format!("{remark}: {e}"));
                    }
                    results.push(
                        json!({"remark": remark, "status": "failed", "error": e.to_string()}),
                    );
                }
            }
        }

        Ok((results, latest_active_config))
    }

    fn sub_remove(&self, target: &str) -> AppResult<()> {
        let mut state = self.load_state()?;
        let idx = state.resolve_target(target).ok_or_else(|| {
            AppError::user_with_hint(
                format!("subscription not found: {target}"),
                "run `valsb sub list` to see available subscriptions",
            )
        })?;

        let profile = &state.profiles[idx];
        let is_active = state.active_profile_id.as_deref() == Some(&profile.id);
        let remark = profile.remark.clone();
        let profile_id = profile.id.clone();

        self.confirm_subscription_removal(&remark, is_active)?;

        state.profiles.remove(idx);
        if is_active {
            state.active_profile_id = state.profiles.first().map(|p| p.id.clone());
            state.clash_api_addr = None;
        } else if state.profiles.is_empty() {
            state.clash_api_addr = None;
        }

        let cache_file = self
            .paths
            .subscription_cache_dir()
            .join(format!("{profile_id}.json"));
        let _ = std::fs::remove_file(&cache_file);

        self.save_state(&state)?;

        if self.renderer.is_json() {
            self.renderer.print_json(&JsonOutput::success(
                "sub remove",
                json!({
                    "removed": { "id": profile_id, "remark": remark },
                    "was_active": is_active,
                }),
            ));
        } else {
            ui::print_ok(&format!("Removed: {remark}"));
        }

        Ok(())
    }

    // ── node ──────────────────────────────────────────────────────────

    async fn cmd_node(&self, sub: NodeCommands) -> AppResult<()> {
        let (raw_config, fallback_remark) = self.load_active_raw_config_for_node()?;
        if let Some(remark) = fallback_remark.filter(|_| !self.renderer.is_json()) {
            ui::print_warn(&format!(
                "Active subscription cache was missing, switched to {remark}"
            ));
        }
        let mut groups = extract_groups_from_config(&raw_config);
        self.sync_groups_with_clash_api(&mut groups).await?;

        if self.renderer.is_json() {
            match sub {
                NodeCommands::Use { target: Some(t) } => {
                    let selectors: Vec<&OutboundGroup> = groups
                        .iter()
                        .filter(|g| g.group_type == "selector")
                        .collect();
                    let (gtag, ntag) = resolve_node_target_in_groups(&selectors, &t)?;
                    return self.apply_node_switch(&gtag, &ntag).await;
                }
                NodeCommands::Use { target: None } => {
                    self.renderer
                        .print_json(&JsonOutput::success("node list", &groups));
                    return Ok(());
                }
            }
        }

        match sub {
            NodeCommands::Use { target: Some(t) } => {
                let selectors: Vec<&OutboundGroup> = groups
                    .iter()
                    .filter(|g| g.group_type == "selector")
                    .collect();
                let (gtag, ntag) = resolve_node_target_in_groups(&selectors, &t)?;
                self.apply_node_switch(&gtag, &ntag).await
            }
            NodeCommands::Use { target: None } => self.interactive_node_browser(&groups).await,
        }
    }

    async fn interactive_node_browser(&self, groups: &[OutboundGroup]) -> AppResult<()> {
        let selector_groups: Vec<&OutboundGroup> = groups
            .iter()
            .filter(|g| g.group_type == "selector")
            .collect();
        if selector_groups.is_empty() {
            return Err(AppError::data("no selector groups found in configuration"));
        }

        let service_running = self.is_service_running();
        if !service_running {
            return Err(AppError::user_with_hint(
                "sing-box is not running",
                "start it first with `valsb start`, then switch nodes via Clash API",
            ));
        }
        let mut delays: Option<std::collections::HashMap<String, u32>> = None;

        loop {
            let group = if selector_groups.len() == 1 {
                selector_groups[0]
            } else {
                let items: Vec<String> = selector_groups
                    .iter()
                    .map(|g| {
                        let current = g.current.as_deref().unwrap_or("-");
                        let count = g.members.len();
                        format!("{:<14} ({current})  [{count} nodes]", g.tag)
                    })
                    .collect();

                let Ok(idx) = dialoguer::FuzzySelect::with_theme(&ui::select_theme())
                    .with_prompt("Select group:")
                    .items(&items)
                    .default(0)
                    .interact()
                else {
                    return Ok(());
                };

                selector_groups[idx]
            };

            match self
                .browse_nodes(group, &mut delays, service_running)
                .await?
            {
                NodeBrowseResult::Select(node) => {
                    return self.apply_node_switch(&group.tag, &node).await;
                }
                NodeBrowseResult::Back => {
                    if selector_groups.len() == 1 {
                        return Ok(());
                    }
                    continue;
                }
            }
        }
    }

    async fn browse_nodes(
        &self,
        group: &OutboundGroup,
        delays: &mut Option<std::collections::HashMap<String, u32>>,
        service_running: bool,
    ) -> AppResult<NodeBrowseResult> {
        use crossterm::event::{KeyCode, KeyModifiers};
        use std::io::Write;
        use tokio::sync::mpsc;

        let mut stderr = std::io::stderr();
        let total = group.members.len();
        if total == 0 {
            ui::print_info("No nodes in this group");
            return Ok(NodeBrowseResult::Back);
        }

        let initial_cursor: usize = group
            .current
            .as_ref()
            .and_then(|c| group.members.iter().position(|m| m == c))
            .unwrap_or(0);

        let mut cursor_pos: usize = initial_cursor;
        let mut prev_line_count: usize = 0;
        let mut filter = String::new();
        let mut filtered_indices: Vec<usize> = (0..total).collect();
        let mut spinner_step: usize = 0;
        let mut delay_loading = false;
        let mut delay_rx = None;

        if service_running {
            if let Ok(client) = self.clash_client() {
                let group_tag = group.tag.clone();
                let (tx, rx) = mpsc::unbounded_channel();
                tokio::spawn(async move {
                    if let Ok(cached) = client.fetch_all_delays().await {
                        let _ = tx.send(cached);
                    }
                    if let Ok(fresh) = client.test_group_delay(&group_tag).await {
                        let _ = tx.send(fresh);
                    }
                });
                delay_loading = true;
                delay_rx = Some(rx);
            }
        }

        loop {
            if let Some(rx) = delay_rx.as_mut() {
                loop {
                    match rx.try_recv() {
                        Ok(update) => {
                            let map = delays.get_or_insert_with(std::collections::HashMap::new);
                            map.extend(update);
                        }
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            delay_loading = false;
                            delay_rx = None;
                            break;
                        }
                    }
                }
            }

            let fcount = filtered_indices.len();
            let viewport = terminal_height().saturating_sub(5).max(5);
            let scroll_offset = cursor_pos.saturating_sub(viewport - 1);
            let visible_end = (scroll_offset + viewport).min(fcount);

            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("  {}", console::style(&group.tag).bold()));

            if filter.is_empty() {
                lines.push(String::new());
            } else {
                lines.push(format!(
                    "  {} {}  {}",
                    console::style("⌕").cyan(),
                    console::style(&filter).cyan().bold(),
                    console::style(format!("({fcount}/{total})")).dim()
                ));
            }

            let term_width = crossterm::terminal::size()
                .map(|(w, _)| w as usize)
                .unwrap_or(80);

            for (vi, &real_idx) in filtered_indices
                .iter()
                .enumerate()
                .take(visible_end)
                .skip(scroll_offset)
            {
                let member = &group.members[real_idx];
                let is_cursor = vi == cursor_pos;
                let is_active = group.current.as_deref() == Some(member.as_str());

                let lat = match delays
                    .as_ref()
                    .and_then(|d| d.get(member.as_str()))
                    .copied()
                    .and_then(|ms| if ms > 0 { Some(ms) } else { None })
                {
                    Some(ms) => format!("  {}", ui::format_latency(ms)),
                    None if delay_loading => {
                        let frame = ui::spinner_frame(spinner_step);
                        format!("  {}", console::style(frame).dim())
                    }
                    None => String::new(),
                };

                let mark = if is_active {
                    format!(" {}", console::style("*").cyan().bold())
                } else {
                    String::new()
                };

                let node_text = format!("{member}{lat}{mark}");
                if is_cursor {
                    lines.push(format!(
                        "  {} {}",
                        console::style("▸").cyan(),
                        console::style(node_text).cyan().bold()
                    ));
                } else {
                    lines.push(format!("    {node_text}"));
                }
            }

            if scroll_offset > 0 || visible_end < fcount {
                lines.push(
                    console::style(format!(
                        "  ({}-{} of {fcount})",
                        scroll_offset + 1,
                        visible_end
                    ))
                    .dim()
                    .to_string(),
                );
            }

            lines.push(String::new());
            lines.push(
                console::style("  ↑↓ navigate  ⏎ select  type to filter  esc back")
                    .dim()
                    .to_string(),
            );

            // Move cursor up to overwrite previous frame (no clear = no flicker)
            if prev_line_count > 0 {
                write!(stderr, "\x1b[{}A\r", prev_line_count)?;
            }
            // Write each line, clearing to end of line to erase leftover chars
            for line in &lines {
                writeln!(stderr, "{line}\x1b[K")?;
            }
            // If previous frame had more lines, blank the extras
            for _ in lines.len()..prev_line_count {
                writeln!(stderr, "\x1b[K")?;
            }
            // Move cursor back up if we wrote extra blank lines
            if prev_line_count > lines.len() {
                let extra = prev_line_count - lines.len();
                write!(stderr, "\x1b[{}A", extra)?;
            }
            stderr.flush()?;
            let _ = term_width; // used above for future truncation
            prev_line_count = lines.len();
            spinner_step = spinner_step.wrapping_add(1);

            let poll_timeout = if delay_loading {
                std::time::Duration::from_millis(80)
            } else {
                std::time::Duration::from_secs(5)
            };

            if let Some(key) = poll_key(poll_timeout)? {
                match key.code {
                    KeyCode::Up => {
                        cursor_pos = cursor_pos.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if cursor_pos + 1 < fcount {
                            cursor_pos += 1;
                        }
                    }
                    KeyCode::Home => cursor_pos = 0,
                    KeyCode::End => {
                        if fcount > 0 {
                            cursor_pos = fcount - 1;
                        }
                    }
                    KeyCode::Enter => {
                        erase_lines(&mut stderr, prev_line_count)?;
                        if fcount > 0 {
                            let real_idx = filtered_indices[cursor_pos];
                            return Ok(NodeBrowseResult::Select(group.members[real_idx].clone()));
                        }
                    }
                    KeyCode::Esc => {
                        if !filter.is_empty() {
                            filter.clear();
                            filtered_indices = (0..total).collect();
                            cursor_pos =
                                initial_cursor.min(filtered_indices.len().saturating_sub(1));
                        } else {
                            erase_lines(&mut stderr, prev_line_count)?;
                            return Ok(NodeBrowseResult::Back);
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        erase_lines(&mut stderr, prev_line_count)?;
                        return Ok(NodeBrowseResult::Back);
                    }
                    KeyCode::Backspace => {
                        if !filter.is_empty() {
                            filter.pop();
                            apply_node_filter(&group.members, &filter, &mut filtered_indices);
                            cursor_pos = cursor_pos.min(filtered_indices.len().saturating_sub(1));
                        }
                    }
                    KeyCode::Char(c) if !c.is_control() => {
                        filter.push(c);
                        apply_node_filter(&group.members, &filter, &mut filtered_indices);
                        cursor_pos = 0;
                    }
                    _ => {}
                }
            }
        }
    }

    async fn apply_node_switch(&self, group_tag: &str, node_tag: &str) -> AppResult<()> {
        self.switch_via_clash_api(group_tag, node_tag).await
    }

    fn is_service_running(&self) -> bool {
        self.get_service_manager()
            .ok()
            .and_then(|m| m.is_active().ok())
            .unwrap_or(false)
    }

    async fn switch_via_clash_api(&self, group_tag: &str, node_tag: &str) -> AppResult<()> {
        if !self.is_service_running() {
            return Err(AppError::user_with_hint(
                "sing-box is not running",
                "start it first with `valsb start`, then switch nodes via Clash API",
            ));
        }

        let client = self.clash_client().map_err(|e| {
            AppError::runtime_with_hint(
                format!("failed to initialize Clash API client: {e}"),
                "check `valsb status` and `valsb logs`",
            )
        })?;

        if self.renderer.is_json() {
            client.select_proxy(group_tag, node_tag).await?;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let ip_info = crate::ip::detect_exit_ip().await;
            self.renderer.print_json(&JsonOutput::success(
                "node use",
                json!({
                    "group": group_tag,
                    "node": node_tag,
                    "exit_ip": ip_info.as_ref().map(|i| &i.ip),
                    "location": ip_info.as_ref().map(|i| json!({"country": i.country, "city": i.city})),
                }),
            ));
            return Ok(());
        }

        let sp = ui::spinner("Switching node...");
        client.select_proxy(group_tag, node_tag).await?;
        ui::finish_ok(&sp, "Node switched");

        println!();
        ui::print_detail("Group", group_tag);
        ui::print_detail("Node", node_tag);

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Some(ip_info) = crate::ip::detect_exit_ip().await {
            ui::print_detail("Exit IP", &ip_info.ip);
            ui::print_detail("Location", &ip_info.location_display());
        }

        Ok(())
    }

    // ── install ───────────────────────────────────────────────────────

    async fn cmd_install(&self) -> AppResult<()> {
        config::init_config(&self.paths)?;

        let (version, target_bin) = self.download_and_install_kernel().await?;

        let mgr = self.get_service_manager()?;
        mgr.install()?;
        if !self.renderer.is_json() {
            ui::print_ok("Service unit installed");
        }

        let manifest = self.build_manifest(Some(version.clone()));
        install::save_manifest(&self.paths.manifest_file(), &manifest)?;

        if self.renderer.is_json() {
            self.renderer.print_json(&JsonOutput::success(
                "install",
                json!({
                    "sing_box_version": version,
                    "path": target_bin.to_string_lossy(),
                }),
            ));
        }

        Ok(())
    }

    async fn download_and_install_kernel(&self) -> AppResult<(String, std::path::PathBuf)> {
        let arch = match self.platform.arch {
            crate::platform::Arch::Amd64 => "amd64",
            crate::platform::Arch::Arm64 => "arm64",
        };

        let (os_name, ext) = match self.platform.os_family {
            crate::platform::OsFamily::MacOS => ("darwin", "tar.gz"),
            crate::platform::OsFamily::Windows => ("windows", "zip"),
            _ => ("linux", "tar.gz"),
        };

        let sp_ver = self.maybe_spinner("Fetching latest sing-box version...");

        let client = reqwest::Client::builder()
            .user_agent("valsb-cli")
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| AppError::network(format!("HTTP client error: {e}")))?;

        let resp: serde_json::Value = client
            .get("https://api.github.com/repos/SagerNet/sing-box/releases/latest")
            .send()
            .await
            .map_err(|e| {
                AppError::network_with_hint(
                    format!("failed to fetch latest release info: {e}"),
                    "check network connectivity",
                )
            })?
            .json()
            .await
            .map_err(|e| AppError::network(format!("failed to parse release JSON: {e}")))?;

        let version = resp["tag_name"]
            .as_str()
            .ok_or_else(|| AppError::data("no tag_name in release response"))?
            .trim_start_matches('v')
            .to_string();

        if let Some(ref pb) = sp_ver {
            ui::finish_ok(pb, &format!("Latest: sing-box {version}"));
        }

        let sp_dl = self.maybe_spinner(&format!("Downloading sing-box {version} ({arch})..."));
        let filename = format!("sing-box-{version}-{os_name}-{arch}.{ext}");
        let download_url =
            format!("https://github.com/SagerNet/sing-box/releases/download/v{version}/{filename}");

        let tmp_dir = tempfile::tempdir()?;
        let archive_path = tmp_dir.path().join(&filename);

        let bytes = client
            .get(&download_url)
            .send()
            .await
            .map_err(|e| {
                AppError::network_with_hint(
                    format!("failed to download: {e}"),
                    "check network connectivity",
                )
            })?
            .bytes()
            .await
            .map_err(|e| AppError::network(format!("download error: {e}")))?;

        std::fs::write(&archive_path, &bytes)?;

        let extract_dir = tmp_dir.path().join("extracted");
        std::fs::create_dir_all(&extract_dir)?;

        extract_archive(&archive_path, &extract_dir)?;

        let extracted_bin =
            find_binary_recursive(&extract_dir, crate::platform::sing_box_bin_name())?;

        std::fs::create_dir_all(&self.paths.sing_box_bin_dir)?;
        let target_bin = self.paths.sing_box_binary();
        std::fs::copy(&extracted_bin, &target_bin)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target_bin, std::fs::Permissions::from_mode(0o755))?;
        }

        if let Some(ref pb) = sp_dl {
            ui::finish_ok(pb, &format!("Installed: {}", target_bin.display()));
        }

        Ok((version, target_bin))
    }

    // ── update ───────────────────────────────────────────────────────

    async fn cmd_update(&self) -> AppResult<()> {
        let current_valsb = env!("CARGO_PKG_VERSION");
        let current_sb = self.sing_box_version();

        let sp = self.maybe_spinner("Checking for updates...");

        let client = http_client()?;
        let (latest_valsb, latest_sb) = tokio::try_join!(
            fetch_latest_release_version(&client, "nsevo/val-sing-box-cli"),
            fetch_latest_release_version(&client, "SagerNet/sing-box"),
        )?;

        if let Some(ref pb) = sp {
            ui::finish_ok(pb, "Version check complete");
        }

        let need_valsb = latest_valsb != current_valsb;
        let need_sb = latest_sb != current_sb && current_sb != "unknown";

        if self.renderer.is_json() {
            if !need_valsb && !need_sb {
                self.renderer.print_json(&JsonOutput::success(
                    "update",
                    json!({"up_to_date": true, "valsb": current_valsb, "sing_box": current_sb}),
                ));
                return Ok(());
            }
        } else if !need_valsb && !need_sb {
            ui::print_ok("Already up to date");
            ui::print_kv("valsb", current_valsb);
            ui::print_kv("sing-box", &current_sb);
            return Ok(());
        }

        if !self.renderer.is_json() {
            println!();
            if need_valsb {
                ui::print_kv(
                    "valsb",
                    &format!(
                        "{} {} {}",
                        console::style(current_valsb).dim(),
                        console::style("→").dim(),
                        console::style(&latest_valsb).green().bold()
                    ),
                );
            }
            if need_sb {
                ui::print_kv(
                    "sing-box",
                    &format!(
                        "{} {} {}",
                        console::style(&current_sb).dim(),
                        console::style("→").dim(),
                        console::style(&latest_sb).green().bold()
                    ),
                );
            }
            println!();
        }

        if !self.yes && !self.renderer.is_json() {
            let confirm = dialoguer::Confirm::with_theme(&ui::select_theme())
                .with_prompt("Proceed with update?")
                .default(true)
                .interact()
                .map_err(|e| AppError::runtime(format!("cancelled: {e}")))?;
            if !confirm {
                return Ok(());
            }
        }

        let arch = match self.platform.arch {
            crate::platform::Arch::Amd64 => "amd64",
            crate::platform::Arch::Arm64 => "arm64",
        };
        let (os_name, ext) = match self.platform.os_family {
            crate::platform::OsFamily::MacOS => ("darwin", "tar.gz"),
            crate::platform::OsFamily::Windows => ("windows", "zip"),
            _ => ("linux", "tar.gz"),
        };

        let tmp_dir = tempfile::tempdir()?;
        let mut new_sb_bin: Option<std::path::PathBuf> = None;
        let mut new_valsb_bin: Option<std::path::PathBuf> = None;

        // Download sing-box (service stays running)
        if need_sb {
            let sp = self.maybe_spinner(&format!("Downloading sing-box {latest_sb}..."));
            let bin = download_and_extract_binary(
                &client,
                &format!("https://github.com/SagerNet/sing-box/releases/download/v{latest_sb}/sing-box-{latest_sb}-{os_name}-{arch}.{ext}"),
                tmp_dir.path(),
                crate::platform::sing_box_bin_name(),
            ).await?;
            new_sb_bin = Some(bin);
            if let Some(ref pb) = sp {
                ui::finish_ok(pb, &format!("Downloaded sing-box {latest_sb}"));
            }
        }

        // Download valsb
        if need_valsb {
            let valsb_bin_name = format!("valsb{}", std::env::consts::EXE_SUFFIX);
            let sp = self.maybe_spinner(&format!("Downloading valsb {latest_valsb}..."));
            let bin = download_and_extract_binary(
                &client,
                &format!("https://github.com/nsevo/val-sing-box-cli/releases/download/v{latest_valsb}/valsb-v{latest_valsb}-{os_name}-{arch}.{ext}"),
                tmp_dir.path(),
                &valsb_bin_name,
            ).await?;
            new_valsb_bin = Some(bin);
            if let Some(ref pb) = sp {
                ui::finish_ok(pb, &format!("Downloaded valsb {latest_valsb}"));
            }
        }

        // Stop service before replacing binaries
        let was_running = self
            .get_service_manager()
            .ok()
            .and_then(|m| m.is_active().ok())
            .unwrap_or(false);

        if was_running && new_sb_bin.is_some() {
            let sp = self.maybe_spinner("Stopping service...");
            if let Ok(mgr) = self.get_service_manager() {
                mgr.stop()?;
            }
            if let Some(ref pb) = sp {
                ui::finish_ok(pb, "Service stopped");
            }
        }

        // Replace sing-box binary
        if let Some(ref src) = new_sb_bin {
            std::fs::create_dir_all(&self.paths.sing_box_bin_dir)?;
            let target = self.paths.sing_box_binary();
            std::fs::copy(src, &target)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
            }
        }

        // Replace valsb binary (self-update)
        if let Some(ref src) = new_valsb_bin {
            let target = self.paths.valsb_binary();
            replace_running_binary(src, &target)?;
        }

        // Restart service if it was running
        if was_running {
            let sp = self.maybe_spinner("Starting service...");
            if let Ok(mgr) = self.get_service_manager() {
                mgr.start()?;
            }
            if let Some(ref pb) = sp {
                ui::finish_ok(pb, "Service started");
            }
        }

        // Update manifest
        let sb_ver = if need_sb {
            Some(latest_sb.clone())
        } else {
            install::load_manifest(&self.paths.manifest_file())?.and_then(|m| m.sing_box_version)
        };
        let manifest = self.build_manifest(sb_ver);
        install::save_manifest(&self.paths.manifest_file(), &manifest)?;

        if self.renderer.is_json() {
            self.renderer.print_json(&JsonOutput::success(
                "update",
                json!({
                    "up_to_date": false,
                    "updated_valsb": need_valsb,
                    "updated_sing_box": need_sb,
                    "valsb_version": if need_valsb { &latest_valsb } else { current_valsb },
                    "sing_box_version": if need_sb { &latest_sb } else { &current_sb },
                }),
            ));
        } else {
            println!();
            ui::print_ok("Update complete");
            if need_valsb {
                ui::print_kv(
                    "valsb",
                    &console::style(&latest_valsb).green().bold().to_string(),
                );
            }
            if need_sb {
                ui::print_kv(
                    "sing-box",
                    &console::style(&latest_sb).green().bold().to_string(),
                );
            }
        }

        Ok(())
    }

    // ── uninstall ─────────────────────────────────────────────────────

    fn cmd_uninstall(&self) -> AppResult<()> {
        if !self.yes {
            return Err(AppError::user_with_hint(
                "uninstall requires confirmation",
                "run with --yes to confirm: `valsb uninstall --yes`",
            ));
        }

        let manifest = install::load_manifest(&self.paths.manifest_file())?;

        if let Some(manifest) = manifest {
            let mgr = self.get_service_manager().ok();

            let steps = uninstall::run_uninstall(&manifest.managed_paths, mgr.as_deref());

            if self.renderer.is_json() {
                self.renderer
                    .print_json(&JsonOutput::success("uninstall", json!({"steps": steps})));
                return Ok(());
            }

            for step in &steps {
                match step.status.as_str() {
                    "ok" => ui::print_ok(&step.action),
                    "skipped" => ui::print_info(&step.action),
                    _ => {
                        let msg = if let Some(ref err) = step.error {
                            format!("{}: {err}", step.action)
                        } else {
                            step.action.clone()
                        };
                        ui::print_warn(&msg);
                    }
                }
            }
            ui::print_ok("Uninstall complete");
        } else {
            if !self.renderer.is_json() {
                ui::print_warn("No manifest found. Cleaning known paths...");
            }

            if let Ok(mgr) = self.get_service_manager() {
                let _ = mgr.stop();
                let _ = mgr.uninstall();
                if !self.renderer.is_json() {
                    ui::print_ok("Service stopped and uninstalled");
                }
            }

            let valsb_bin = self.paths.valsb_binary();
            if valsb_bin.exists() {
                let _ = std::fs::remove_file(&valsb_bin);
            }

            let sb_bin = self.paths.sing_box_binary();
            if sb_bin.exists() {
                let _ = std::fs::remove_file(&sb_bin);
            }
            if let Some(parent) = sb_bin.parent() {
                let _ = std::fs::remove_dir_all(parent);
                if let Some(grandparent) = parent.parent() {
                    let _ = std::fs::remove_dir(grandparent);
                }
            }

            if self.paths.unit_file.exists() {
                let _ = std::fs::remove_file(&self.paths.unit_file);
            }

            let _ = std::fs::remove_dir_all(&self.paths.config_dir);
            let _ = std::fs::remove_dir_all(&self.paths.cache_dir);
            let _ = std::fs::remove_dir_all(&self.paths.data_dir);

            if !self.renderer.is_json() {
                ui::print_ok("Known paths cleaned");
            }
        }

        Ok(())
    }

    // ── helpers ───────────────────────────────────────────────────────

    fn maybe_spinner(&self, msg: &str) -> Option<indicatif::ProgressBar> {
        if self.renderer.is_json() {
            None
        } else {
            Some(ui::spinner(msg))
        }
    }

    fn preflight_checks(&self) -> AppResult<()> {
        let sb_bin = self.resolve_sing_box_bin();
        if let Some(ref bin) = sb_bin {
            let v = parse_sing_box_version(bin);
            ui::print_ok(&format!("sing-box kernel     v{v}"));
        } else {
            ui::print_fail("sing-box kernel     not found");
            ui::print_hint("run: valsb install");
            return Err(AppError::env("sing-box kernel not found"));
        }

        let config_path = self.paths.generated_config_file();
        if config_path.exists() {
            if let Some(ref bin) = sb_bin {
                match config::validate_config(bin, &config_path) {
                    Ok(()) => ui::print_ok("configuration       ok"),
                    Err(e) => {
                        ui::print_fail("configuration       invalid");
                        ui::print_hint(&format!("sing-box check: {e}"));
                        return Err(e);
                    }
                }
            } else {
                ui::print_ok("configuration       present");
            }
        } else {
            ui::print_fail("configuration       not found");
            ui::print_hint("run: valsb sub add <url>");
            return Err(AppError::user_with_hint(
                "no config found",
                "run `valsb sub add <url>` to add a subscription",
            ));
        }

        if self.get_service_manager().is_ok() {
            ui::print_ok("service unit        loaded");
        } else {
            ui::print_fail("service unit        not available");
            ui::print_hint("run: valsb install");
            return Err(AppError::env("no service backend available"));
        }

        Ok(())
    }

    async fn show_exit_info(&self) {
        let node = self.current_selector_node().await;
        if let Some(ref n) = node {
            ui::print_detail("Node", n);
        }

        let sp = ui::spinner("Detecting exit IP...");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        match crate::ip::detect_exit_ip().await {
            Some(ip_info) => {
                ui::finish_ok(&sp, "Exit IP detected");
                ui::print_detail("Exit IP", &ip_info.ip);
                ui::print_detail("Location", &ip_info.location_display());
            }
            None => {
                ui::finish_fail(&sp, "Exit IP detection timed out");
            }
        }
    }

    fn apply_config_and_reload(&self, raw_config: &serde_json::Value) -> AppResult<bool> {
        config::write_active_config(&self.paths, raw_config)?;

        let config_path = self.paths.generated_config_file();
        if let Some(bin) = self.resolve_sing_box_bin() {
            config::validate_config(&bin, &config_path)?;
        }

        if let Ok(mgr) = self.get_service_manager() {
            if mgr.is_active().unwrap_or(false) {
                mgr.reload()?;
                if !self.renderer.is_json() {
                    let sp = ui::spinner("Reloading sing-box...");
                    ui::finish_ok(&sp, "Configuration reloaded");
                }
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn rollback_active_profile(&self, old_active_id: Option<&str>) -> AppResult<()> {
        let mut state = self.load_state()?;
        state.active_profile_id = old_active_id.map(String::from);
        self.save_state(&state)?;

        if let Some(old_id) = old_active_id {
            if let Ok(old_config) = config::read_raw_config(&self.paths, old_id) {
                let _ = config::write_active_config(&self.paths, &old_config);
            }
        }
        Ok(())
    }

    fn prompt_subscription_url(&self) -> AppResult<String> {
        if self.renderer.is_json() {
            return Err(AppError::user_with_hint(
                "subscription URL is required in JSON mode",
                "provide the URL as an argument to `valsb sub add <url>`",
            ));
        }

        let url = dialoguer::Input::<String>::with_theme(&ui::select_theme())
            .with_prompt("Subscription URL")
            .interact_text()
            .map_err(|e| AppError::runtime(format!("input cancelled: {e}")))?;
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(AppError::user_with_hint(
                "subscription URL cannot be empty",
                "paste a full subscription URL and try again",
            ));
        }
        Ok(url)
    }

    fn confirm_subscription_removal(&self, remark: &str, is_active: bool) -> AppResult<()> {
        if self.yes {
            return Ok(());
        }

        if self.renderer.is_json() {
            return Err(AppError::user_with_hint(
                format!("removal of '{remark}' requires confirmation"),
                "re-run with --yes to confirm subscription removal",
            ));
        }

        let prompt = if is_active {
            format!("Remove active subscription '{remark}'?")
        } else {
            format!("Remove subscription '{remark}'?")
        };

        let confirm = dialoguer::Confirm::with_theme(&ui::select_theme())
            .with_prompt(prompt)
            .default(false)
            .interact()
            .map_err(|e| AppError::runtime(format!("cancelled: {e}")))?;

        if confirm {
            return Ok(());
        }

        Err(AppError::user_with_hint(
            format!("removal cancelled for '{remark}'"),
            "re-run the command and confirm, or pass --yes to skip confirmation",
        ))
    }

    fn load_active_raw_config_for_node(&self) -> AppResult<(serde_json::Value, Option<String>)> {
        let mut state = self.load_state()?;
        let active_profile = state.active_profile().cloned().ok_or_else(|| {
            AppError::user_with_hint("no active profile", "run `valsb sub add <url>` first")
        })?;

        if let Ok(raw_config) = config::read_raw_config(&self.paths, &active_profile.id) {
            return Ok((raw_config, None));
        }

        let fallback = state
            .profiles
            .iter()
            .filter(|profile| profile.id != active_profile.id)
            .find_map(|profile| {
                config::read_raw_config(&self.paths, &profile.id)
                    .ok()
                    .map(|raw_config| (profile.id.clone(), profile.remark.clone(), raw_config))
            });

        if let Some((profile_id, remark, raw_config)) = fallback {
            state.active_profile_id = Some(profile_id);
            state.clash_api_addr = None;
            self.save_state(&state)?;
            return Ok((raw_config, Some(remark)));
        }

        Err(AppError::data_with_hint(
            format!("profile '{}' has no cached config", active_profile.remark),
            "run `valsb sub update` to fetch subscription data",
        ))
    }

    fn interactive_select_profile(&self, prompt: &str) -> AppResult<String> {
        if self.renderer.is_json() {
            return Err(AppError::user_with_hint(
                "target is required in JSON mode",
                "provide the profile remark, id, or index as an argument",
            ));
        }

        let state = self.load_state()?;
        if state.profiles.is_empty() {
            return Err(AppError::user_with_hint(
                "no profiles available",
                "run `valsb sub add <url>` first",
            ));
        }

        let items: Vec<String> = state
            .profiles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let active = if state.active_profile_id.as_deref() == Some(&p.id) {
                    " *"
                } else {
                    ""
                };
                format!("[{i}] {}{active}", p.remark)
            })
            .collect();

        let selection = dialoguer::FuzzySelect::with_theme(&ui::select_theme())
            .with_prompt(prompt)
            .items(&items)
            .default(0)
            .interact()
            .map_err(|e| AppError::runtime(format!("selection cancelled: {e}")))?;

        Ok(selection.to_string())
    }
}

/// True when the command needs to read or write system state.
///
/// The only commands that are safe to run as a regular user are the ones
/// that do not touch the on-disk state, the binary install root, or the
/// service manager. Everything else is re-launched under sudo / UAC so
/// users never have to type `sudo` themselves.
fn command_requires_root(command: &Commands) -> bool {
    !matches!(
        command,
        Commands::Version | Commands::Completion { .. } | Commands::ServiceWorker { .. }
    )
}

#[cfg(not(windows))]
fn relaunch_with_sudo() -> AppResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe()
        .map_err(|e| AppError::runtime(format!("failed to locate current executable: {e}")))?;
    eprintln!(
        "  {}",
        console::style("valsb requires root; requesting sudo...").dim()
    );
    let status = std::process::Command::new("sudo")
        .arg(exe)
        .args(args)
        .status()
        .map_err(|e| AppError::runtime(format!("failed to invoke sudo: {e}")))?;
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(windows)]
fn relaunch_as_admin() -> AppResult<()> {
    let exe = std::env::current_exe()
        .map_err(|e| AppError::runtime(format!("failed to locate current executable: {e}")))?;
    let cwd = std::env::current_dir()
        .map_err(|e| AppError::runtime(format!("failed to resolve working directory: {e}")))?;

    let host = std::env::var("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");

    let escaped_exe = exe.to_string_lossy().replace('\'', "''");
    let escaped_cwd = cwd.to_string_lossy().replace('\'', "''");

    let mut command = String::from("$ErrorActionPreference = 'Stop'\r\n");
    command.push_str(&format!("$exe = '{escaped_exe}'\r\n"));
    command.push_str(&format!("$cwd = '{escaped_cwd}'\r\n"));
    command.push_str("$argsList = @(\r\n");
    for arg in std::env::args().skip(1) {
        let escaped = arg.replace('\'', "''");
        command.push_str(&format!("  '{escaped}'\r\n"));
    }
    command.push_str(")\r\n");
    command.push_str("$proc = Start-Process -FilePath $exe -Verb RunAs -WorkingDirectory $cwd -ArgumentList $argsList -Wait -PassThru\r\n");
    command.push_str("exit $proc.ExitCode\r\n");

    eprintln!(
        "  {}",
        console::style("valsb requires Administrator; requesting UAC prompt...").dim()
    );
    let status = Command::new(host)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &command,
        ])
        .status()
        .map_err(|e| {
            AppError::runtime(format!("failed to request Administrator privileges: {e}"))
        })?;

    std::process::exit(status.code().unwrap_or(1));
}

// ── Data types ────────────────────────────────────────────────────────

enum NodeBrowseResult {
    Select(String),
    Back,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutboundGroup {
    pub tag: String,
    pub group_type: String,
    pub members: Vec<String>,
    pub current: Option<String>,
}

// ── Free functions ────────────────────────────────────────────────────

fn erase_lines(w: &mut impl std::io::Write, n: usize) -> std::io::Result<()> {
    if n > 0 {
        write!(w, "\x1b[{}A\r", n)?;
        for _ in 0..n {
            writeln!(w, "\x1b[K")?;
        }
        write!(w, "\x1b[{}A\r", n)?;
        w.flush()?;
    }
    Ok(())
}

fn terminal_height() -> usize {
    crossterm::terminal::size()
        .map(|(_, h)| h as usize)
        .unwrap_or(24)
}

/// Read a key press with timeout. Temporarily enables raw mode for input capture.
fn poll_key(timeout: std::time::Duration) -> std::io::Result<Option<crossterm::event::KeyEvent>> {
    crossterm::terminal::enable_raw_mode()?;
    let result = (|| {
        if !crossterm::event::poll(timeout)? {
            return Ok(None);
        }
        match crossterm::event::read()? {
            crossterm::event::Event::Key(key)
                if key.kind == crossterm::event::KeyEventKind::Press =>
            {
                Ok(Some(key))
            }
            _ => Ok(None),
        }
    })();
    let _ = crossterm::terminal::disable_raw_mode();
    result
}

fn apply_node_filter(members: &[String], query: &str, out: &mut Vec<usize>) {
    out.clear();
    if query.is_empty() {
        out.extend(0..members.len());
        return;
    }
    let q = query.to_lowercase();
    for (i, m) in members.iter().enumerate() {
        if m.to_lowercase().contains(&q) {
            out.push(i);
        }
    }
}

fn extract_groups_from_config(config: &serde_json::Value) -> Vec<OutboundGroup> {
    let Some(outbounds) = config.get("outbounds").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut groups = Vec::new();
    for ob in outbounds {
        let ob_type = ob.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if ob_type != "selector" && ob_type != "urltest" {
            continue;
        }
        let tag = ob
            .get("tag")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let members: Vec<String> = ob
            .get("outbounds")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let current = ob.get("default").and_then(|v| v.as_str()).map(String::from);

        groups.push(OutboundGroup {
            tag,
            group_type: ob_type.to_string(),
            members,
            current,
        });
    }
    groups
}

fn resolve_node_target_in_groups(
    groups: &[&OutboundGroup],
    target: &str,
) -> AppResult<(String, String)> {
    let mut matches: Vec<(String, String)> = Vec::new();
    for g in groups {
        if g.members.iter().any(|m| m == target) {
            matches.push((g.tag.clone(), target.to_string()));
        }
    }

    match matches.len() {
        0 => Err(AppError::user_with_hint(
            format!("node not found: {target}"),
            "run `valsb node list` to see available nodes",
        )),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(AppError::user_with_hint(
            format!("node '{target}' exists in multiple groups"),
            "use interactive selection: `valsb node use`",
        )),
    }
}

fn clear_group_currents(groups: &mut [OutboundGroup]) {
    for group in groups {
        group.current = None;
    }
}

fn apply_clash_proxy_groups(
    groups: &mut [OutboundGroup],
    proxies: &std::collections::HashMap<String, crate::clash::ProxyGroupStatus>,
) {
    for group in groups {
        if let Some(proxy) = proxies.get(&group.tag) {
            group.current = proxy.current.clone();
            if !proxy.members.is_empty() {
                group.members = proxy.members.clone();
            }
        }
    }
}

fn parse_sing_box_version(bin: &std::path::Path) -> String {
    std::process::Command::new(bin)
        .arg("version")
        .output()
        .ok()
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(2))
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        return url.to_string();
    }
    let end = max_len.saturating_sub(3);
    let truncated: String = url.chars().take(end).collect();
    format!("{truncated}...")
}

// ── Update helpers ────────────────────────────────────────────────────

fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("valsb-cli")
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::network(format!("HTTP client error: {e}")))
}

async fn fetch_latest_release_version(client: &reqwest::Client, repo: &str) -> AppResult<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            AppError::network_with_hint(
                format!("failed to fetch {repo} release info: {e}"),
                "check network connectivity",
            )
        })?
        .json()
        .await
        .map_err(|e| AppError::network(format!("failed to parse release JSON: {e}")))?;

    resp["tag_name"]
        .as_str()
        .ok_or_else(|| AppError::data(format!("no tag_name in {repo} release response")))
        .map(|s| s.trim_start_matches('v').to_string())
}

async fn download_and_extract_binary(
    client: &reqwest::Client,
    url: &str,
    tmp_dir: &std::path::Path,
    binary_name: &str,
) -> AppResult<std::path::PathBuf> {
    let mut resp = client.get(url).send().await.map_err(|e| {
        AppError::network_with_hint(
            format!("download failed: {e}"),
            "check network connectivity",
        )
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let detail = if body.trim().is_empty() {
            format!("HTTP {status}")
        } else {
            body
        };
        return Err(AppError::network_with_hint(
            format!("download failed: {detail}"),
            "check the release asset URL or retry later",
        ));
    }

    let filename = url.rsplit('/').next().unwrap_or("archive");
    let archive_path = tmp_dir.join(filename);
    let mut archive_file = std::fs::File::create(&archive_path)?;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| AppError::network(format!("download error: {e}")))?
    {
        std::io::Write::write_all(&mut archive_file, &chunk)?;
    }

    let extract_dir = tmp_dir.join(format!("extract-{}", binary_name.replace('.', "-")));
    std::fs::create_dir_all(&extract_dir)?;

    extract_archive(&archive_path, &extract_dir)?;

    find_binary_recursive(&extract_dir, binary_name)
}

fn extract_archive(archive_path: &std::path::Path, extract_dir: &std::path::Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        let is_zip = archive_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
        if is_zip {
            let status = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    "Expand-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force",
                ])
                .arg(archive_path)
                .arg(extract_dir)
                .status()
                .map_err(|e| AppError::runtime(format!("failed to extract zip archive: {e}")))?;
            if !status.success() {
                return Err(AppError::runtime("zip archive extraction failed"));
            }
            return Ok(());
        }
    }

    let tar_status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(archive_path)
        .arg("-C")
        .arg(extract_dir)
        .status()
        .map_err(|e| AppError::runtime(format!("failed to extract archive: {e}")))?;

    if !tar_status.success() {
        return Err(AppError::runtime("archive extraction failed"));
    }

    Ok(())
}

fn find_binary_recursive(dir: &std::path::Path, name: &str) -> AppResult<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|f| f == name) {
            return Ok(path);
        }
        if path.is_dir() {
            if let Ok(found) = find_binary_recursive(&path, name) {
                return Ok(found);
            }
        }
    }
    Err(AppError::data(format!(
        "{name} not found in extracted archive"
    )))
}

/// Replace a binary that may currently be running.
/// Unix: unlink first (kernel keeps old inode for running process), then copy.
/// Windows: rename the running exe out of the way first, then copy new.
fn replace_running_binary(src: &std::path::Path, target: &std::path::Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        let old = target.with_extension("exe.old");
        let _ = std::fs::remove_file(&old);
        if target.exists() {
            std::fs::rename(target, &old)?;
        }
        std::fs::copy(src, target)?;
        let _ = std::fs::remove_file(&old);
    }
    #[cfg(not(windows))]
    {
        let _ = std::fs::remove_file(target);
        std::fs::copy(src, target)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_url_short() {
        assert_eq!(truncate_url("https://a.com", 50), "https://a.com");
    }

    #[test]
    fn test_truncate_url_long() {
        let url = "https://very-long-url-that-exceeds-the-max-length.example.com/path/to/resource";
        let result = truncate_url(url, 30);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 30);
    }

    #[test]
    fn test_extract_groups_from_config() {
        let config = json!({
            "outbounds": [
                {"type": "selector", "tag": "Proxy", "outbounds": ["Auto", "HK-1", "JP-1"], "default": "HK-1"},
                {"type": "urltest", "tag": "Auto", "outbounds": ["HK-1", "JP-1"]},
                {"type": "hysteria2", "tag": "HK-1"},
                {"type": "hysteria2", "tag": "JP-1"},
            ]
        });
        let groups = extract_groups_from_config(&config);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].tag, "Proxy");
        assert_eq!(groups[0].group_type, "selector");
        assert_eq!(groups[0].current, Some("HK-1".to_string()));
        assert_eq!(groups[1].tag, "Auto");
        assert_eq!(groups[1].group_type, "urltest");
    }

    #[test]
    fn test_apply_clash_proxy_groups_prefers_live_current_node() {
        let mut groups = vec![
            OutboundGroup {
                tag: "Proxy".to_string(),
                group_type: "selector".to_string(),
                members: vec!["Auto".to_string(), "HK-1".to_string()],
                current: Some("Auto".to_string()),
            },
            OutboundGroup {
                tag: "Auto".to_string(),
                group_type: "urltest".to_string(),
                members: vec!["HK-1".to_string()],
                current: None,
            },
        ];
        let proxies = std::collections::HashMap::from([
            (
                "Proxy".to_string(),
                crate::clash::ProxyGroupStatus {
                    current: Some("HK-1".to_string()),
                    members: vec!["Auto".to_string(), "HK-1".to_string(), "JP-1".to_string()],
                },
            ),
            (
                "Auto".to_string(),
                crate::clash::ProxyGroupStatus {
                    current: Some("JP-1".to_string()),
                    members: vec!["HK-1".to_string(), "JP-1".to_string()],
                },
            ),
        ]);

        apply_clash_proxy_groups(&mut groups, &proxies);

        assert_eq!(groups[0].current.as_deref(), Some("HK-1"));
        assert_eq!(groups[0].members, vec!["Auto", "HK-1", "JP-1"]);
        assert_eq!(groups[1].current.as_deref(), Some("JP-1"));
    }

    #[test]
    fn version_and_completion_skip_root_elevation() {
        assert!(!command_requires_root(&Commands::Version));
        assert!(!command_requires_root(&Commands::Completion {
            shell: clap_complete::Shell::Bash,
        }));
    }

    #[test]
    fn state_and_service_commands_require_root() {
        assert!(command_requires_root(&Commands::Install));
        assert!(command_requires_root(&Commands::Update));
        assert!(command_requires_root(&Commands::Uninstall));
        assert!(command_requires_root(&Commands::Status));
        assert!(command_requires_root(&Commands::Start));
    }
}
