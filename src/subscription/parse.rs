use serde::Serialize;

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
pub struct NodeInfo {
    pub tag: String,
    pub node_type: String,
    pub server: String,
    pub server_port: String,
}

pub struct SubscriptionData {
    pub raw_config: serde_json::Value,
    pub nodes: Vec<NodeInfo>,
    pub clash_api_addr: String,
}

/// Parse subscription content. Returns the raw config (for direct use)
/// and extracted node list (for display).
///
/// Supports:
/// - Full sing-box JSON config (preferred, used as-is)
/// - Base64-encoded link lists (legacy fallback — produces an error)
pub fn parse_subscription_content(raw: &str) -> AppResult<SubscriptionData> {
    let content = raw.trim();

    if content.starts_with('{') {
        return parse_full_config(content);
    }

    if content.starts_with('[') {
        return Err(AppError::data_with_hint(
            "subscription returned a bare JSON array instead of a full sing-box config",
            "use a subscription URL with sing-box output format (e.g. &format=singbox)",
        ));
    }

    Err(AppError::data_with_hint(
        "subscription did not return a sing-box JSON config",
        "use a subscription URL with sing-box output format (e.g. &format=singbox)",
    ))
}

fn parse_full_config(content: &str) -> AppResult<SubscriptionData> {
    let mut config: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| AppError::data(format!("failed to parse subscription JSON: {e}")))?;

    let outbounds = config
        .get("outbounds")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::data_with_hint(
                "subscription JSON has no \"outbounds\" array",
                "use a subscription URL with sing-box output format",
            )
        })?;

    let nodes = extract_nodes_from_values(outbounds);

    if nodes.is_empty() {
        return Err(AppError::data(
            "no proxy nodes found in subscription config",
        ));
    }

    let clash_api_addr = crate::clash::inject_clash_api_defaults(&mut config);

    Ok(SubscriptionData {
        raw_config: config,
        nodes,
        clash_api_addr,
    })
}

fn extract_nodes_from_values(values: &[serde_json::Value]) -> Vec<NodeInfo> {
    let mut nodes = Vec::new();

    for val in values {
        let tag = val
            .get("tag")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();

        let node_type = val
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let server = val
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let server_port = if let Some(ports) = val.get("server_ports").and_then(|v| v.as_array()) {
            ports
                .iter()
                .filter_map(|p| p.as_str())
                .map(|s| s.replace(':', "-"))
                .collect::<Vec<_>>()
                .join(",")
        } else if let Some(port) = val.get("server_port").and_then(serde_json::Value::as_u64) {
            port.to_string()
        } else {
            String::new()
        };

        if matches!(
            node_type.as_str(),
            "selector" | "urltest" | "direct" | "block" | "dns"
        ) {
            continue;
        }

        nodes.push(NodeInfo {
            tag,
            node_type,
            server,
            server_port,
        });
    }

    nodes
}
