// src/main.rs
mod models;
mod db;
mod push_notification;
mod utils;

use models::{IosLiveActivity, IosLiveActivityContent};
use push_notification::{send_push_notification, LiveActivity, LiveActivityContentState, Alert, TokenPrice, PushNotificationError};
use utils::{format_decimal, deal_number, format_percentage};
use std::collections::HashMap;
use chrono::Utc;
use sqlx::types::BigDecimal; // 使用 sqlx 自带的 BigDecimal
use num_traits::cast::ToPrimitive; // 导入 ToPrimitive trait
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::sync::Semaphore;
use std::sync::Arc;
use log::{info, error}; // 使用日志宏
use env_logger;
use std::str::FromStr; // 需要字符串解析

#[tokio::main]
async fn main() -> Result<(), PushNotificationError> {
    // 初始化日志
    env_logger::init();

    // 示例数据
    let data = get_sample_data();

    live_activity(data).await
}

#[derive(Clone)]
struct Data {
    id: i32,
    token: Vec<(String, TokenInfo)>,
    total_market_cap: Option<BigDecimal>,
    market_cap_change24h_usd: Option<String>,
}

#[derive(Clone)]
struct TokenInfo {
    last_price: f64,
    change24h: String,
}

fn get_sample_data() -> Data {
    Data {
        id: 124,
        token: vec![
            ("SWARMS".to_string(), TokenInfo { last_price: 0.06061590089171, change24h: "+57.29%".to_string() }),
        ],
        total_market_cap: Some(BigDecimal::from_str("3435635411867.5000000000").unwrap()),
        market_cap_change24h_usd: Some("-2.8544826685128".to_string()),
    }
}

async fn live_activity(data: Data) -> Result<(), PushNotificationError> {
    // 只推送到 iOS 平台
    let platform = vec!["ios"];
    let pool = db::get_db_pool().await;

    // 获取 iOS Live Activity
    let ios_live_activity: Vec<IosLiveActivity> = sqlx::query_as!(
        IosLiveActivity,
        "SELECT * FROM ios_live_activity"
    )
        .fetch_all(&pool)
        .await?;

    if ios_live_activity.is_empty() {
        return Err(PushNotificationError::CustomError("没有 live_activity_id".to_string()));
    }

    let ios_live_activity_ids: Vec<String> = ios_live_activity
        .iter()
        .map(|activity| activity.live_activity_id.clone())
        .collect();

    // 构建 tokenPrice 字符串
    let token_price = data.token.iter().fold(String::new(), |acc, (key, v)| {
        format!("{}{}|{}|{};", acc, key, v.last_price, v.change24h)
    });

    // 更新 IosLiveActivityContent
    if data.total_market_cap.is_none() || data.market_cap_change24h_usd.is_none() {
        sqlx::query!(
            "UPDATE ios_live_activity_content SET is_send = 1, token_price = ?, total_market_cap = 0, market_cap_change24h_usd = '' WHERE id = ?",
            token_price,
            data.id
        )
            .execute(&pool)
            .await?;
    } else {
        sqlx::query!(
            "UPDATE ios_live_activity_content SET is_send = 1, token_price = ? WHERE id = ?",
            token_price,
            data.id
        )
            .execute(&pool)
            .await?;
    }

    // 获取更新后的 IosLiveActivityContent
    let ios_res: IosLiveActivityContent = sqlx::query_as!(
        IosLiveActivityContent,
        "SELECT * FROM ios_live_activity_content WHERE id = ?",
        data.id
    )
        .fetch_one(&pool)
        .await?;

    let type_field = if ios_res.is_flash != 0 { "flash" } else { "news" };
    let type_title = "实时消息";

    // 设置并发限制，例如同时最多运行 10 个任务
    let max_concurrent = 10;
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut push_tasks = FuturesUnordered::new();

    for live_activity_id in ios_live_activity_ids {
        let platform = platform.clone();
        let ios_res = ios_res.clone();
        let data = data.clone();
        let live_activity_id = live_activity_id.clone();
        let semaphore = semaphore.clone();

        push_tasks.push(tokio::spawn(async move {
            // 获取一个许可，确保同时运行的任务不会超过限制
            let _permit = semaphore.acquire().await.unwrap();

            let audience: HashMap<&str, &str> = [("live_activity_id", live_activity_id.as_str())].iter().cloned().collect();

            // 处理 token_price
            let mut result = Vec::new();
            if !ios_res.token_price.is_empty() {
                let pairs: Vec<&str> = ios_res.token_price.trim_end_matches(';').split(';').collect();
                for pair in pairs {
                    let parts: Vec<&str> = pair.split('|').collect();
                    if parts.len() == 3 {
                        let symbol = parts[0].to_uppercase();
                        let price = format_decimal(parts[1].parse::<f64>().unwrap());
                        let change = parts[2].to_string();
                        result.push(TokenPrice {
                            name: format!("{}/USDT", symbol),
                            price,
                            change,
                            url: "https://p2p.binance.com/zh-CN/express/buy/ETH/CNY".to_string(),
                        });
                    }
                }
            }

            // 构建 LiveActivity 结构
            let live_activity = LiveActivity {
                event: "update".to_string(),
                content_state: LiveActivityContentState {
                    blue_url: "blockbeats://m.theblockbeats.info/home".to_string(),
                    red_url: "blockbeats://m.theblockbeats.info/flash/list".to_string(),
                    title: ios_res.title.clone(),
                    content: ios_res.content.clone(),
                    token_price: result,
                    market_text: "加密总市值".to_string(),
                    type_title: type_title.to_string(),
                    total_market_cap: if let Some(ref cap) = data.total_market_cap {
                        let cap_f64 = cap.to_f64().unwrap_or(0.0);
                        if cap_f64 > 0.0 {
                            deal_number(cap_f64)
                        } else {
                            "0".to_string()
                        }
                    } else {
                        "0".to_string()
                    },
                    market_cap_change24h_usd: if let Some(ref change) = data.market_cap_change24h_usd {
                        if let Ok(change_f64) = change.parse::<f64>() {
                            if change_f64 != 0.0 {
                                format_percentage(change)
                            } else {
                                "0".to_string()
                            }
                        } else {
                            "0".to_string()
                        }
                    } else {
                        "0".to_string()
                    },

                    url: format!(
                        "blockbeats://m.theblockbeats.info/{}?id={}",
                        type_field, ios_res.article_id
                    ),
                },
                alert: Alert {
                    title: ios_res.title.clone(),
                    body: ios_res.content.clone(),
                    sound: "default".to_string(),
                },
                dismissal_date: Utc::now().timestamp() + 4 * 3600, // 4小时后
            };

            // 构建推送选项
            let options = HashMap::from([
                ("apns_production", serde_json::json!(true)),
                ("time_to_live", serde_json::json!(86400)),
            ]);

            // 发送推送通知
            match send_push_notification(&platform, &audience, &live_activity, &options).await {
                Ok((status, response)) => {
                    info!("成功发送推送通知给 {}: HTTP {}", live_activity_id, status);
                    info!("推送响应: {}", response);
                },
                Err(e) => error!("发送推送通知给 {} 失败: {:?}", live_activity_id, e),
            }
        }));
    }

    // 处理所有推送任务
    while let Some(res) = push_tasks.next().await {
        match res {
            Ok(_) => (), // 任务成功完成
            Err(e) => error!("推送任务执行失败: {:?}", e),
        }
    }

    Ok(())
}