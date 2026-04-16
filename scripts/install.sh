#!/usr/bin/env bash
#
# val-sing-box-cli installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
#   wget -qO- https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
#
# Environment variables:
#   VALSB_VERSION      Override valsb version to install (default: latest)
#   SINGBOX_VERSION    Override sing-box version to install (default: latest)
#   INSTALL_DIR        Override binary install directory
#   NO_CONFIRM         Skip interactive prompts (equivalent to root auto-detection)
#   VALSB_DELEGATE_USER Delegate future valsb control to this user for system installs

set -euo pipefail

# ── Constants ───────────────────────────────────────────────────────────
readonly APP_NAME="val-sing-box-cli"
readonly BIN_NAME="valsb"
readonly SINGBOX_BIN="sing-box"
readonly UNIT_NAME="valsb-sing-box.service"
readonly CONTROL_GROUP="valsb"
readonly POLKIT_RULE_FILE="/etc/polkit-1/rules.d/90-valsb-systemd.rules"

readonly GITHUB_REPO="SagerNet/sing-box"
readonly VALSB_REPO="nsevo/val-sing-box-cli"

readonly COLOR_RED='\033[0;31m'
readonly COLOR_GREEN='\033[0;32m'
readonly COLOR_YELLOW='\033[0;33m'
readonly COLOR_CYAN='\033[0;36m'
readonly COLOR_BOLD='\033[1m'
readonly COLOR_RESET='\033[0m'

# ── Logging (all to stderr so stdout is reserved for return values) ────
info()    { printf "${COLOR_CYAN}[info]${COLOR_RESET}  %s\n" "$*" >&2; }
success() { printf "${COLOR_GREEN}[ok]${COLOR_RESET}    %s\n" "$*" >&2; }
warn()    { printf "${COLOR_YELLOW}[warn]${COLOR_RESET}  %s\n" "$*" >&2; }
error()   { printf "${COLOR_RED}[error]${COLOR_RESET} %s\n" "$*" >&2; }
fatal()   { error "$@"; exit 1; }

INSTALL_SUCCESS=0
ROLLBACK_TMP_DIR=""
ROLLBACK_BACKUP_DIR=""
ROLLBACK_SERVICE_TOUCHED=0
ROLLBACK_SERVICE_PREV_ENABLED=0
ROLLBACK_REMOVE_FILES=()
ROLLBACK_REMOVE_DIRS=()
ROLLBACK_RESTORE_SRCS=()
ROLLBACK_RESTORE_DSTS=()

# ── Pre-flight checks ──────────────────────────────────────────────────
check_dependencies() {
    local missing=()
    for cmd in curl tar; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done
    if (( ${#missing[@]} > 0 )); then
        fatal "missing required commands: ${missing[*]}"
    fi
}

# ── Architecture detection ──────────────────────────────────────────────
detect_arch() {
    local raw
    raw="$(uname -m)"
    case "$raw" in
        x86_64|amd64)   echo "amd64" ;;
        aarch64|arm64)  echo "arm64" ;;
        *)              fatal "unsupported architecture: $raw" ;;
    esac
}

# ── OS family detection ────────────────────────────────────────────────
detect_os_family() {
    if [[ -f /etc/openwrt_release ]]; then
        echo "openwrt"
    elif [[ "$(uname -s)" == "Darwin" ]]; then
        echo "macos"
    else
        echo "linux"
    fi
}

# ── Path resolution (mirrors src/platform/paths.rs exactly) ───────────
resolve_paths_user() {
    local home_dir="${HOME:-/tmp}"
    CONFIG_DIR="${XDG_CONFIG_HOME:-$home_dir/.config}/$APP_NAME"
    CACHE_DIR="${XDG_CACHE_HOME:-$home_dir/.cache}/$APP_NAME"
    DATA_DIR="${XDG_DATA_HOME:-$home_dir/.local/share}/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-$home_dir/.local/bin}"
    SINGBOX_BIN_DIR="$DATA_DIR/bin"
    UNIT_FILE="${XDG_CONFIG_HOME:-$home_dir/.config}/systemd/user/$UNIT_NAME"
    SYSTEMD_MODE="user"
}

resolve_paths_root() {
    CONFIG_DIR="/etc/$APP_NAME"
    CACHE_DIR="/var/cache/$APP_NAME"
    DATA_DIR="/var/lib/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-/usr/local/bin}"
    SINGBOX_BIN_DIR="/usr/local/lib/$APP_NAME/bin"
    UNIT_FILE="/etc/systemd/system/$UNIT_NAME"
    SYSTEMD_MODE="system"
}

resolve_paths_openwrt() {
    CONFIG_DIR="/etc/$APP_NAME"
    CACHE_DIR="/var/cache/$APP_NAME"
    DATA_DIR="/var/lib/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-/usr/bin}"
    SINGBOX_BIN_DIR="/usr/lib/$APP_NAME/bin"
    UNIT_FILE="/etc/init.d/valsb-sing-box"
    SYSTEMD_MODE="procd"
}

resolve_paths_macos_user() {
    local home_dir="${HOME:-/tmp}"
    CONFIG_DIR="$home_dir/Library/Application Support/$APP_NAME"
    CACHE_DIR="$home_dir/Library/Caches/$APP_NAME"
    DATA_DIR="$home_dir/Library/Application Support/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-$home_dir/.local/bin}"
    SINGBOX_BIN_DIR="$DATA_DIR/bin"
    UNIT_FILE="$home_dir/Library/LaunchAgents/com.valsb.sing-box.plist"
    SYSTEMD_MODE="launchd-user"
}

resolve_paths_macos_root() {
    CONFIG_DIR="/etc/$APP_NAME"
    CACHE_DIR="/var/cache/$APP_NAME"
    DATA_DIR="/var/lib/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-/usr/local/bin}"
    SINGBOX_BIN_DIR="/usr/local/lib/$APP_NAME/bin"
    UNIT_FILE="/Library/LaunchDaemons/com.valsb.sing-box.plist"
    SYSTEMD_MODE="launchd-system"
}

resolve_delegate_user() {
    local delegate_user="${VALSB_DELEGATE_USER:-}"
    if [[ -z "$delegate_user" ]] && [[ -n "${SUDO_USER:-}" ]]; then
        delegate_user="$SUDO_USER"
    fi
    if [[ -z "$delegate_user" ]] || [[ "$delegate_user" == "root" ]]; then
        echo ""
        return
    fi
    echo "$delegate_user"
}

configure_linux_system_delegate() {
    local delegate_user="$1"

    $SUDO_CMD getent group "$CONTROL_GROUP" >/dev/null 2>&1 || \
        $SUDO_CMD groupadd --system "$CONTROL_GROUP" || fatal "failed to create group $CONTROL_GROUP"

    $SUDO_CMD usermod -a -G "$CONTROL_GROUP" "$delegate_user" \
        || fatal "failed to add $delegate_user to group $CONTROL_GROUP"

    $SUDO_CMD chgrp -R "$CONTROL_GROUP" "$CONFIG_DIR" "$CACHE_DIR" "$DATA_DIR" \
        || fatal "failed to assign valsb group ownership"

    $SUDO_CMD find "$CONFIG_DIR" "$CACHE_DIR" "$DATA_DIR" -type d -exec chmod 2770 {} + \
        || fatal "failed to set directory permissions for delegated valsb access"
    $SUDO_CMD find "$CONFIG_DIR" "$CACHE_DIR" "$DATA_DIR" -type f -exec chmod 0660 {} + \
        || fatal "failed to set file permissions for delegated valsb access"

    $SUDO_CMD mkdir -p "$(dirname "$POLKIT_RULE_FILE")"
    $SUDO_CMD tee "$POLKIT_RULE_FILE" >/dev/null <<EOF
polkit.addRule(function(action, subject) {
    if (action.id != "org.freedesktop.systemd1.manage-units") {
        return;
    }

    var unit = action.lookup("unit");
    var verb = action.lookup("verb");
    var allowed = ["start", "stop", "restart", "reload"];
    if (unit == "$UNIT_NAME" && subject.isInGroup("$CONTROL_GROUP") && allowed.indexOf(verb) >= 0) {
        return polkit.Result.YES;
    }
});
EOF

    DELEGATED_MODE="systemd_polkit_group"
    DELEGATED_PRINCIPAL="$delegate_user"
    DELEGATED_GROUP="$CONTROL_GROUP"
}

preflight_service_requirements() {
    case "$SYSTEMD_MODE" in
        user)
            command -v systemctl &>/dev/null \
                || fatal "systemctl not found; systemd is required for user-mode installation"
            [[ -n "${XDG_RUNTIME_DIR:-}" ]] \
                || fatal "user-mode installation requires a logged-in systemd user session (XDG_RUNTIME_DIR is unset). Re-run from a normal login shell, or install system-wide with sudo."
            systemctl --user show-environment >/dev/null 2>&1 \
                || fatal "user-mode installation requires an active systemd user manager. Re-run from a normal login shell, or install system-wide with sudo."
            ;;
        system)
            command -v systemctl &>/dev/null \
                || fatal "systemctl not found; systemd is required for this installation mode"
            ;;
    esac
}

run_privileged() {
    if [[ -n "${SUDO_CMD:-}" ]]; then
        sudo "$@"
    else
        "$@"
    fi
}

path_exists() {
    if [[ -n "${SUDO_CMD:-}" ]]; then
        sudo test -e "$1"
    else
        test -e "$1"
    fi
}

ensure_dir_tracked() {
    local dir="$1"
    if ! path_exists "$dir"; then
        run_privileged mkdir -p "$dir"
        ROLLBACK_REMOVE_DIRS+=("$dir")
    fi
}

backup_file_if_present() {
    local path="$1"
    if ! path_exists "$path"; then
        return
    fi

    local backup_path="$ROLLBACK_BACKUP_DIR/backup.$(printf '%03d' "${#ROLLBACK_RESTORE_SRCS[@]}")"
    run_privileged cp -fp "$path" "$backup_path"
    ROLLBACK_RESTORE_SRCS+=("$backup_path")
    ROLLBACK_RESTORE_DSTS+=("$path")
}

install_tracked_file() {
    local src="$1" dest="$2" mode="$3"
    local existed=0
    if path_exists "$dest"; then
        existed=1
    fi

    backup_file_if_present "$dest"
    ensure_dir_tracked "$(dirname "$dest")"
    run_privileged cp -f "$src" "$dest"
    run_privileged chmod "$mode" "$dest"

    if (( ! existed )); then
        ROLLBACK_REMOVE_FILES+=("$dest")
    fi
}

service_enabled() {
    case "$SYSTEMD_MODE" in
        user)
            systemctl --user is-enabled "$UNIT_NAME" >/dev/null 2>&1
            ;;
        system)
            run_privileged systemctl is-enabled "$UNIT_NAME" >/dev/null 2>&1
            ;;
        procd)
            [[ -x "$UNIT_FILE" ]] && "$UNIT_FILE" enabled >/dev/null 2>&1
            ;;
        *)
            return 1
            ;;
    esac
}

capture_service_state() {
    if service_enabled; then
        ROLLBACK_SERVICE_PREV_ENABLED=1
    else
        ROLLBACK_SERVICE_PREV_ENABLED=0
    fi
}

restore_service_state() {
    (( ROLLBACK_SERVICE_TOUCHED )) || return

    case "$SYSTEMD_MODE" in
        user)
            systemctl --user daemon-reload >/dev/null 2>&1 || true
            if (( ROLLBACK_SERVICE_PREV_ENABLED )); then
                systemctl --user enable "$UNIT_NAME" >/dev/null 2>&1 || true
            else
                systemctl --user disable "$UNIT_NAME" >/dev/null 2>&1 || true
            fi
            ;;
        system)
            run_privileged systemctl daemon-reload >/dev/null 2>&1 || true
            if (( ROLLBACK_SERVICE_PREV_ENABLED )); then
                run_privileged systemctl enable "$UNIT_NAME" >/dev/null 2>&1 || true
            else
                run_privileged systemctl disable "$UNIT_NAME" >/dev/null 2>&1 || true
            fi
            ;;
        procd)
            if (( ROLLBACK_SERVICE_PREV_ENABLED )) && path_exists "$UNIT_FILE"; then
                "$UNIT_FILE" enable >/dev/null 2>&1 || true
            else
                "$UNIT_FILE" disable >/dev/null 2>&1 || true
            fi
            ;;
    esac
}

rollback_install() {
    local status=$?
    local i

    if (( INSTALL_SUCCESS )); then
        [[ -n "${ROLLBACK_TMP_DIR:-}" ]] && rm -rf "$ROLLBACK_TMP_DIR"
        return
    fi

    [[ -n "${ROLLBACK_TMP_DIR:-}" ]] || return

    warn "Installation failed; rolling back partial changes."

    for (( i=${#ROLLBACK_REMOVE_FILES[@]}-1; i>=0; i-- )); do
        run_privileged rm -f "${ROLLBACK_REMOVE_FILES[$i]}" >/dev/null 2>&1 || true
    done

    for (( i=${#ROLLBACK_RESTORE_SRCS[@]}-1; i>=0; i-- )); do
        run_privileged cp -fp "${ROLLBACK_RESTORE_SRCS[$i]}" "${ROLLBACK_RESTORE_DSTS[$i]}" >/dev/null 2>&1 || true
    done

    restore_service_state

    for (( i=${#ROLLBACK_REMOVE_DIRS[@]}-1; i>=0; i-- )); do
        run_privileged rm -rf "${ROLLBACK_REMOVE_DIRS[$i]}" >/dev/null 2>&1 || true
    done

    rm -rf "$ROLLBACK_TMP_DIR"
    return "$status"
}

# ── Elevate to root or fall back to user mode ────────────────────────
try_elevate_to_root() {
    printf "\n${COLOR_BOLD}Root privileges recommended${COLOR_RESET}\n" >&2
    printf "  - TUN mode requires root to create network interfaces\n" >&2
    printf "  - System-wide service runs at boot before any user logs in\n" >&2
    printf "  - Binaries go to /usr/local/bin (available to all users)\n" >&2
    printf "\n" >&2

    if [[ -n "${NO_CONFIRM:-}" ]] || [[ ! -e /dev/tty ]]; then
        if command -v sudo &>/dev/null && sudo -n true 2>/dev/null; then
            echo "sudo"
        else
            echo "user"
        fi
        return
    fi

    local choice
    printf "Proceed with sudo? [Y/n]: " >&2
    read -r choice </dev/tty
    case "$choice" in
        [nN]|[nN][oO])
            info "Continuing with user-mode installation."
            echo "user"
            return
            ;;
    esac

    if command -v sudo &>/dev/null; then
        info "Requesting root privileges..."
        if sudo -v </dev/tty 2>&1; then
            echo "sudo"
            return
        fi
    fi

    warn "Could not obtain root privileges, falling back to user-mode."
    echo "user"
}

# ── Version resolution ──────────────────────────────────────────────────
extract_tag_version() {
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -1
}

get_latest_singbox_version() {
    local resp
    resp="$(curl -fsSL "https://api.github.com/repos/$GITHUB_REPO/releases/latest" 2>/dev/null)" \
        || fatal "failed to fetch latest sing-box release info"
    echo "$resp" | extract_tag_version
}

get_latest_valsb_version() {
    local resp
    resp="$(curl -fsSL "https://api.github.com/repos/$VALSB_REPO/releases/latest" 2>/dev/null)" \
        || fatal "failed to fetch latest valsb release info"
    echo "$resp" | extract_tag_version
}

# ── Download + extract ─────────────────────────────────────────────────
extract_tar_archive() {
    local archive="$1" extract_dir="$2"

    mkdir -p "$extract_dir"
    tar xzf "$archive" -C "$extract_dir" 2>/dev/null
}

find_extracted_binary() {
    local extract_dir="$1" binary_name="$2"
    find "$extract_dir" -name "$binary_name" -type f | head -1
}

download_singbox() {
    local version="$1" arch="$2" dest_dir="$3"
    local os_tag="linux"
    [[ "$OS_FAMILY" == "macos" ]] && os_tag="darwin"
    local filename="sing-box-${version}-${os_tag}-${arch}.tar.gz"
    local url="https://github.com/$GITHUB_REPO/releases/download/v${version}/${filename}"
    local archive="$dest_dir/$filename"

    info "Downloading sing-box $version ($arch)..."
    curl -fSL --progress-bar -o "$archive" "$url" \
        || fatal "failed to download $url"

    info "Extracting..."
    local extract_dir="$dest_dir/extracted"
    extract_tar_archive "$archive" "$extract_dir" \
        || fatal "failed to extract archive"

    local bin
    bin="$(find_extracted_binary "$extract_dir" "sing-box")"
    if [[ -z "$bin" ]]; then
        fatal "sing-box binary not found in extracted archive"
    fi

    echo "$bin"
}

download_valsb() {
    local version="$1" arch="$2" dest_dir="$3"
    local os_tag="linux"
    [[ "$OS_FAMILY" == "macos" ]] && os_tag="darwin"
    local filename="valsb-v${version}-${os_tag}-${arch}.tar.gz"
    local url="https://github.com/$VALSB_REPO/releases/download/v${version}/${filename}"
    local archive="$dest_dir/$filename"

    info "Downloading valsb $version ($arch)..."
    curl -fSL --progress-bar -o "$archive" "$url" \
        || fatal "failed to download $url"

    info "Extracting..."
    local extract_dir="$dest_dir/valsb-extracted"
    extract_tar_archive "$archive" "$extract_dir" \
        || fatal "failed to extract valsb archive"

    local bin
    bin="$(find_extracted_binary "$extract_dir" "valsb")"
    if [[ -z "$bin" ]]; then
        fatal "valsb binary not found in extracted archive"
    fi

    echo "$bin"
}

# ── Systemd unit generation (mirrors src/service/systemd.rs) ──────────
write_systemd_unit() {
    local mode="$1" unit_file="$2" sing_box_bin="$3" config_path="$4" data_dir="$5"

    local wanted_by="multi-user.target"
    local extra=""
    if [[ "$mode" == "user" ]]; then
        wanted_by="default.target"
    else
        extra=$'\nLimitNOFILE=infinity'
    fi

    ensure_dir_tracked "$(dirname "$unit_file")"
    ensure_dir_tracked "$data_dir/logs"

    local staged_unit="$ROLLBACK_TMP_DIR/${UNIT_NAME}.systemd"
    cat > "$staged_unit" <<UNIT
[Unit]
Description=sing-box managed by valsb
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${sing_box_bin} -D ${data_dir} -c ${config_path} run
ExecReload=/bin/kill -HUP \$MAINPID
Restart=on-failure
RestartSec=5${extra}
StandardOutput=append:${data_dir}/logs/sing-box.stdout.log
StandardError=append:${data_dir}/logs/sing-box.stderr.log

[Install]
WantedBy=${wanted_by}
UNIT

    install_tracked_file "$staged_unit" "$unit_file" 644
    ROLLBACK_SERVICE_TOUCHED=1

    if [[ "$mode" == "user" ]]; then
        [[ -n "${XDG_RUNTIME_DIR:-}" ]] \
            || fatal "user-mode installation requires a logged-in systemd user session (XDG_RUNTIME_DIR is unset). Re-run from a normal login shell, or install system-wide with sudo."

        systemctl --user daemon-reload \
            || fatal "failed to reload the user systemd daemon"
        systemctl --user enable "$UNIT_NAME" \
            || fatal "failed to enable user service $UNIT_NAME"
        systemctl --user is-enabled "$UNIT_NAME" >/dev/null \
            || fatal "user service $UNIT_NAME was not enabled successfully"
    else
        $SUDO_CMD systemctl daemon-reload \
            || fatal "failed to reload the systemd daemon"
        $SUDO_CMD systemctl enable "$UNIT_NAME" \
            || fatal "failed to enable system service $UNIT_NAME"
        $SUDO_CMD systemctl is-enabled "$UNIT_NAME" >/dev/null \
            || fatal "system service $UNIT_NAME was not enabled successfully"
    fi
}

# ── OpenWrt init script (mirrors src/service/procd.rs) ─────────────────
write_procd_init_script() {
    local init_file="$1" sing_box_bin="$2" config_path="$3" data_dir="$4"

    local staged_init="$ROLLBACK_TMP_DIR/valsb-sing-box.procd"

    cat > "$staged_init" <<'INITEOF'
#!/bin/sh /etc/rc.common

START=99
STOP=10
USE_PROCD=1

INITEOF

    cat >> "$staged_init" <<INITEOF
SING_BOX_BIN="${sing_box_bin}"
CONFIG_FILE="${config_path}"
DATA_DIR="${data_dir}"

start_service() {
    procd_open_instance
    procd_set_param command \$SING_BOX_BIN -D \$DATA_DIR -c \$CONFIG_FILE run
    procd_set_param respawn
    procd_set_param stderr 1
    procd_set_param stdout 1
    procd_close_instance
}

reload_service() {
    procd_send_signal valsb-sing-box
}
INITEOF

    install_tracked_file "$staged_init" "$init_file" 755
    ROLLBACK_SERVICE_TOUCHED=1

    "$init_file" enable \
        || fatal "failed to enable procd service $init_file"
    "$init_file" enabled >/dev/null \
        || fatal "procd service $init_file was not enabled successfully"
}

# ── launchd plist generation (mirrors src/service/launchd.rs) ───────────
write_launchd_plist() {
    local plist_file="$1" sing_box_bin="$2" config_path="$3" data_dir="$4"

    ensure_dir_tracked "$(dirname "$plist_file")"

    local staged_plist="$ROLLBACK_TMP_DIR/com.valsb.sing-box.plist"
    cat > "$staged_plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.valsb.sing-box</string>
    <key>ProgramArguments</key>
    <array>
        <string>${sing_box_bin}</string>
        <string>-D</string>
        <string>${data_dir}</string>
        <string>-c</string>
        <string>${config_path}</string>
        <string>run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>${data_dir}/sing-box.err.log</string>
</dict>
</plist>
PLIST

    install_tracked_file "$staged_plist" "$plist_file" 644
}

# ── Manifest generation (mirrors src/install/manifest.rs) ──────────────
write_manifest() {
    local manifest_file="$1"
    local valsb_version="$2"
    local singbox_version="$3"
    local valsb_bin_path="$4"
    local singbox_bin_path="$5"
    local config_dir="$6"
    local cache_dir="$7"
    local data_dir="$8"
    local unit_file="$9"
    local install_scope="${10}"
    local delegated_mode="${11}"
    local delegated_principal="${12}"
    local delegated_group="${13}"

    local now
    now="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

    ensure_dir_tracked "$(dirname "$manifest_file")"

    local delegated_block="null"
    if [[ -n "$delegated_mode" ]]; then
        delegated_block=$(cat <<EOF
{
    "mode": "${delegated_mode}",
    "principal": "${delegated_principal}",
    "group": "${delegated_group}"
  }
EOF
)
    fi

    local staged_manifest="$ROLLBACK_TMP_DIR/manifest.json"
    cat > "$staged_manifest" <<MANIFEST
{
  "schema_version": 2,
  "installed_at": "${now}",
  "valsb_version": "${valsb_version}",
  "sing_box_version": "${singbox_version}",
  "install_scope": "${install_scope}",
  "delegated_control": ${delegated_block},
  "managed_paths": {
    "valsb_bin": "${valsb_bin_path}",
    "sing_box_bin": "${singbox_bin_path}",
    "config_dir": "${config_dir}",
    "cache_dir": "${cache_dir}",
    "data_dir": "${data_dir}",
    "unit_file": "${unit_file}"
  }
}
MANIFEST

    install_tracked_file "$staged_manifest" "$manifest_file" 644

    success "Manifest written to $manifest_file"
}

# ── Main install procedure ─────────────────────────────────────────────
do_install() {
    local arch os_family install_mode
    local delegate_user=""

    check_dependencies
    arch="$(detect_arch)"
    os_family="$(detect_os_family)"
    OS_FAMILY="$os_family"

    info "Detected: os=$os_family arch=$arch uid=$(id -u)"

    # ── Determine install mode ──────────────────────────────────────
    SUDO_CMD=""
    if [[ "$(id -u)" -eq 0 ]]; then
        install_mode="root"
        info "Running as root, using system-wide installation."
    elif [[ "$os_family" == "openwrt" ]]; then
        fatal "OpenWrt installation requires root. Please run with: sudo $0"
    else
        local elevate_result
        elevate_result="$(try_elevate_to_root)"
        if [[ "$elevate_result" == "sudo" ]]; then
            install_mode="root"
            SUDO_CMD="sudo"
            info "Installing system-wide via sudo."
        else
            install_mode="user"
        fi
    fi

    # ── Resolve paths ───────────────────────────────────────────────
    case "${os_family}:${install_mode}" in
        openwrt:*)   resolve_paths_openwrt ;;
        linux:root)  resolve_paths_root ;;
        linux:user)  resolve_paths_user ;;
        macos:root)  resolve_paths_macos_root ;;
        macos:user)  resolve_paths_macos_user ;;
        *)           fatal "unexpected os/mode combination: $os_family:$install_mode" ;;
    esac

    local install_scope="user"
    [[ "$install_mode" == "root" ]] && install_scope="system"
    DELEGATED_MODE=""
    DELEGATED_PRINCIPAL=""
    DELEGATED_GROUP=""

    printf "\n${COLOR_BOLD}Install plan:${COLOR_RESET}\n" >&2
    printf "  %-20s %s\n" "valsb binary:"  "$BIN_DIR/$BIN_NAME" >&2
    printf "  %-20s %s\n" "sing-box binary:" "$SINGBOX_BIN_DIR/$SINGBOX_BIN" >&2
    printf "  %-20s %s\n" "config dir:"    "$CONFIG_DIR" >&2
    printf "  %-20s %s\n" "cache dir:"     "$CACHE_DIR" >&2
    printf "  %-20s %s\n" "data dir:"      "$DATA_DIR" >&2
    printf "  %-20s %s\n" "unit file:"     "$UNIT_FILE" >&2
    printf "\n" >&2

    # ── Fail early before writing files or downloading archives ───────
    preflight_service_requirements
    capture_service_state

    ROLLBACK_TMP_DIR="$(mktemp -d)"
    ROLLBACK_BACKUP_DIR="$ROLLBACK_TMP_DIR/backups"
    mkdir -p "$ROLLBACK_BACKUP_DIR"
    trap rollback_install EXIT
    trap 'exit 130' INT TERM

    # ── Resolve versions ────────────────────────────────────────────
    info "Resolving latest versions..."

    local valsb_version="${VALSB_VERSION:-}"
    if [[ -z "$valsb_version" ]]; then
        valsb_version="$(get_latest_valsb_version)" || true
    fi

    local singbox_version="${SINGBOX_VERSION:-}"
    if [[ -z "$singbox_version" ]]; then
        singbox_version="$(get_latest_singbox_version)"
    fi

    if [[ -n "$valsb_version" ]]; then
        info "valsb:    $valsb_version"
    else
        warn "could not resolve valsb version; skipping valsb binary download"
    fi
    info "sing-box: $singbox_version"

    # ── Create directories ──────────────────────────────────────────
    ensure_dir_tracked "$CONFIG_DIR"
    ensure_dir_tracked "$CACHE_DIR"
    ensure_dir_tracked "$DATA_DIR"
    ensure_dir_tracked "$BIN_DIR"
    ensure_dir_tracked "$SINGBOX_BIN_DIR"
    ensure_dir_tracked "$CACHE_DIR/subscriptions"

    # ── Install valsb ───────────────────────────────────────────────
    local valsb_target="$BIN_DIR/$BIN_NAME"
    if [[ -n "$valsb_version" ]]; then
        local valsb_extracted
        valsb_extracted="$(download_valsb "$valsb_version" "$arch" "$ROLLBACK_TMP_DIR")"
        install_tracked_file "$valsb_extracted" "$valsb_target" 755
        success "valsb $valsb_version installed to $valsb_target"
    else
        warn "skipped valsb binary installation (no release found)"
    fi

    # ── Install sing-box ────────────────────────────────────────────
    local singbox_extracted
    singbox_extracted="$(download_singbox "$singbox_version" "$arch" "$ROLLBACK_TMP_DIR")"

    local singbox_target="$SINGBOX_BIN_DIR/$SINGBOX_BIN"
    install_tracked_file "$singbox_extracted" "$singbox_target" 755
    success "sing-box $singbox_version installed to $singbox_target"

    # ── Write service unit ──────────────────────────────────────────
    local config_file="$CONFIG_DIR/sing-box.json"

    if [[ "$SYSTEMD_MODE" == "procd" ]]; then
        write_procd_init_script "$UNIT_FILE" "$singbox_target" "$config_file" "$DATA_DIR"
        success "procd init script written to $UNIT_FILE"
    elif [[ "$SYSTEMD_MODE" == launchd-* ]]; then
        write_launchd_plist "$UNIT_FILE" "$singbox_target" "$config_file" "$DATA_DIR"
        success "launchd plist written to $UNIT_FILE"
    elif [[ "$SYSTEMD_MODE" == "user" || "$SYSTEMD_MODE" == "system" ]]; then
        write_systemd_unit "$SYSTEMD_MODE" "$UNIT_FILE" "$singbox_target" "$config_file" "$DATA_DIR"
        success "systemd unit written to $UNIT_FILE ($SYSTEMD_MODE mode)"
        if [[ "$os_family" == "linux" && "$SYSTEMD_MODE" == "system" ]]; then
            delegate_user="$(resolve_delegate_user)"
            if [[ -n "$delegate_user" ]]; then
                configure_linux_system_delegate "$delegate_user"
                success "Delegated future valsb control to $delegate_user via group $CONTROL_GROUP"
                warn "You may need to log out and log back in before the new group membership takes effect."
            fi
        fi
    else
        warn "no supported service backend found, skipping service installation"
    fi

    # ── Write manifest ──────────────────────────────────────────────
    local manifest_file="$DATA_DIR/manifest.json"
    write_manifest \
        "$manifest_file" \
        "${valsb_version:-unknown}" \
        "$singbox_version" \
        "$valsb_target" \
        "$singbox_target" \
        "$CONFIG_DIR" \
        "$CACHE_DIR" \
        "$DATA_DIR" \
        "$UNIT_FILE" \
        "$install_scope" \
        "$DELEGATED_MODE" \
        "$DELEGATED_PRINCIPAL" \
        "$DELEGATED_GROUP"

    # ── Verify PATH ────────────────────────────────────────────────
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
        warn "$BIN_DIR is not in your PATH"
        if [[ "$install_mode" == "user" ]]; then
            local shell_rc=".bashrc"
            [[ "$os_family" == "macos" ]] && shell_rc=".zshrc"
            info "Add it with:"
            printf "  ${COLOR_BOLD}echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/${shell_rc} && source ~/${shell_rc}${COLOR_RESET}\n" >&2
        fi
    fi

    # ── Done ────────────────────────────────────────────────────────
    printf "\n${COLOR_GREEN}${COLOR_BOLD}Installation complete!${COLOR_RESET}\n\n" >&2
    printf "Next steps:\n" >&2
    printf "  ${COLOR_CYAN}1.${COLOR_RESET} Add a subscription:    ${COLOR_BOLD}valsb sub add \"<url>\"${COLOR_RESET}\n" >&2
    printf "  ${COLOR_CYAN}2.${COLOR_RESET} Start the service:     ${COLOR_BOLD}valsb start${COLOR_RESET}\n" >&2
    printf "  ${COLOR_CYAN}3.${COLOR_RESET} Check environment:     ${COLOR_BOLD}valsb doctor${COLOR_RESET}\n" >&2
    printf "\n" >&2

    INSTALL_SUCCESS=1
}

do_install "$@"
