use std::{env, fs, net::SocketAddr};

use kanban::server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/kanban.db?mode=rwc".to_owned());
    if database_url.contains("data/kanban.db") {
        fs::create_dir_all("data")?;
    }
    let pool = server::connect(&database_url).await?;
    let bind_addr: SocketAddr = env::var("KANBAN_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
        .parse()?;
    let web_dir = env::var("KANBAN_WEB_DIR")
        .unwrap_or_else(|_| "target/dx/kanban/release/web/public".to_owned());
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    println!("Kanban is listening on http://{bind_addr}");
    axum::serve(listener, server::router(pool, &web_dir)).await?;
    Ok(())
}
