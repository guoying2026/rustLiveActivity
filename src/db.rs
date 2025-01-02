use sqlx::{mysql::MySqlPoolOptions, MySqlPool};
use std::env;
use dotenv::dotenv;
use sqlx::mysql::MySqlConnectOptions;

pub async fn get_db_pool() -> MySqlPool {
    dotenv().ok();
    let host = env::var("DB_HOST").expect("HOST must be set");
    let user = env::var("DB_USER").expect("USER must be set");
    let password = env::var("DB_PASSWORD").expect("PASSWORD must be set");
    let database = env::var("DB_DATABASE").expect("DATABASE must be set");

    // 手动构建连接参数
    let conn_options = MySqlConnectOptions::new()
        .host(&host)
        .username(&user)
        .password(&password)
        .database(&database);
    
    MySqlPoolOptions::new()
        .max_connections(5)
        .connect_with(conn_options)
        .await
        .expect("Failed to create pool")
}