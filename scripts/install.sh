#!/usr/bin/env bash
#
# val-sing-box-cli installer (root-only)
#
# valsb manages a system service (sing-box) that needs root for TUN mode and
# binary placement under /usr/local/bin. This installer therefore *requires*
# root and will re-exec itself with sudo when run by a regular user.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | sudo bash
#   sudo ./install.sh
#
# Environment variables:
#   VALSB_VERSION    Override valsb version to install (default: latest)
#   SINGBOX_VERSION  Override sing-box version to install (default: latest)
#   INSTALL_DIR      Override binary install directory (default: /usr/local/bin
#                    on Linux/macOS, /usr/bin on OpenWrt)

set -euo pipefail

readonly APP_NAME="val-sing-box-cli"
readonly BIN_NAME="valsb"
readonly SINGBOX_BIN="sing-box"
readonly UNIT_NAME="valsb-sing-box.service"

readonly GITHUB_REPO="SagerNet/sing-box"
readonly VALSB_REPO="nsevo/val-sing-box-cli"

readonly COLOR_RED='\033[0;31m'
readonly COLOR_GREEN='\033[0;32m'
readonly COLOR_YELLOW='\033[0;33m'
readonly COLOR_CYAN='\033[0;36m'
readonly COLOR_BOLD='\033[1m'
readonly COLOR_RESET='\033[0m'

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

# ── Architecture detection ─────────────────────────────────────────────
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

# ── Path resolution (mirrors src/platform/paths.rs) ────────────────────
resolve_paths_linux() {
    CONFIG_DIR="/etc/$APP_NAME"
    CACHE_DIR="/var/cache/$APP_NAME"
    DATA_DIR="/var/lib/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-/usr/local/bin}"
    SINGBOX_BIN_DIR="/usr/local/lib/$APP_NAME/bin"
    UNIT_FILE="/etc/systemd/system/$UNIT_NAME"
    SERVICE_BACKEND="systemd"
}

resolve_paths_openwrt() {
    CONFIG_DIR="/etc/$APP_NAME"
    CACHE_DIR="/var/cache/$APP_NAME"
    DATA_DIR="/var/lib/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-/usr/bin}"
    SINGBOX_BIN_DIR="/usr/lib/$APP_NAME/bin"
    UNIT_FILE="/etc/init.d/valsb-sing-box"
    SERVICE_BACKEND="procd"
}

resolve_paths_macos() {
    CONFIG_DIR="/etc/$APP_NAME"
    CACHE_DIR="/var/cache/$APP_NAME"
    DATA_DIR="/var/lib/$APP_NAME"
    BIN_DIR="${INSTALL_DIR:-/usr/local/bin}"
    SINGBOX_BIN_DIR="/usr/local/lib/$APP_NAME/bin"
    UNIT_FILE="/Library/LaunchDaemons/com.valsb.sing-box.plist"
    SERVICE_BACKEND="launchd"
}

# ── Privilege handling: re-exec ourselves under sudo if needed ────────
ensure_root() {
    if [[ "$(id -u)" -eq 0 ]]; then
        return
    fi

    if ! command -v sudo &>/dev/null; then
        fatal "valsb installation requires root, and sudo is not available; re-run as root"
    fi

    info "valsb installer requires root; re-running with sudo..."

    # Preserve user-facing env vars across the sudo boundary.
    local pass_env=(VALSB_VERSION SINGBOX_VERSION INSTALL_DIR)
    local sudo_env=()
    for var in "${pass_env[@]}"; do
        if [[ -n "${!var:-}" ]]; then
            sudo_env+=("$var=${!var}")
        fi
    done

    # Re-execute the installer as root. When piped from curl, $0 is "bash"
    # and there is no script file on disk, so we cannot just `exec sudo $0`.
    # Detect that case and re-pipe the script body instead.
    if [[ -f "$0" && -r "$0" ]]; then
        exec sudo -E "${sudo_env[@]}" bash "$0" "$@"
    else
        fatal "could not auto-elevate; re-run as: curl -fsSL <url> | sudo bash"
    fi
}

preflight_service_requirements() {
    case "$SERVICE_BACKEND" in
        systemd)
            command -v systemctl &>/dev/null \
                || fatal "systemctl not found; systemd is required on Linux"
            ;;
        procd)
            [[ -d /etc/init.d ]] \
                || fatal "/etc/init.d not found; procd is required on OpenWrt"
            ;;
        launchd)
            command -v launchctl &>/dev/null \
                || fatal "launchctl not found; launchd is required on macOS"
            ;;
    esac
}

path_exists() { test -e "$1"; }

ensure_dir_tracked() {
    local dir="$1"
    if ! path_exists "$dir"; then
        mkdir -p "$dir"
        ROLLBACK_REMOVE_DIRS+=("$dir")
    fi
}

backup_file_if_present() {
    local path="$1"
    if ! path_exists "$path"; then
        return
    fi

    local backup_path="$ROLLBACK_BACKUP_DIR/backup.$(printf '%03d' "${#ROLLBACK_RESTORE_SRCS[@]}")"
    cp -fp "$path" "$backup_path"
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
    cp -f "$src" "$dest"
    chmod "$mode" "$dest"

    if (( ! existed )); then
        ROLLBACK_REMOVE_FILES+=("$dest")
    fi
}

service_enabled() {
    case "$SERVICE_BACKEND" in
        systemd) systemctl is-enabled "$UNIT_NAME" >/dev/null 2>&1 ;;
        procd)   [[ -x "$UNIT_FILE" ]] && "$UNIT_FILE" enabled >/dev/null 2>&1 ;;
        launchd) launchctl list | grep -q "com.valsb.sing-box" ;;
        *)       return 1 ;;
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

    case "$SERVICE_BACKEND" in
        systemd)
            systemctl daemon-reload >/dev/null 2>&1 || true
            if (( ROLLBACK_SERVICE_PREV_ENABLED )); then
                systemctl enable "$UNIT_NAME" >/dev/null 2>&1 || true
            else
                systemctl disable "$UNIT_NAME" >/dev/null 2>&1 || true
            fi
            ;;
        procd)
            if (( ROLLBACK_SERVICE_PREV_ENABLED )) && path_exists "$UNIT_FILE"; then
                "$UNIT_FILE" enable >/dev/null 2>&1 || true
            else
                "$UNIT_FILE" disable >/dev/null 2>&1 || true
            fi
            ;;
        launchd)
            launchctl unload "$UNIT_FILE" >/dev/null 2>&1 || true
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
        rm -f "${ROLLBACK_REMOVE_FILES[$i]}" >/dev/null 2>&1 || true
    done

    for (( i=${#ROLLBACK_RESTORE_SRCS[@]}-1; i>=0; i-- )); do
        cp -fp "${ROLLBACK_RESTORE_SRCS[$i]}" "${ROLLBACK_RESTORE_DSTS[$i]}" >/dev/null 2>&1 || true
    done

    restore_service_state

    for (( i=${#ROLLBACK_REMOVE_DIRS[@]}-1; i>=0; i-- )); do
        rm -rf "${ROLLBACK_REMOVE_DIRS[$i]}" >/dev/null 2>&1 || true
    done

    rm -rf "$ROLLBACK_TMP_DIR"
    return "$status"
}

# ── Version resolution ─────────────────────────────────────────────────
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

# ── Service unit generation (mirrors src/service/*) ────────────────────
write_systemd_unit() {
    local unit_file="$1" sing_box_bin="$2" config_path="$3" data_dir="$4"

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
RestartSec=5
LimitNOFILE=infinity
StandardOutput=append:${data_dir}/logs/sing-box.stdout.log
StandardError=append:${data_dir}/logs/sing-box.stderr.log

[Install]
WantedBy=multi-user.target
UNIT

    install_tracked_file "$staged_unit" "$unit_file" 644
    ROLLBACK_SERVICE_TOUCHED=1

    systemctl daemon-reload \
        || fatal "failed to reload the systemd daemon"
    systemctl enable "$UNIT_NAME" \
        || fatal "failed to enable system service $UNIT_NAME"
}

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
}

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

    local now
    now="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

    ensure_dir_tracked "$(dirname "$manifest_file")"

    local staged_manifest="$ROLLBACK_TMP_DIR/manifest.json"
    cat > "$staged_manifest" <<MANIFEST
{
  "schema_version": 3,
  "installed_at": "${now}",
  "valsb_version": "${valsb_version}",
  "sing_box_version": "${singbox_version}",
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
    check_dependencies
    ensure_root "$@"

    local arch os_family
    arch="$(detect_arch)"
    os_family="$(detect_os_family)"
    OS_FAMILY="$os_family"

    info "Detected: os=$os_family arch=$arch (root)"

    case "$os_family" in
        openwrt) resolve_paths_openwrt ;;
        linux)   resolve_paths_linux ;;
        macos)   resolve_paths_macos ;;
        *)       fatal "unsupported OS family: $os_family" ;;
    esac

    printf "\n${COLOR_BOLD}Install plan:${COLOR_RESET}\n" >&2
    printf "  %-20s %s\n" "valsb binary:"    "$BIN_DIR/$BIN_NAME" >&2
    printf "  %-20s %s\n" "sing-box binary:" "$SINGBOX_BIN_DIR/$SINGBOX_BIN" >&2
    printf "  %-20s %s\n" "config dir:"      "$CONFIG_DIR" >&2
    printf "  %-20s %s\n" "cache dir:"       "$CACHE_DIR" >&2
    printf "  %-20s %s\n" "data dir:"        "$DATA_DIR" >&2
    printf "  %-20s %s\n" "unit file:"       "$UNIT_FILE" >&2
    printf "\n" >&2

    preflight_service_requirements
    capture_service_state

    ROLLBACK_TMP_DIR="$(mktemp -d)"
    ROLLBACK_BACKUP_DIR="$ROLLBACK_TMP_DIR/backups"
    mkdir -p "$ROLLBACK_BACKUP_DIR"
    trap rollback_install EXIT
    trap 'exit 130' INT TERM

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

    ensure_dir_tracked "$CONFIG_DIR"
    ensure_dir_tracked "$CACHE_DIR"
    ensure_dir_tracked "$DATA_DIR"
    ensure_dir_tracked "$BIN_DIR"
    ensure_dir_tracked "$SINGBOX_BIN_DIR"
    ensure_dir_tracked "$CACHE_DIR/subscriptions"

    local valsb_target="$BIN_DIR/$BIN_NAME"
    if [[ -n "$valsb_version" ]]; then
        local valsb_extracted
        valsb_extracted="$(download_valsb "$valsb_version" "$arch" "$ROLLBACK_TMP_DIR")"
        install_tracked_file "$valsb_extracted" "$valsb_target" 755
        success "valsb $valsb_version installed to $valsb_target"
    else
        warn "skipped valsb binary installation (no release found)"
    fi

    local singbox_extracted
    singbox_extracted="$(download_singbox "$singbox_version" "$arch" "$ROLLBACK_TMP_DIR")"

    local singbox_target="$SINGBOX_BIN_DIR/$SINGBOX_BIN"
    install_tracked_file "$singbox_extracted" "$singbox_target" 755
    success "sing-box $singbox_version installed to $singbox_target"

    local config_file="$CONFIG_DIR/sing-box.json"
    case "$SERVICE_BACKEND" in
        systemd)
            write_systemd_unit "$UNIT_FILE" "$singbox_target" "$config_file" "$DATA_DIR"
            success "systemd unit written to $UNIT_FILE"
            ;;
        procd)
            write_procd_init_script "$UNIT_FILE" "$singbox_target" "$config_file" "$DATA_DIR"
            success "procd init script written to $UNIT_FILE"
            ;;
        launchd)
            write_launchd_plist "$UNIT_FILE" "$singbox_target" "$config_file" "$DATA_DIR"
            success "launchd plist written to $UNIT_FILE"
            ;;
    esac

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
        "$UNIT_FILE"

    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
        warn "$BIN_DIR is not in your PATH"
    fi

    printf "\n${COLOR_GREEN}${COLOR_BOLD}Installation complete!${COLOR_RESET}\n\n" >&2
    printf "valsb is a root-managed CLI: future commands will request sudo automatically.\n\n" >&2
    printf "Next steps:\n" >&2
    printf "  ${COLOR_CYAN}1.${COLOR_RESET} Add a subscription:    ${COLOR_BOLD}valsb sub add \"<url>\"${COLOR_RESET}\n" >&2
    printf "  ${COLOR_CYAN}2.${COLOR_RESET} Start the service:     ${COLOR_BOLD}valsb start${COLOR_RESET}\n" >&2
    printf "  ${COLOR_CYAN}3.${COLOR_RESET} Check environment:     ${COLOR_BOLD}valsb doctor${COLOR_RESET}\n" >&2
    printf "\n" >&2

    INSTALL_SUCCESS=1
}

do_install "$@"
