use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 下载目录
    pub download_dir: PathBuf,
    /// 同时下载并发数
    pub concurrent_downloads: usize,
    /// 每次搜索最大结果数
    pub max_search_results: usize,
    /// 音频格式
    pub audio_format: String,
    /// 音频质量 (0=最高)
    pub audio_quality: String,
    /// Web 服务器端口
    pub web_port: u16,
    /// Web 服务器监听地址
    pub bind_host: String,
    /// 显式允许的跨域来源，空列表表示不启用 CORS
    pub allowed_origins: Vec<String>,
    /// yt-dlp 可执行文件路径
    pub ytdlp_path: String,
    /// 日志级别 (trace, debug, info, warn, error)
    pub log_level: String,
    /// 日志保存目录
    pub log_dir: PathBuf,
    /// 日志保留天数
    pub log_keep_days: u32,
}

impl Default for Config {
    fn default() -> Self {
        let download_dir = std::env::var("DOWNLOAD_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                // 优先使用项目根目录下的 downloads 文件夹
                let mut p = std::env::current_dir().unwrap_or_default();
                p.push("downloads");
                p
            });

        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|origin| !origin.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Config {
            download_dir,
            concurrent_downloads: 2,
            max_search_results: 10,
            audio_format: "mp3".to_string(),
            audio_quality: "0".to_string(),
            web_port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            bind_host: std::env::var("BIND_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            allowed_origins,
            ytdlp_path: std::env::var("YTDLP_PATH").unwrap_or_else(|_| "yt-dlp".to_string()),
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "debug".to_string()),
            log_dir: std::env::var("LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let mut p = std::env::current_dir().unwrap_or_default();
                    p.push("logs");
                    p
                }),
            log_keep_days: std::env::var("LOG_KEEP_DAYS")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(7),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::default()
    }

    /// 确保下载目录存在
    pub fn ensure_download_dir(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.download_dir)?;
        Ok(())
    }
}
