use crate::config::Config;
use crate::state::{DownloadStatus, SharedState, VideoInfo};
use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// 启动一个下载任务（异步），实时更新共享状态中的进度
pub async fn download_video(
    task_id: String,
    video: VideoInfo,
    config: Config,
    state: SharedState,
) {
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

async fn do_download(
    task_id: &str,
    video: &VideoInfo,
    config: &Config,
    state: &SharedState,
) -> Result<String> {
    config.ensure_download_dir()?;

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
            "--encoding", "utf-8",
            "--no-check-certificate",
            "-x",
            "--audio-format",
            &config.audio_format,
            "--audio-quality",
            &config.audio_quality,
            "--embed-metadata",
            "--extractor-args",
            "youtube:player_client=web,default",
            "--newline",       // 每个进度更新单独一行
            "--progress",
            "-o",
            &output_template,
            &video.url,
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

    // 实时解析 yt-dlp 进度输出
    while let Ok(Some(line)) = reader.next_line().await {
        tracing::debug!("yt-dlp: {}", line);

        // 解析进度：[download]  57.3% of ...
        if let Some(progress) = parse_progress(&line) {
            let mut app = state.lock().await;
            app.update_download_status(
                task_id,
                DownloadStatus::Downloading { progress },
            );
        }

        // 检测转换阶段或后处理阶段
        if line.contains("[ExtractAudio]") 
            || line.contains("[Metadata]") 
            || line.contains("[Fixup")
            || (line.contains("Destination:") && line.contains(".mp3")) 
        {
            let mut app = state.lock().await;
            app.update_download_status(task_id, DownloadStatus::Converting);
        }

        // 提取最终文件路径
        if line.contains("Destination:") {
            if let Some(path) = extract_destination(&line) {
                downloaded_file = Some(path);
            }
        }
        // 已合并文件
        if line.contains("Merging formats into") || line.contains("[ExtractAudio] Destination:") {
            if let Some(path) = extract_destination(&line) {
                downloaded_file = Some(path);
            }
        }
    }

    let status = child.wait().await.context("Failed to wait for yt-dlp")?;

    if !status.success() {
        return Err(anyhow::anyhow!(
            "yt-dlp exited with status: {}",
            status.code().unwrap_or(-1)
        ));
    }

    // 如果没有从输出中提取到文件路径，尝试在下载目录中查找最新 mp3 文件
    let file_path = downloaded_file.unwrap_or_else(|| {
        find_latest_file(&config.download_dir, "mp3")
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

fn extract_destination(line: &str) -> Option<String> {
    // 格式: [ExtractAudio] Destination: /path/to/file.mp3
    let after = line.split("Destination:").nth(1)?;
    let path = after.trim().to_string();
    if !path.is_empty() {
        Some(path)
    } else {
        None
    }
}

fn find_latest_file(dir: &std::path::Path, ext: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == ext)
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();

    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().next().map(|(_, p)| p.to_string_lossy().to_string())
}
