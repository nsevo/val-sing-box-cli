use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::errors::{AppError, AppResult};

pub const DEFAULT_EXTERNAL_CONTROLLER: &str = "127.0.0.1:9090";
const DEFAULT_DELAY_TEST_URL: &str = "https://www.gstatic.com/generate_204";

#[derive(Debug, Deserialize)]
struct ProxiesResponse {
    proxies: HashMap<String, ProxyEntry>,
}

#[derive(Debug, Deserialize)]
struct ProxyEntry {
    #[serde(default)]
    now: String,
    #[serde(default, rename = "all")]
    members: Vec<String>,
    #[serde(default)]
    history: Vec<HistoryEntry>,
}

#[derive(Debug, Deserialize)]
struct HistoryEntry {
    #[serde(default)]
    delay: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ProxyGroupStatus {
    pub current: Option<String>,
    pub members: Vec<String>,
}

pub struct ClashClient {
    base_url: String,
    secret: Option<String>,
    client: reqwest::Client,
}

pub fn inject_clash_api_defaults(config: &mut Value) -> String {
    ensure_group_interrupts(config);

    let Some(obj) = config.as_object_mut() else {
        return DEFAULT_EXTERNAL_CONTROLLER.to_string();
    };

    let experimental = obj
        .entry("experimental")
        .or_insert_with(|| serde_json::json!({}));

    let Some(exp_obj) = experimental.as_object_mut() else {
        return DEFAULT_EXTERNAL_CONTROLLER.to_string();
    };

    let addr = if let Some(clash_api) = exp_obj.get("clash_api") {
        clash_api
            .get("external_controller")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_EXTERNAL_CONTROLLER)
            .to_string()
    } else {
        exp_obj.insert(
            "clash_api".to_string(),
            serde_json::json!({ "external_controller": DEFAULT_EXTERNAL_CONTROLLER }),
        );
        DEFAULT_EXTERNAL_CONTROLLER.to_string()
    };

    if !exp_obj.contains_key("cache_file") {
        exp_obj.insert(
            "cache_file".to_string(),
            serde_json::json!({ "enabled": true }),
        );
    }

    addr
}

impl ClashClient {
    pub fn new(addr: &str, secret: Option<String>) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| AppError::runtime(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            base_url: format!("http://{addr}"),
            secret,
            client,
        })
    }

    fn auth_header(&self) -> Option<String> {
        self.secret.as_ref().map(|s| format!("Bearer {s}"))
    }

    fn build_request(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.auth_header() {
            Some(auth) => req.header("Authorization", auth),
            None => req,
        }
    }

    pub async fn select_proxy(&self, group: &str, node: &str) -> AppResult<()> {
        let url = format!("{}/proxies/{}", self.base_url, urlencoding_minimal(group));

        let req = self
            .build_request(self.client.put(&url))
            .json(&serde_json::json!({"name": node}));

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::runtime(format!("failed to switch proxy: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::runtime(format!("failed to switch proxy: {body}")));
        }

        Ok(())
    }

    /// Fetch cached latency for all nodes from the Clash API.
    /// Returns a map of `node_name` -> `delay_ms` (0 means timeout/unknown).
    pub async fn fetch_all_delays(&self) -> AppResult<HashMap<String, u32>> {
        let url = format!("{}/proxies", self.base_url);
        let req = self.build_request(self.client.get(&url));

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::runtime(format!("Clash API unreachable: {e}")))?;

        if !resp.status().is_success() {
            return Ok(HashMap::new());
        }

        let data: ProxiesResponse = resp.json().await.unwrap_or(ProxiesResponse {
            proxies: HashMap::new(),
        });

        let mut delays = HashMap::new();
        for (name, entry) in data.proxies {
            if let Some(last) = entry.history.last() {
                delays.insert(name, last.delay);
            }
        }

        Ok(delays)
    }

    pub async fn fetch_proxy_groups(&self) -> AppResult<HashMap<String, ProxyGroupStatus>> {
        let url = format!("{}/proxies", self.base_url);
        let req = self.build_request(self.client.get(&url));

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::runtime(format!("Clash API unreachable: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::runtime(format!(
                "Clash API proxy request failed: {body}"
            )));
        }

        let data: ProxiesResponse = resp
            .json()
            .await
            .map_err(|e| AppError::runtime(format!("failed to decode Clash proxies: {e}")))?;

        Ok(data
            .proxies
            .into_iter()
            .map(|(name, entry)| {
                let current = (!entry.now.is_empty()).then_some(entry.now);
                (
                    name,
                    ProxyGroupStatus {
                        current,
                        members: entry.members,
                    },
                )
            })
            .collect())
    }

    /// Trigger a batch delay test for all members of a group via
    /// `GET /group/:name/delay?url=...&timeout=...`.
    /// Returns fresh delay map for the group members.
    pub async fn test_group_delay(&self, group: &str) -> AppResult<HashMap<String, u32>> {
        let url = format!(
            "{}/group/{}/delay?url={}&timeout=3000",
            self.base_url,
            urlencoding_minimal(group),
            urlencoding_minimal(DEFAULT_DELAY_TEST_URL)
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .build()
            .map_err(|e| AppError::runtime(format!("failed to build HTTP client: {e}")))?;

        let req = match self.auth_header() {
            Some(auth) => client.get(&url).header("Authorization", auth),
            None => client.get(&url),
        };

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::runtime(format!("Clash API group delay test failed: {e}")))?;

        if !resp.status().is_success() {
            return self.fetch_all_delays().await;
        }

        let data: HashMap<String, u32> = resp.json().await.unwrap_or_default();
        Ok(data)
    }
}

fn ensure_group_interrupts(config: &mut Value) {
    let Some(outbounds) = config.get_mut("outbounds").and_then(Value::as_array_mut) else {
        return;
    };

    for outbound in outbounds {
        let Some(obj) = outbound.as_object_mut() else {
            continue;
        };

        let outbound_type = obj.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(outbound_type, "selector" | "urltest") {
            obj.insert("interrupt_exist_connections".to_string(), Value::Bool(true));
        }
    }
}

fn urlencoding_minimal(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('&', "%26")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proxy_group_status_shape() {
        let payload = r#"{
            "proxies": {
                "Proxy": {
                    "type": "Selector",
                    "now": "JP-1",
                    "all": ["Auto", "HK-1", "JP-1"],
                    "history": []
                }
            }
        }"#;
        let data: ProxiesResponse = serde_json::from_str(payload).unwrap();
        let proxy = data.proxies.get("Proxy").unwrap();
        assert_eq!(proxy.now, "JP-1");
        assert_eq!(proxy.members, vec!["Auto", "HK-1", "JP-1"]);
    }

    #[test]
    fn inject_clash_api_defaults_enables_group_interrupts() {
        let mut config = serde_json::json!({
            "outbounds": [
                {
                    "type": "selector",
                    "tag": "Proxy",
                    "outbounds": ["Auto", "JP-1"],
                    "interrupt_exist_connections": false
                },
                {
                    "type": "urltest",
                    "tag": "Auto",
                    "outbounds": ["JP-1"]
                },
                {
                    "type": "direct",
                    "tag": "direct"
                }
            ]
        });

        let addr = inject_clash_api_defaults(&mut config);

        assert_eq!(addr, DEFAULT_EXTERNAL_CONTROLLER);
        assert_eq!(
            config["experimental"]["clash_api"]["external_controller"],
            DEFAULT_EXTERNAL_CONTROLLER
        );
        assert_eq!(config["experimental"]["cache_file"]["enabled"], true);
        assert_eq!(config["outbounds"][0]["interrupt_exist_connections"], true);
        assert_eq!(config["outbounds"][1]["interrupt_exist_connections"], true);
        assert!(
            config["outbounds"][2]
                .get("interrupt_exist_connections")
                .is_none()
        );
    }
}
