use crate::config::Config;
use crate::state::{DownloadStatus, SharedState, VideoInfo};
use anyhow::{Context, Result};
use reqwest::Url;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// 启动一个下载任务（异步），实时更新共享状态中的进度
pub async fn download_video(task_id: String, video: VideoInfo, config: Config, state: SharedState) {
    let limiter = {
        let app = state.lock().await;
        app.download_limiter.clone()
    };

    let _permit = match limiter.acquire_owned().await {
        Ok(permit) => permit,
        Err(e) => {
            let mut app = state.lock().await;
            app.update_download_status(
                &task_id,
                DownloadStatus::Failed {
                    error: format!("下载调度器不可用: {}", e),
                },
            );
            return;
        }
    };

    let result = download_with_retry(&task_id, &video, &config, &state, 3).await;

    let mut app = state.lock().await;
    match result {
        Ok(file_path) => {
            app.update_download_status(&task_id, DownloadStatus::Done { file_path });
        }
        Err(e) => {
            app.update_download_status(
                &task_id,
                DownloadStatus::Failed {
                    error: e.to_string(),
                },
            );
        }
    }
}

async fn download_with_retry(
    task_id: &str,
    video: &VideoInfo,
    config: &Config,
    state: &SharedState,
    max_retries: u32,
) -> Result<String> {
    let mut last_err = anyhow::anyhow!("Unknown error");

    for attempt in 0..max_retries {
        if attempt > 0 {
            let wait_secs = 2u64.pow(attempt);
            tracing::warn!(
                "Retrying download for '{}' (attempt {}/{}), waiting {}s",
                video.title,
                attempt + 1,
                max_retries,
                wait_secs
            );
            tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
        }

        match do_download(task_id, video, config, state).await {
            Ok(path) => return Ok(path),
            Err(e) => {
                tracing::error!("Download attempt {} failed: {}", attempt + 1, e);
                last_err = e;
            }
        }
    }

    Err(last_err)
}

/// 清理文件名中的非法字符
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn download_source_url(video: &VideoInfo) -> &str {
    video.webpage_url.as_deref().unwrap_or(video.url.as_str())
}

pub fn is_allowed_media_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };

    let Some(host) = parsed.host_str() else {
        return false;
    };

    matches!(
        host,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be"
    )
}

async fn do_download(
    task_id: &str,
    video: &VideoInfo,
    config: &Config,
    state: &SharedState,
) -> Result<String> {
    config.ensure_download_dir()?;
    let download_url = download_source_url(video);

    // 使用搜索结果中的 title 作为文件名（已经是 "artist - track" 格式）
    let safe_title = sanitize_filename(&video.title);
    let output_template = config
        .download_dir
        .join(format!("{}.%(ext)s", safe_title))
        .to_string_lossy()
        .to_string();

    tracing::info!("Downloading: {} -> {}", video.title, output_template);

    let mut child = Command::new(&config.ytdlp_path)
        .env("PYTHONIOENCODING", "utf-8")
        .args([
            "--encoding",
            "utf-8",
            "--no-check-certificate",
            "-x",
            "--audio-format",
            &config.audio_format,
            "--audio-quality",
            &config.audio_quality,
            "--embed-metadata",
            "--extractor-args",
            "youtube:player_client=web,default",
            "--newline", // 每个进度更新单独一行
            "--progress",
            "-o",
            &output_template,
            download_url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp process")?;

    let stdout = child.stdout.take().expect("stdout should be piped");
    let stderr = child.stderr.take().expect("stderr should be piped");

    // 异步处理 stderr 输出，防止缓冲区占满导致进程挂起
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            tracing::warn!("yt-dlp stderr: {}", line);
        }
    });

    let mut reader = BufReader::new(stdout).lines();

    let mut downloaded_file: Option<String> = None;
    let target_ext = normalized_extension(&config.audio_format);

    // 实时解析 yt-dlp 进度输出
    while let Ok(Some(line)) = reader.next_line().await {
        tracing::debug!("yt-dlp: {}", line);
        let lower_line = line.to_ascii_lowercase();

        // 解析进度：[download]  57.3% of ...
        if let Some(progress) = parse_progress(&line) {
            let mut app = state.lock().await;
            app.update_download_status(task_id, DownloadStatus::Downloading { progress });
        }

        // 检测转换阶段或后处理阶段
        if line.contains("[ExtractAudio]")
            || line.contains("[Metadata]")
            || line.contains("[Fixup")
            || (line.contains("Destination:") && lower_line.contains(&format!(".{}", target_ext)))
        {
            let mut app = state.lock().await;
            app.update_download_status(task_id, DownloadStatus::Converting);
        }

        // 提取最终文件路径
        if let Some(path) = extract_output_path(&line) {
            downloaded_file = Some(path);
        }
    }

    let status = child.wait().await.context("Failed to wait for yt-dlp")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "yt-dlp exited with status: {}",
            status.code().unwrap_or(-1)
        ));
    }

    // 如果没有从输出中提取到文件路径，尝试基于当前任务标题匹配目标文件
    let file_path = downloaded_file.unwrap_or_else(|| {
        find_output_file(&config.download_dir, &safe_title, &target_ext)
            .unwrap_or_else(|| config.download_dir.to_string_lossy().to_string())
    });

    Ok(file_path)
}

fn parse_progress(line: &str) -> Option<f32> {
    // 格式: [download]  57.3% of ...
    if !line.contains("[download]") {
        return None;
    }
    let after = line.split("[download]").nth(1)?;
    let trimmed = after.trim();
    if let Some(pct_str) = trimmed.split('%').next() {
        if let Ok(v) = pct_str.trim().parse::<f32>() {
            if v >= 0.0 && v <= 100.0 {
                return Some(v);
            }
        }
    }
    None
}

fn extract_output_path(line: &str) -> Option<String> {
    for marker in ["Destination:", "Merging formats into"] {
        if let Some(after) = line.split(marker).nth(1) {
            let path = after.trim().trim_matches('"').to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    None
}

fn normalized_extension(ext: &str) -> String {
    ext.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn find_output_file(dir: &std::path::Path, stem: &str, ext: &str) -> Option<String> {
    let exact = dir.join(format!("{}.{}", stem, ext));
    if exact.exists() {
        return Some(exact.to_string_lossy().to_string());
    }

    let entries = std::fs::read_dir(dir).ok()?;
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            let matches_extension = path
                .extension()
                .map(|value| value.to_string_lossy().eq_ignore_ascii_case(ext))
                .unwrap_or(false);
            let matches_stem = path
                .file_stem()
                .map(|value| value.to_string_lossy().starts_with(stem))
                .unwrap_or(false);
            matches_extension && matches_stem
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();

    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
        .into_iter()
        .next()
        .map(|(_, p)| p.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    #[test]
    fn parse_progress_handles_percentages() {
        assert_eq!(parse_progress("[download]  57.3% of 3.00MiB"), Some(57.3));
        assert_eq!(parse_progress("[download]  120% of 3.00MiB"), None);
        assert_eq!(parse_progress("not a progress line"), None);
    }

    #[test]
    fn allowed_media_url_only_accepts_youtube_hosts() {
        assert!(is_allowed_media_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_allowed_media_url(
            "https://music.youtube.com/watch?v=abc"
        ));
        assert!(is_allowed_media_url("https://youtu.be/abc"));
        assert!(!is_allowed_media_url("https://example.com/watch?v=abc"));
        assert!(!is_allowed_media_url("not-a-url"));
    }

    #[test]
    fn find_output_file_prefers_matching_task_file() {
        let base_dir = std::env::temp_dir().join(format!("ytb-mdl-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&base_dir).expect("create temp dir");

        let expected = base_dir.join("Artist - Song.m4a");
        let unrelated = base_dir.join("Other Song.m4a");
        fs::write(&expected, "ok").expect("write expected file");
        fs::write(&unrelated, "ok").expect("write unrelated file");

        let matched = find_output_file(&base_dir, "Artist - Song", "m4a");
        assert_eq!(
            matched.as_deref(),
            Some(expected.to_string_lossy().as_ref())
        );

        let _ = fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn global_download_limiter_blocks_extra_tasks() {
        let state = Arc::new(Mutex::new(AppState::new(1)));
        let limiter = {
            let app = state.lock().await;
            app.download_limiter.clone()
        };

        let _first = limiter.clone().acquire_owned().await.expect("first permit");
        let pending = limiter.acquire_owned();
        assert!(tokio::time::timeout(Duration::from_millis(50), pending)
            .await
            .is_err());
    }
}
