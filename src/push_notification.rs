// src/push_notification.rs
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;
use reqwest::Client;

#[derive(Error, Debug)]
pub enum PushNotificationError {
    #[error("HTTP request failed: {0}")]
    HttpRequestError(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Configuration error: {0}")]
    ConfigError(String),
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
    #[serde(rename = "typeTitle")]
    pub(crate) type_title: String,
    pub(crate) total_market_cap: String,
    pub(crate) market_cap_change24h_usd: String,
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
    pub(crate) change: String,
    pub(crate) url: String,
}

pub async fn send_push_notification(
    platform: &[&str],
    audience: &HashMap<&str, &str>,
    live_activity: &LiveActivity,
    options: &HashMap<&str, serde_json::Value>,
) -> Result<(u16, String), PushNotificationError> {
    // 从环境变量读取极光推送的 Key 和 Secret
    let push_key = std::env::var("JG_PUSH_KEY")
        .map_err(|_| PushNotificationError::ConfigError("JG_PUSH_KEY not set".to_string()))?;
    let push_secret = std::env::var("JG_PUSH_SECRET")
        .map_err(|_| PushNotificationError::ConfigError("JG_PUSH_SECRET not set".to_string()))?;

    // 构造请求数据
    let payload = serde_json::json!({
        "platform": platform, // 平台
        "audience": audience, // 推送目标
        "live_activity": {
            "ios": live_activity
        }, // 通知内容
        "options": options, // 其他选项，如是否生产环境等
    });

    // 序列化 payload 为 JSON 字符串
    let payload_str = serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|_| "Failed to serialize payload".to_string());

    // println!("Push API request payload: {}", payload_str);

    // Ok((0, "1".to_string()))

    // 初始化 HTTP 客户端
    let client = Client::new();

    // 发送 POST 请求到极光推送 API
    let response = client
        .post("https://api.jpush.cn/v3/push")
        .basic_auth(push_key, Some(push_secret)) // 设置 Basic Auth
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    // 获取响应状态码和内容
    // 获取响应状态码和响应体
    let status = response.status();
    let response_text = response.text().await.unwrap_or_default();

    // 记录响应信息
    log::info!("Push API response status: {}, body: {}", status, response_text);

    if status.is_success() {
        Ok((status.as_u16(), response_text))
    } else {
        Err(PushNotificationError::CustomError(format!(
            "Failed to send push notification: {}",
            response_text
        )))
    }
}