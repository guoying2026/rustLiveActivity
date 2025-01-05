mod push_notification; // 你的推送模块
mod utils;             // 你的工具函数

use actix_web::{web, App, HttpResponse, HttpServer, Error, HttpRequest};
use serde::Deserialize;
use log::{info, error};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::sync::Semaphore;
use std::sync::Arc;
use std::collections::HashMap;
use dotenv::dotenv;

use push_notification::{send_push_notification, LiveActivity, LiveActivityContentState, Alert, TokenPrice};

/// 请求体：Rust 只接收，不做处理
#[derive(Deserialize, Clone, Debug)]
struct AddRequest {
    ios_live_activity_ids: Vec<String>,
    token: Option<HashMap<String, TokenInput>>,
    total_market_cap: Option<String>,
    market_cap_change24h_usd: Option<String>,
    title: Option<String>,
    content: Option<String>,
    time: Option<String>,
    url: Option<String>,
    market_text: Option<String>,
    type_title: Option<String>,
    blue_url: Option<String>,
    red_url: Option<String>,
    // 新增这个字段
    apns_production: Option<bool>
}

/// TokenInput：例如 { "BTC": { "lastPrice": 30000.5, "change24h": "+5%" , "url": "..." } }
#[derive(Deserialize, Clone, Debug)]
struct TokenInput {
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "change24h")]
    change24h: String,
    url: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // 初始化日志
    env_logger::init();
    dotenv().ok();

    println!("Starting Rust API server on http://0.0.0.0:11115");

    HttpServer::new(|| {
        App::new()
            // 路由，处理 POST /send_live_activity
            .route("/send_live_activity", web::post().to(live_activity))
    })
        .bind("0.0.0.0:11115")?
        .run()
        .await
}

/// 处理 POST /send_live_activity
async fn live_activity(
    req: web::Json<AddRequest>,
    _headers: HttpRequest,
) -> Result<HttpResponse, Error> {

    // 1. 读取请求体
    let data = req.into_inner();
    info!("收到请求: {:?}", data);

    // 如果没有 live_activity_ids，直接报错
    if data.ios_live_activity_ids.is_empty() {
        return Ok(HttpResponse::BadRequest().body("没有 live_activity_id"));
    }

    // 2. 准备好要推送的 live_activity_ids
    let ios_live_activity_ids = data.ios_live_activity_ids.clone();

    // 3. 构建 token_price 列表
    //   (把 data.token 里的可选字段都拼到 TokenPrice)
    let mut token_price_vec = Vec::new();
    if let Some(token_map) = &data.token {
        for (symbol, info) in token_map {
            token_price_vec.push(TokenPrice {
                name: symbol.to_uppercase(),
                price: info.last_price.clone(),
                change: info.change24h.clone(),
                url: info.url.clone(),
            });
        }
    }

    // 4. 因为 LiveActivityContentState 的所有字段都是 String，
    //   我们要在循环外就把这些 Option<String> 转成实际的 String
    //   给默认值或者空字符串。
    let total_market_cap = data.total_market_cap.clone().unwrap_or_default();
    let market_cap_change24h_usd = data.market_cap_change24h_usd.clone().unwrap_or_default();
    let time_str = data.time.clone().unwrap_or_default();
    let page_url = data.url.clone().unwrap_or_default();
    let title_str = data.title.clone().unwrap_or_else(|| "默认标题".to_string());
    let content_str = data.content.clone().unwrap_or_else(|| "默认内容".to_string());
    let market_text_str = data.market_text.clone().unwrap_or_default();
    let type_title_str = data.type_title.clone().unwrap_or_default();
    let blue_url_str = data.blue_url.clone().unwrap_or_else(|| "blockbeats://m.theblockbeats.info/home".to_string());
    let red_url_str = data.red_url.clone().unwrap_or_else(|| "blockbeats://m.theblockbeats.info/flash/list".to_string());
    let apns_production = true;
    // 5. 并发推送
    let max_concurrent = ios_live_activity_ids.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut push_tasks = FuturesUnordered::new();

    for (i, live_activity_id) in ios_live_activity_ids.into_iter().enumerate() {

        info!(
            "即将创建第 {}/{} 个推送任务: {}",
            i + 1,
            max_concurrent,
            live_activity_id
        );

        // 这里要克隆 / 拷贝我们准备好的字符串，
        // 因为要在异步闭包里使用它们
        let sem_clone = semaphore.clone();
        let token_price_vec_clone = token_price_vec.clone(); // Vec<TokenPrice> 需要它实现 Clone

        let tmc = total_market_cap.clone();
        let mcu = market_cap_change24h_usd.clone();
        let t_str = title_str.clone();
        let c_str = content_str.clone();
        let time_c = time_str.clone();
        let url_c = page_url.clone();
        let market_t = market_text_str.clone();
        let tt_str = type_title_str.clone();
        let b_str = blue_url_str.clone();
        let r_str = red_url_str.clone();

        push_tasks.push(tokio::spawn(async move {
            let _permit = sem_clone.acquire().await.unwrap();

            info!(
                "开始执行第 {} 个任务, live_activity_id = {}",
                i + 1,
                live_activity_id
            );

            // 构建 LiveActivity
            let live_activity = LiveActivity {
                event: "update".to_string(),
                content_state: LiveActivityContentState {
                    blue_url: b_str,
                    red_url: r_str,
                    title: t_str.clone(),
                    content: c_str.clone(),
                    token_price: token_price_vec_clone,
                    market_text: market_t,
                    type_title: tt_str,
                    total_market_cap: tmc,
                    market_cap_change24h_usd: mcu,
                    time: time_c,
                    url: url_c,
                },
                alert: Alert {
                    title: t_str.clone(),
                    body: c_str.clone(),
                    sound: "default".to_string(),
                },
                dismissal_date: chrono::Utc::now().timestamp() + 4 * 3600, // 4小时后
            };

            let options = HashMap::from([
                ("apns_production", serde_json::json!(apns_production)),
                ("time_to_live", serde_json::json!(86400)),
            ]);

            // 调用推送函数
            match send_push_notification(
                &["ios"],
                &live_activity_id,
                &live_activity,
                &options
            ).await {
                Ok((status, response)) => {
                    info!(
                        "成功发送第 {} 个任务, live_activity_id = {}: HTTP {}",
                        i + 1,
                        live_activity_id,
                        status
                    );
                    info!("推送响应: {}", response);
                }
                Err(e) => error!(
                    "发送第 {} 个任务, live_activity_id = {} 失败: {:?}",
                    i + 1,
                    live_activity_id,
                    e
                ),
            }
        }));
    }

    info!("共创建了 {} 个任务等待执行", max_concurrent);

    while let Some(res) = push_tasks.next().await {
        if let Err(e) = res {
            error!("推送任务执行失败: {:?}", e);
        }
    }

    info!("所有任务都已执行完成");
    Ok(HttpResponse::Ok().body("Live activity sent successfully"))
}