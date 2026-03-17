use crate::config::Config;
use crate::state::SharedState;
use crate::web::routes;
use axum::{
    http::{header::CONTENT_TYPE, Method},
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;

pub async fn start_web_server(config: Config, state: SharedState) -> anyhow::Result<()> {
    let port = config.web_port;
    let bind_host = config.bind_host.clone();
    let bind_address = format!("{}:{}", config.bind_host, port);
    let allowed_origins = build_allowed_origins(&config.allowed_origins)?;
    let shared = routes::AppContext {
        config: std::sync::Arc::new(config),
        state,
    };

    let app = Router::new()
        // 静态前端
        .route("/", get(routes::index_handler))
        // API 路由
        .route("/api/search", post(routes::search_handler))
        .route("/api/downloads", get(routes::list_downloads_handler))
        .route("/api/downloads/{id}", get(routes::get_download_handler))
        .route("/api/download", post(routes::start_download_handler))
        .route("/api/download/batch", post(routes::batch_download_handler))
        .route(
            "/api/log/level",
            get(routes::get_log_level_handler).post(routes::set_log_level_handler),
        )
        .with_state(shared);

    let app = if let Some(origins) = allowed_origins {
        app.layer(
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([CONTENT_TYPE]),
        )
    } else {
        app
    };

    println!("🎵 音乐下载器 Web 服务启动: http://{}:{}", bind_host, port);
    println!("   按 Ctrl+C 停止服务\n");

    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("\n👋 服务已停止");
    Ok(())
}

/// 监听 Ctrl+C（Windows / Unix 通用）
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

fn build_allowed_origins(
    origins: &[String],
) -> anyhow::Result<Option<Vec<axum::http::HeaderValue>>> {
    if origins.is_empty() {
        return Ok(None);
    }

    let parsed = origins
        .iter()
        .map(|origin| {
            axum::http::HeaderValue::from_str(origin)
                .map_err(|e| anyhow::anyhow!("Invalid origin '{}': {}", origin, e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(parsed))
}
