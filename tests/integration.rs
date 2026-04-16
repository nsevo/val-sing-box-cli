use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;

fn valsb() -> Command {
    Command::cargo_bin("valsb").unwrap()
}

fn valsb_with_temp_config() -> (tempfile::TempDir, Command) {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let home_dir = tmp.path().join("home");
    let xdg_config = tmp.path().join("xdg-config");
    let xdg_cache = tmp.path().join("xdg-cache");
    let xdg_data = tmp.path().join("xdg-data");
    let mut cmd = valsb();
    cmd.env("HOME", &home_dir)
        .env("XDG_CONFIG_HOME", &xdg_config)
        .env("XDG_CACHE_HOME", &xdg_cache)
        .env("XDG_DATA_HOME", &xdg_data)
        .arg("--config-dir")
        .arg(config_dir);
    (tmp, cmd)
}

#[test]
fn test_help_output() {
    valsb()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "A modern CLI for managing sing-box",
        ));
}

#[test]
fn test_help_contains_flat_commands() {
    valsb()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("restart"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("reload"))
        .stdout(predicate::str::contains("logs"));
}

#[test]
fn test_version_json() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.args(["version", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("\"valsb_version\""))
        .stdout(predicate::str::contains("\"platform\""));
}

#[test]
fn test_doctor_json() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.args(["doctor", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("\"command\": \"doctor\""));
}

#[test]
fn test_config_init() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");

    valsb()
        .args(["config", "init", "--config-dir"])
        .arg(config_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized"));
}

#[test]
fn test_config_path_json() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");

    valsb()
        .args(["config", "path", "--json", "--config-dir"])
        .arg(config_dir.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));
}

#[test]
fn test_sub_list_empty() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.args(["sub", "list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No subscriptions"));
}

#[test]
fn test_config_list_empty() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.args(["config", "list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No subscriptions"));
}

#[test]
fn test_config_help_no_longer_shows_use() {
    valsb()
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("path"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains(" use ").not());
}

#[test]
fn test_uninstall_requires_yes() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.arg("uninstall")
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires confirmation"));
}

#[test]
fn test_subcommand_help() {
    valsb()
        .args(["sub", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("use"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("remove"));
}

#[test]
fn test_node_help() {
    valsb()
        .args(["node", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("use"));
}

#[test]
fn test_reload_no_config() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.arg("reload").assert().failure();
}

#[test]
fn test_exit_code_user_error() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    let output = cmd.args(["node", "list"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn test_sub_add_help_shows_optional_url() {
    valsb()
        .args(["sub", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[URL]"))
        .stdout(predicate::str::contains("omit to paste interactively"));
}

#[test]
fn test_sub_add_json_requires_url_argument() {
    let (_tmp, mut cmd) = valsb_with_temp_config();
    cmd.args(["sub", "add", "--json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "\"subscription URL is required in JSON mode\"",
        ));
}

#[test]
fn test_completion_bash_outputs_script() {
    valsb()
        .args(["completion", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_valsb"))
        .stdout(predicate::str::contains("complete -F"));
}

#[test]
fn test_sub_remove_json_requires_yes() {
    let (tmp, mut cmd) = valsb_with_temp_config();
    let state_path = tmp.path().join("config/data/state.json");
    std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    std::fs::write(
        &state_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 2,
            "active_profile_id": "prof_01",
            "clash_api_addr": null,
            "profiles": [
                {
                    "id": "prof_01",
                    "subscription_url": "https://example.com/sub",
                    "subscription_url_normalized": "https://example.com/sub",
                    "remark": "demo",
                    "remark_source": "auto",
                    "last_update_at": null,
                    "last_update_status": null,
                    "last_update_error": null,
                    "node_count": 0
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    cmd.args(["sub", "remove", "0", "--json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("requires confirmation"));
}
