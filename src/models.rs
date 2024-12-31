// src/models.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{NaiveDate, NaiveDateTime};
use sqlx::types::BigDecimal; // 使用 sqlx 自带的 BigDecimal

pub struct IosLiveActivitySelect {
    pub live_activity_id: String,
}
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct IosLiveActivity {
    pub id: i32,
    pub live_activity_id: String,
    pub create_time: NaiveDateTime,
    pub update_time: NaiveDateTime,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct IosLiveActivityContent {
    pub id: i32,
    pub article_id: i32, // int
    pub title: String,
    pub content: String,
    pub is_flash: i32, // int
    pub token_price: String,
    pub is_send: i32, // int
    pub total_market_cap: BigDecimal, // decimal(30,10)
    pub market_cap_change24h_usd: String,
    pub time: Option<NaiveDate>,
    pub create_time: NaiveDateTime,
    pub update_time: NaiveDateTime,
}