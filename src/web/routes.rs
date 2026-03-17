use crate::config::Config;
use crate::download;
use crate::search;
use crate::state::{DownloadTask, SharedState, VideoInfo};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Axum 共享上下文
#[derive(Clone)]
pub struct AppContext {
    pub config: Arc<Config>,
    pub state: SharedState,
}

// ─── 前端 HTML ───────────────────────────────────────────────────────────────

pub async fn index_handler() -> impl IntoResponse {
    Html(include_str!("../../assets/index.html"))
}

// ─── API 数据结构 ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub max_results: Option<usize>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub results: Vec<VideoInfo>,
    pub query: String,
}

#[derive(Deserialize)]
pub struct DownloadRequest {
    pub video: VideoInfo,
}

#[derive(Deserialize)]
pub struct BatchDownloadRequest {
    pub videos: Vec<VideoInfo>,
}

#[derive(Serialize)]
pub struct DownloadStarted {
    pub task_id: String,
    pub title: String,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ─── Handler 实现 ─────────────────────────────────────────────────────────────

/// POST /api/search
pub async fn search_handler(
    State(ctx): State<AppContext>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<ErrorResponse>)> {
    let max = req.max_results.unwrap_or(ctx.config.max_search_results);

    // 更新搜索状态
    {
        let mut app = ctx.state.lock().await;
        app.last_query = req.query.clone();
        app.search_state = crate::state::SearchState::Searching;
        app.search_results.clear();
    }

    match search::search_youtube(&req.query, max, &ctx.config).await {
        Ok(results) => {
            let mut app = ctx.state.lock().await;
            app.search_results = results.clone();
            app.search_state = crate::state::SearchState::Done;
            Ok(Json(SearchResponse {
                results,
                query: req.query,
            }))
        }
        Err(e) => {
            let mut app = ctx.state.lock().await;
            app.search_results.clear();
            app.search_state = crate::state::SearchState::Error(e.to_string());
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

/// POST /api/download  (单曲)
pub async fn start_download_handler(
    State(ctx): State<AppContext>,
    Json(req): Json<DownloadRequest>,
) -> Result<Json<DownloadStarted>, (StatusCode, Json<ErrorResponse>)> {
    validate_download_request(&req.video)?;

    let task = DownloadTask::new(req.video.clone());
    let task_id = task.id.clone();
    let title = req.video.title.clone();

    {
        let mut app = ctx.state.lock().await;
        app.add_download(task);
    }

    // 后台启动下载
    let state = ctx.state.clone();
    let config = (*ctx.config).clone();
    let id = task_id.clone();
    tokio::spawn(async move {
        download::download_video(id, req.video, config, state).await;
    });

    Ok(Json(DownloadStarted { task_id, title }))
}

/// POST /api/download/batch  (批量)
pub async fn batch_download_handler(
    State(ctx): State<AppContext>,
    Json(req): Json<BatchDownloadRequest>,
) -> Result<Json<Vec<DownloadStarted>>, (StatusCode, Json<ErrorResponse>)> {
    for video in &req.videos {
        validate_download_request(video)?;
    }

    let mut responses = Vec::new();

    for video in req.videos {
        let task = DownloadTask::new(video.clone());
        let task_id = task.id.clone();
        let title = video.title.clone();

        {
            let mut app = ctx.state.lock().await;
            app.add_download(task);
        }

        let state = ctx.state.clone();
        let config = (*ctx.config).clone();
        let id = task_id.clone();
        tokio::spawn(async move {
            download::download_video(id, video, config, state).await;
        });

        responses.push(DownloadStarted { task_id, title });
    }

    Ok(Json(responses))
}

/// GET /api/downloads
pub async fn list_downloads_handler(
    State(ctx): State<AppContext>,
) -> Json<Vec<crate::state::DownloadTask>> {
    let app = ctx.state.lock().await;
    let tasks: Vec<_> = app.downloads_ordered().into_iter().cloned().collect();
    Json(tasks)
}

/// GET /api/downloads/:id
pub async fn get_download_handler(
    State(ctx): State<AppContext>,
    Path(id): Path<String>,
) -> Result<Json<crate::state::DownloadTask>, StatusCode> {
    let app = ctx.state.lock().await;
    match app.downloads.get(&id) {
        Some(task) => Ok(Json(task.clone())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

use tracing_subscriber::filter::EnvFilter;

#[derive(Serialize)]
pub struct LogLevelResponse {
    pub level: String,
}

#[derive(Deserialize)]
pub struct LogLevelRequest {
    pub level: String,
}

pub async fn get_log_level_handler(State(ctx): State<AppContext>) -> Json<LogLevelResponse> {
    let state = ctx.state.lock().await;
    Json(LogLevelResponse {
        level: state.current_log_level.clone(),
    })
}

pub async fn set_log_level_handler(
    State(ctx): State<AppContext>,
    Json(req): Json<LogLevelRequest>,
) -> Result<Json<LogLevelResponse>, (StatusCode, String)> {
    let mut state = ctx.state.lock().await;

    // Validate level
    let level_str = req.level.to_lowercase();
    if !["trace", "debug", "info", "warn", "error"].contains(&level_str.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Invalid log level".to_string()));
    }

    if let Some(handle) = &state.log_handle {
        let new_filter = EnvFilter::new(format!("ytb_music_downloader={}", level_str));
        if handle.reload(new_filter).is_ok() {
            state.current_log_level = level_str.clone();
            tracing::info!("Log level changed via Web API to {}", level_str);
        }
    }

    Ok(Json(LogLevelResponse { level: level_str }))
}

fn validate_download_request(video: &VideoInfo) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let source_url = download::download_source_url(video);
    if download::is_allowed_media_url(source_url) {
        return Ok(());
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "Only YouTube URLs are allowed".to_string(),
        }),
    ))
}
