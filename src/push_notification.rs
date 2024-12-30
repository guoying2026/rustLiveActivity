// src/push_notification.rs
use serde::Serialize;
use reqwest::Client;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PushNotificationError {
    #[error("HTTP request failed: {0}")]
    HttpRequestError(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Custom error: {0}")]
    CustomError(String),
}

#[derive(Serialize)]
pub struct LiveActivityContentState {
    pub(crate) blue_url: String,
    pub(crate) red_url: String,
    pub(crate) title: String,
    pub(crate) content: String,
    pub(crate) token_price: Vec<TokenPrice>,
    pub(crate) market_text: String,
    pub(crate) typeTitle: String,
    pub(crate) total_market_cap: f64,
    pub(crate) market_cap_change24h_usd: f64,
    pub(crate) url: String,
}

#[derive(Serialize)]
pub struct Alert {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) sound: String,
}

#[derive(Serialize)]
pub struct LiveActivity {
    pub(crate) event: String,
    #[serde(rename = "content-state")]
    pub(crate) content_state: LiveActivityContentState,
    pub(crate) alert: Alert,
    #[serde(rename = "dismissal-date")]
    pub(crate) dismissal_date: i64,
}

#[derive(Serialize)]
pub struct TokenPrice {
    pub(crate) name: String,
    pub(crate) price: String,
    pub(crate) change: f64,
    pub(crate) url: String,
}

pub async fn send_push_notification(
    platform: &[&str],
    audience: &HashMap<&str, &str>,
    live_activity: &LiveActivity,
    options: &HashMap<&str, serde_json::Value>,
) -> Result<(), PushNotificationError> {
    let client = Client::new();

    let payload = serde_json::json!({
        "platform": platform,
        "audience": audience,
        "live_activity": live_activity,
        "options": options,
    });

    // 假设推送通知的 API 端点
    let response = client
        .post("https://api.pushservice.com/send")
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(PushNotificationError::CustomError(format!(
            "Failed to send push notification: {}",
            response.text().await.unwrap_or_default()
        )))
    }
}