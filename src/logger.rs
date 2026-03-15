use crate::config::Config;
use anyhow::{Context, Result};
use std::fs;
use std::time::{Duration, SystemTime};
use tracing_subscriber::{
    EnvFilter,
    fmt,
    prelude::*,
    reload,
    Registry,
};

// 导出重载句柄类型，供 Web API 和 TUI 使用
pub type LogReloadHandle = reload::Handle<EnvFilter, Registry>;

pub fn init_logger(config: &Config, is_tui: bool) -> Result<LogReloadHandle> {
    // 1. 确保日志目录存在并清理过期日志
    fs::create_dir_all(&config.log_dir).context("Failed to create log directory")?;
    cleanup_old_logs(config);

    // 2. 解析初始日志级别
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("ytb_music_downloader={}", config.log_level)));

    // 3. 创建 reload 层
    let (filter_layer, reload_handle) = reload::Layer::new(filter);

    // 4. 创建控制台输出层
    let console_layer = if is_tui {
        None
    } else {
        Some(fmt::layer().compact().with_target(false))
    };

    // 5. 创建按天滚动的日志文件层
    let file_appender = tracing_appender::rolling::daily(&config.log_dir, "app.log");
    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false) // 文件日志不需要转义颜色码
        .with_target(true);

    // 6. 注册全局订阅者
    Registry::default()
        .with(filter_layer)
        .with(console_layer)
        .with(file_layer)
        .try_init()
        .context("Failed to initialize tracing subscriber")?;

    Ok(reload_handle)
}

/// 扫描日志目录并清理超过 log_keep_days 天的历史日志
fn cleanup_old_logs(config: &Config) {
    let keep_duration = Duration::from_secs(config.log_keep_days as u64 * 24 * 3600);
    let now = SystemTime::now();

    let Ok(entries) = fs::read_dir(&config.log_dir) else { return };

    for entry in entries.flatten() {
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_file() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            // 只清理我们生成的 app.log.* 文件
            if file_name.starts_with("app.log.") {
                if let Ok(modified) = meta.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > keep_duration {
                            let _ = fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }
}
