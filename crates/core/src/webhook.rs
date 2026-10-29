use std::time::Duration;

pub const WEBHOOK_TIMEOUT_MS: u64 = 5_000;

/// Fire-and-forget JSON webhook, mirroring the Node helper.
pub fn send_webhook(url: &str, body: serde_json::Value) {
    let url = url.to_string();
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_millis(WEBHOOK_TIMEOUT_MS))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[webhook] failed to build client: {e}");
                return;
            }
        };
        match client.post(&url).json(&body).send().await {
            Ok(response) if !response.status().is_success() => {
                tracing::warn!("[webhook] HTTP {}", response.status());
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("[webhook] failed: {e}"),
        }
    });
}
