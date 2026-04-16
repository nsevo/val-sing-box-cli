use crate::errors::{AppError, AppResult};

pub async fn fetch_subscription(url: &str, sing_box_version: &str) -> AppResult<String> {
    let user_agent = format!("sing-box {sing_box_version} valsb-cli");

    let client = reqwest::Client::builder()
        .user_agent(&user_agent)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::network(format!("failed to build HTTP client: {e}")))?;

    let response = client.get(url).send().await.map_err(|e| {
        AppError::network_with_hint(
            format!("failed to fetch subscription: {e}"),
            "check subscription URL and network connectivity",
        )
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::network_with_hint(
            format!("subscription server returned HTTP {status}"),
            "check if the subscription URL is valid and accessible",
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| AppError::network(format!("failed to read subscription response: {e}")))?;

    if body.trim().is_empty() {
        return Err(AppError::data("subscription response is empty"));
    }

    Ok(body)
}
