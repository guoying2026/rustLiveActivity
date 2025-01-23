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

// 注意：这里引入你需要用到的结构
use push_notification::{
    send_push_notification,
    // 三种结构体
    LiveActivityStart,
    LiveActivityUpdate,
    LiveActivityEnd,
    // 公用的部分
    LiveActivityContentState,
    LiveActivityAttributes,
    Alert,
    TokenPrice,
    // 枚举
    LiveActivityEnum,
};

/// 请求体：Rust 只接收，不做处理
#[derive(Deserialize, Clone, Debug)]
struct AddRequest {
    ios_live_activity_ids: Vec<String>,
    token: Option<Vec<TokenPriceInput>>,
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
    apns_production: Option<bool>,
    dismissal_date: Option<i64>,
    event: Option<String>,
    sound: Option<String>,
    attributes_name: Option<String>,
    attributes_type: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
struct TokenPriceInput {
    name: String,
    price: String,
    change: String,
    url: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // 初始化日志
    env_logger::init();
    dotenv().ok();

    println!("Starting Rust API server on http://0.0.0.0:11116");

    HttpServer::new(|| {
        App::new()
            // 路由，处理 POST /send_live_activity
            .route("/send_live_activity", web::post().to(live_activity))
    })
        .bind("0.0.0.0:11116")?
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
    let mut token_price_vec = Vec::new();
    if let Some(token_array) = &data.token {
        for item in token_array {
            // item 即 TokenPriceInput { name, price, change, url }
            token_price_vec.push(TokenPrice {
                name: item.name.clone(),     // 直接复制
                price: item.price.clone(),
                change: item.change.clone(),
                url: item.url.clone(),
            });
        }
    }

    // 4. 从请求里取出各种 Option<String>，转成 String
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
    let apns_production = data.apns_production.unwrap_or(false);
    let dismissal_date_tmp = data.dismissal_date.unwrap_or_default();
    let event_str = data.event.clone().unwrap_or_default();
    let sound_str = data.sound.clone().unwrap_or_default();
    let attributes_name_str = data.attributes_name.clone().unwrap_or_default();
    let attributes_type_str = data.attributes_type.clone().unwrap_or_default();

    // 5. 并发推送 (可选：改成固定并发量，而不是 ios_live_activity_ids.len())
    //    比如这里限制每次最多并发20:
    let max_concurrent = 20;
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut push_tasks = FuturesUnordered::new();

    for (i, live_activity_id) in ios_live_activity_ids.into_iter().enumerate() {
        info!(
            "即将创建第 {}/{} 个推送任务: {}",
            i + 1,
            // 如果你改成固定并发数20的话，这里还是可以用 i+1, 并不知道总的 concurrency
            // 不过你想打印总数的话，可以先 let total_ids = data.ios_live_activity_ids.len();
            // 再放进这里。
            max_concurrent,
            live_activity_id
        );

        // 这里克隆，供异步任务使用
        let sem_clone = semaphore.clone();
        let token_price_vec_clone = token_price_vec.clone();

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
        let dismissal_date = dismissal_date_tmp; // i64
        let event_str_inner = event_str.clone();
        let sound_str_inner = sound_str.clone();
        let attributes_name_inner = attributes_name_str.clone();
        let attributes_type_inner = attributes_type_str.clone();

        push_tasks.push(tokio::spawn(async move {
            let _permit = sem_clone.acquire().await.unwrap();

            info!(
                "开始执行第 {} 个任务, live_activity_id = {}",
                i + 1,
                live_activity_id
            );

            // 根据 event_str，构造三种不同的枚举分支
            let live_activity_enum = match event_str_inner.as_str() {
                "start" => {
                    LiveActivityEnum::Start(
                        LiveActivityStart {
                            event: "start".to_string(),
                            content_state: LiveActivityContentState {
                                blue_url: b_str.clone(),
                                red_url: r_str.clone(),
                                title: t_str.clone(),
                                content: c_str.clone(),
                                token_price: token_price_vec_clone,
                                market_text: market_t.clone(),
                                type_title: tt_str.clone(),
                                total_market_cap: tmc.clone(),
                                market_cap_change24h_usd: mcu.clone(),
                                time: time_c.clone(),
                                url: url_c.clone(),
                            },
                            alert: Alert {
                                title: t_str.clone(),
                                body: c_str.clone(),
                                sound: sound_str_inner.clone(),
                            },
                            attributes_type: attributes_type_inner.clone(),
                            attributes: LiveActivityAttributes {
                               name: attributes_name_inner.clone(),
                            },
                        }
                    )
                }

                "update" => {
                    LiveActivityEnum::Update(
                        LiveActivityUpdate {
                            event: "update".to_string(),
                            content_state: LiveActivityContentState {
                                blue_url: b_str.clone(),
                                red_url: r_str.clone(),
                                title: t_str.clone(),
                                content: c_str.clone(),
                                token_price: token_price_vec_clone,
                                market_text: market_t.clone(),
                                type_title: tt_str.clone(),
                                total_market_cap: tmc.clone(),
                                market_cap_change24h_usd: mcu.clone(),
                                time: time_c.clone(),
                                url: url_c.clone(),
                            },
                            alert: Alert {
                                title: t_str.clone(),
                                body: c_str.clone(),
                                sound: sound_str_inner.clone(),
                            },
                        }
                    )
                }

                "end" => {
                    LiveActivityEnum::End(
                        LiveActivityEnd {
                            event: "end".to_string(),
                            content_state: LiveActivityContentState {
                                blue_url: b_str.clone(),
                                red_url: r_str.clone(),
                                title: t_str.clone(),
                                content: c_str.clone(),
                                token_price: token_price_vec_clone,
                                market_text: market_t.clone(),
                                type_title: tt_str.clone(),
                                total_market_cap: tmc.clone(),
                                market_cap_change24h_usd: mcu.clone(),
                                time: time_c.clone(),
                                url: url_c.clone(),
                            },
                            alert: Alert {
                                title: t_str.clone(),
                                body: c_str.clone(),
                                sound: sound_str_inner.clone(),
                            },
                            dismissal_date,
                        }
                    )
                }

                _ => {
                    let err_msg = format!("Unsupported event type: {}", event_str_inner);
                    error!("{}", err_msg);
                    return Err(err_msg);
                }
            };

            // APNs 环境选项
            let options = HashMap::from([
                ("apns_production", serde_json::json!(apns_production)),
            ]);

            // 调用推送函数
            match send_push_notification(
                &["ios"],
                &live_activity_id,
                &live_activity_enum,
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
                    Ok(())
                }
                Err(e) => {
                    let err_msg = format!(
                        "发送第 {} 个任务失败, live_activity_id = {}, error: {:?}",
                        i + 1,
                        live_activity_id,
                        e
                    );
                    error!("{}", err_msg);
                    Err(err_msg)
                }
            }
        }));
    }

    info!("共创建了任务等待执行(并发控制) ...");

    while let Some(res) = push_tasks.next().await {
        match res {
            Ok(Ok(())) => info!("任务成功完成"),
            Ok(Err(e)) => error!("任务逻辑失败: {}", e),
            Err(e) => error!("任务运行失败: {:?}", e),
        }
    }

    info!("所有任务都已执行完成");
    Ok(HttpResponse::Ok().body("Live activity sent successfully"))
}