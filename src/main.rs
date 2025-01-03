mod push_notification; // 你自己的推送模块
mod utils;             // 如果有其他工具函数

use actix_web::{web, App, HttpResponse, HttpServer, Error, HttpRequest};
use serde::Deserialize;
use log::{info, error};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::sync::Semaphore;
use std::sync::Arc;
use std::collections::HashMap;

// 引入你自己的推送所需结构或函数
use push_notification::{send_push_notification, LiveActivity, LiveActivityContentState, Alert, TokenPrice};
// 假设你有一个函数 `format_decimal`，若不需要可删除
use utils::format_decimal;

/// 请求体：Rust 只接收，不做处理
#[derive(Deserialize, Clone, Debug)]
struct AddRequest {
    /// 前端会传一批 live_activity_id，我们直接拿来用
    ios_live_activity_ids: Vec<String>,

    /// 前端传的 Token 数据（如 BTC, ETH...），不做格式化
    token: Option<HashMap<String, TokenInput>>,

    /// 前端传的市值、24h 变化这些字段，同样直接使用
    total_market_cap: Option<String>,
    market_cap_change24h_usd: Option<String>,

    /// 前端可能也会传推送时需要的标题、内容等
    title: Option<String>,
    content: Option<String>,
}

/// TokenInput：例如 { "BTC": { "lastPrice": 30000.5, "change24h": "+5%" } }
#[derive(Deserialize, Clone, Debug)]
struct TokenInput {
    #[serde(rename = "lastPrice")]
    last_price: f64,

    #[serde(rename = "change24h")]
    change24h: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // 初始化日志
    env_logger::init();

    println!("Starting Rust API server on http://127.0.0.1:11115");

    HttpServer::new(|| {
        App::new()
            // 直接定义路由，不再需要数据库
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

    // 如果前端没有传 ios_live_activity_ids，直接报错
    if data.ios_live_activity_ids.is_empty() {
        return Ok(HttpResponse::BadRequest().body("没有 live_activity_id"));
    }

    // 2. 组装需要推送给哪些 id
    let ios_live_activity_ids = data.ios_live_activity_ids.clone();

    // 3. 构建 token_price 列表（如需完全“不处理”，可以直接放空或者原样塞进 content_state）
    let mut token_price_vec = Vec::new();
    if let Some(token_map) = &data.token {
        for (symbol, info) in token_map {
            token_price_vec.push(TokenPrice {
                // 假设只拼个名字和价格，这里不做任何转换
                name: format!("{}/USDT", symbol.to_uppercase()),
                price: format!("${}", format_decimal(info.last_price)), // 如果不想格式化，直接 info.last_price.to_string() 也行
                change: info.change24h.clone(),
                url: "https://p2p.binance.com/zh-CN/express/buy/ETH/CNY".to_string(),
            });
        }
    }

    // 4. 其他字段不处理、直接拿过来用
    let total_market_cap = data.total_market_cap.clone().unwrap_or_default();
    let market_cap_change24h_usd = data.market_cap_change24h_usd.clone().unwrap_or_default();
    let title = data.title.clone().unwrap_or_else(|| "实时消息".to_string());
    let content = data.content.clone().unwrap_or_else(|| "这里是默认内容".to_string());

    // 5. 并发推送
    let max_concurrent = ios_live_activity_ids.len();
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut push_tasks = FuturesUnordered::new();

    for (i, live_activity_id) in ios_live_activity_ids.into_iter().enumerate() {
        // 创建任务日志
        info!(
            "即将创建第 {}/{} 个推送任务: {}",
            i + 1,
            max_concurrent,
            live_activity_id
        );

        // 复制进任务
        let semaphore = semaphore.clone();
        let title_clone = title.clone();
        let content_clone = content.clone();
        let tmc = total_market_cap.clone();
        let mcu = market_cap_change24h_usd.clone();
        let tp_vec = token_price_vec.clone();

        push_tasks.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();

            info!(
                "开始执行第 {} 个任务, live_activity_id = {}",
                i + 1,
                live_activity_id
            );

            // 构建推送数据: 不做任何转换，直接塞进 content_state
            let live_activity = LiveActivity {
                event: "update".to_string(),
                content_state: LiveActivityContentState {
                    blue_url: "blockbeats://m.theblockbeats.info/home".to_string(),
                    red_url: "blockbeats://m.theblockbeats.info/flash/list".to_string(),
                    title: title_clone.clone(),
                    content: content_clone.clone(),
                    token_price: tp_vec, // 从前端原样转来的 token
                    market_text: "ETF总净流入".to_string(),
                    type_title: "实时消息".to_string(),

                    // 不再对 total_market_cap / market_cap_change24h_usd 做任何单位转换
                    total_market_cap: tmc,
                    market_cap_change24h_usd: mcu,

                    // 如果前端不传时间，这里就写死或留空
                    time: "".to_string(),

                    // 你也可以允许前端传 url
                    url: format!("blockbeats://m.theblockbeats.info/news?id=9999"),
                },
                alert: Alert {
                    title: title_clone.clone(),
                    body: content_clone.clone(),
                    sound: "default".to_string(),
                },
                // 4小时后失效
                dismissal_date: chrono::Utc::now().timestamp() + 4 * 3600,
            };

            // 从环境变量读取 APNS_PRODUCTION
            let apns_production = std::env::var("APNS_PRODUCTION")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .unwrap_or(false);

            let options = HashMap::from([
                ("apns_production", serde_json::json!(apns_production)),
                ("time_to_live", serde_json::json!(86400)),
            ]);

            // 调用你自己的推送函数
            match send_push_notification(
                &["ios"],
                live_activity_id.as_str(),
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

    // 等待所有并发任务执行完成
    while let Some(res) = push_tasks.next().await {
        if let Err(e) = res {
            error!("推送任务执行失败: {:?}", e);
        }
    }

    info!("所有任务都已执行完成");
    Ok(HttpResponse::Ok().body("Live activity sent successfully"))
}