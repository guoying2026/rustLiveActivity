// src/main.rs
mod models;
mod db;
mod push_notification;
mod utils;

use actix_web::{web, App, HttpResponse, HttpServer, Error, HttpRequest};
use serde::{Deserialize};
use models::IosLiveActivityContent;
use push_notification::{send_push_notification, LiveActivity, LiveActivityContentState, Alert, TokenPrice};
use utils::{format_decimal, deal_number, format_percentage};
use std::collections::HashMap;
use chrono::{NaiveDate, Utc};
use sqlx::types::BigDecimal; // 使用 sqlx 自带的 BigDecimal
use num_traits::cast::ToPrimitive; // 导入 ToPrimitive trait
use futures::stream::FuturesUnordered;
use futures::{StreamExt, TryFutureExt};
use tokio::sync::Semaphore;
use std::sync::Arc;
use log::{info, error}; // 使用日志宏
use std::str::FromStr;
use actix_web::error::ErrorInternalServerError;
use sqlx::MySqlPool;
use crate::models::IosLiveActivitySelect;
// 需要字符串解析

#[tokio::main]
async fn main()  -> std::io::Result<()> {
    // 加载环境变量
    dotenv::dotenv().ok();
    // 初始化数据库连接池
    let pool = db::get_db_pool().await;

    println!("Starting Rust API server on http://127.0.0.1:11115");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone())) // 共享数据库池
            .route("/send_live_activity", web::post().to(live_activity)) // 定义路由
    })
        .bind("0.0.0.0:11115")?
        .run()
        .await
    // 示例数据
    // let data = get_sample_data();

    // live_activity(data).await
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
#[derive(Deserialize, Clone)]
struct AddRequest {
    id: i32,
    token: Option<HashMap<String, TokenInput>>, // 修改为 HashMap
    total_market_cap: Option<String>,
    market_cap_change24h_usd: Option<String>,
}

#[derive(Deserialize, Clone)]
struct TokenInput {
    #[serde(rename = "lastPrice")]
    last_price: f64,
    #[serde(rename = "change24h")]
    change24h: String,
}
pub async fn get_market_data(pool: &MySqlPool) -> Result<(String, String, Option<chrono::NaiveDate>), Error> {
    // 查询 `btc_etf` 表获取 `total_market_cap` 和 `market_cap_change24h_usd` 以及 `time`
    let btc_etf_result = sqlx::query!(
        r#"
        SELECT amount, total, time
        FROM btc_etf
        WHERE type = 1 and status = 1
        ORDER BY time DESC
        LIMIT 1
        "#
    )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!("Failed to query btc_etf table: {:?}", e);
            ErrorInternalServerError("Failed to query btc_etf table")
        })?;

    // 处理查询结果
    if let Some(result) = btc_etf_result {
        let total_market_cap = format!("{:.2}", result.amount.to_f64().unwrap_or(0.0)); // 处理 `BigDecimal`
        let market_cap_change24h_usd = format!("{:.2}", result.total.to_f64().unwrap_or(0.0)); // 处理 `BigDecimal`
        let time = result.time; // 这里保留时间字段
        Ok((total_market_cap, market_cap_change24h_usd, Option::from(time)))
    } else {
        // 如果查询结果为空，返回默认值
        Ok(("0.00".to_string(), "0.00".to_string(), None))
    }
}
async fn live_activity(
    pool: web::Data<sqlx::MySqlPool>,
    req: web::Json<AddRequest>,
    headers: HttpRequest,
) -> Result<HttpResponse, Error> {
    // 访问 AddRequest 数据
    let data = req.into_inner();
    // 只推送到 iOS 平台
    let platform = vec!["ios"];

    // 获取 iOS Live Activity
    let ios_live_activity: Vec<IosLiveActivitySelect> = sqlx::query_as!(
        IosLiveActivitySelect,
        "SELECT live_activity_id FROM ios_live_activity"
    )
        .fetch_all(pool.get_ref())
        .await
        .map_err(|e| {
            error!("Database query failed: {:?}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    if ios_live_activity.is_empty() {
        return Err(actix_web::error::ErrorInternalServerError("没有 live_activity_id"));
    }

    let ios_live_activity_ids: Vec<String> = ios_live_activity
        .iter()
        .map(|activity| activity.live_activity_id.clone())
        .collect();

    // 构建 tokenPrice 字符串
    let token_price = if let Some(token_map) = &data.token {
        if token_map.is_empty() {
            String::new()
        } else {
            token_map.iter().fold(String::new(), |acc, (key, v)| {
                format!("{}{}|{}|{};", acc, key, v.last_price, v.change24h)
            })
        }
    } else {
        String::new()
    };
    // 调用 `get_market_data` 获取数据
    let (total_market_cap, market_cap_change24h_usd, time) = get_market_data(pool.get_ref()).await?;
    // 构建时间字符串（如果 time 为 None，则使用默认值空字符串）
    let time_value = time.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default();
    // 将字符串克隆出来，避免所有权问题
    let total_market_cap_cloned = total_market_cap.clone();
    let market_cap_change24h_usd_cloned = market_cap_change24h_usd.clone();
    // 更新 `ios_live_activity_content` 表
    let query_result = if token_price.is_empty() {
        // 如果 token_price 为空，则更新所有字段
        sqlx::query!(
        "UPDATE ios_live_activity_content 
         SET is_send = 1, token_price = '', total_market_cap = ?, market_cap_change24h_usd = ?, time = ? 
         WHERE id = ?",
        total_market_cap,               // 填充 total_market_cap
        market_cap_change24h_usd,       // 填充 market_cap_change24h_usd
        time_value,                     // 填充格式化后的时间值
        data.id                         // 填充 id
    )
            .execute(pool.get_ref())
            .await
    } else {
        // 如果 token_price 不为空，仅更新 token_price
        sqlx::query!(
        "UPDATE ios_live_activity_content 
         SET is_send = 1, token_price = ?, total_market_cap = ?, market_cap_change24h_usd = ?, time = ? 
         WHERE id = ?",
            token_price,                    // 填充 token_price
            total_market_cap,               // 填充 total_market_cap
            market_cap_change24h_usd,       // 填充 market_cap_change24h_usd
            time_value,                     // 填充格式化后的时间值
            data.id                         // 填充 id
    )
            .execute(pool.get_ref())
            .await
    };
    // 检查更新结果并处理错误
    query_result.map_err(|e| {
        error!(
        "Failed to update ios_live_activity_content: id = {}, error: {:?}",
        data.id, e
    );
        actix_web::error::ErrorInternalServerError("Failed to update database")
    })?;

    // 获取更新后的 IosLiveActivityContent
    let ios_res: IosLiveActivityContent = sqlx::query_as!(
        IosLiveActivityContent,
        "SELECT * FROM ios_live_activity_content WHERE id = ?",
        data.id
    )
        .fetch_one(pool.get_ref())
        .await
        .map_err(|e| {
            error!("Failed to update ios_live_activity_content: {:?}", e);
            actix_web::error::ErrorInternalServerError("Failed to update database")
        })?;

    let type_field = if ios_res.is_flash != 0 { "flash" } else { "news" };
    let type_title = "实时消息";

    // 设置并发限制，例如同时最多运行 10 个任务
    let max_concurrent = 10;
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut push_tasks = FuturesUnordered::new();

    for live_activity_id in ios_live_activity_ids {
        let total_market_cap_task = total_market_cap_cloned.clone();
        let market_cap_change24h_usd_task = market_cap_change24h_usd_cloned.clone();
        // let time_task = time_value.clone();

        // 使用 `NaiveDate::parse_from_str` 解析日期
        // let date = NaiveDate::parse_from_str(&*time_task, "%Y年%m月%d日").expect("Failed to parse date");

        // 格式化为 "06月20日"
        // let formatted_date = date.format("%m月%d日").to_string();
        
        let data = data.clone(); // 克隆 data 以便在异步任务中使用
        let platform = platform.clone();
        let ios_res = ios_res.clone();
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
            } else {
                result = Vec::new();
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
                    market_text: "ETF总净流入".to_string(),
                    type_title: type_title.to_string(),
                    total_market_cap: format!("${}M", total_market_cap_task),
                    market_cap_change24h_usd: format!("${}B", market_cap_change24h_usd_task),
                    // time: formatted_date,

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

    Ok(HttpResponse::Ok().body("Live activity sent successfully"))
}