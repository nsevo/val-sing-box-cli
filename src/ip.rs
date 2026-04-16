use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct IpInfo {
    pub ip: String,
    pub country: String,
    pub city: String,
}

impl IpInfo {
    pub fn location_display(&self) -> String {
        format!("{} · {}", self.country, self.city)
    }
}

pub async fn detect_exit_ip() -> Option<IpInfo> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .ok()?;

    let resp = client
        .get("https://1.1.1.1/cdn-cgi/trace")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;

    let mut ip = None;
    let mut loc = None;
    let mut colo = None;

    for line in resp.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "ip" => ip = Some(v.to_string()),
                "loc" => loc = Some(v.to_string()),
                "colo" => colo = Some(v.to_string()),
                _ => {}
            }
        }
    }

    Some(IpInfo {
        ip: ip?,
        country: loc?,
        city: colo?,
    })
}
