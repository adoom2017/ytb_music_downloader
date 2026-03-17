use crate::logger::LogReloadHandle;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

/// 单条搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    pub duration: Option<u64>, // 秒
    pub channel: Option<String>,
    pub thumbnail: Option<String>,
    pub view_count: Option<u64>,
    /// 预估文件大小（字节），搜索时不一定有
    pub filesize_approx: Option<u64>,
    /// 内容类型: "video" | "audio" | "video+audio"
    pub media_type: Option<String>,
    /// 原始网页链接（YouTube watch URL）
    pub webpage_url: Option<String>,
    /// 上传日期 YYYYMMDD
    pub upload_date: Option<String>,
}

impl VideoInfo {
    #[allow(dead_code)]
    pub fn duration_display(&self) -> String {
        match self.duration {
            Some(secs) => {
                let m = secs / 60;
                let s = secs % 60;
                if m >= 60 {
                    format!("{}:{:02}:{:02}", m / 60, m % 60, s)
                } else {
                    format!("{}:{:02}", m, s)
                }
            }
            None => "--:--".to_string(),
        }
    }
}

/// 下载任务状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    Queued,
    Downloading { progress: f32 },
    Converting,
    Done { file_path: String },
    Failed { error: String },
}

/// 单条下载记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub video: VideoInfo,
    pub status: DownloadStatus,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl DownloadTask {
    pub fn new(video: VideoInfo) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            video,
            status: DownloadStatus::Queued,
            created_at: Utc::now(),
            finished_at: None,
        }
    }
}

/// 搜索状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SearchState {
    Idle,
    Searching,
    Done,
    Error(String),
}

/// 全局共享应用状态（TUI 和 Web 共用）
#[derive(Debug)]
pub struct AppState {
    /// 最后一次搜索的关键词
    pub last_query: String,
    /// 搜索状态
    pub search_state: SearchState,
    /// 搜索结果列表
    pub search_results: Vec<VideoInfo>,
    /// 下载队列 / 历史（id -> task）
    pub downloads: HashMap<String, DownloadTask>,
    /// 保持下载顺序的 id 列表
    pub download_order: Vec<String>,
    /// 日志运行时动态过滤句柄
    pub log_handle: Option<LogReloadHandle>,
    /// 当前启用的日志级别显示字符
    pub current_log_level: String,
    /// 全局下载并发限制
    pub download_limiter: Arc<Semaphore>,
}

impl Default for SearchState {
    fn default() -> Self {
        SearchState::Idle
    }
}

impl AppState {
    pub fn new(concurrent_downloads: usize) -> Self {
        Self {
            last_query: String::new(),
            search_state: SearchState::Idle,
            search_results: Vec::new(),
            downloads: HashMap::new(),
            download_order: Vec::new(),
            log_handle: None,
            current_log_level: String::new(),
            download_limiter: Arc::new(Semaphore::new(concurrent_downloads.max(1))),
        }
    }

    pub fn add_download(&mut self, task: DownloadTask) -> String {
        let id = task.id.clone();
        self.download_order.push(id.clone());
        self.downloads.insert(id.clone(), task);
        id
    }

    pub fn update_download_status(&mut self, id: &str, status: DownloadStatus) {
        if let Some(task) = self.downloads.get_mut(id) {
            if matches!(
                status,
                DownloadStatus::Done { .. } | DownloadStatus::Failed { .. }
            ) {
                task.finished_at = Some(Utc::now());
            }
            task.status = status;
        }
    }

    /// 获取有序的下载列表（最新在前）
    pub fn downloads_ordered(&self) -> Vec<&DownloadTask> {
        self.download_order
            .iter()
            .rev()
            .filter_map(|id| self.downloads.get(id))
            .collect()
    }
}

/// 线程安全的共享状态句柄
pub type SharedState = Arc<Mutex<AppState>>;

pub fn new_shared_state(
    log_handle: LogReloadHandle,
    initial_level: String,
    concurrent_downloads: usize,
) -> SharedState {
    let mut state = AppState::new(concurrent_downloads);
    state.log_handle = Some(log_handle);
    state.current_log_level = initial_level;
    Arc::new(Mutex::new(state))
}
