use crate::config::Config;
use crate::state::VideoInfo;
use anyhow::{Context, Result};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;

/// 搜索策略：优先 YouTube Music（music.youtube.com/search），无结果时回退到普通 YouTube 搜索
pub async fn search_youtube(query: &str, max_results: usize, config: &Config) -> Result<Vec<VideoInfo>> {
    // ── 第一步：YouTube Music 搜索 ──────────────────────────────────
    // 使用 music.youtube.com/search?q=... URL，yt-dlp 的 YouTubeMusic 提取器原生支持
    tracing::info!("Searching YouTube Music for: {}", query);
    match run_music_search(query, max_results, config).await {
        Ok(results) if !results.is_empty() => {
            tracing::info!("YouTube Music returned {} results", results.len());
            return Ok(results);
        }
        Ok(_) => {
            tracing::warn!("YouTube Music returned 0 results, falling back to YouTube...");
        }
        Err(e) => {
            tracing::warn!("YouTube Music search failed ({}), falling back to YouTube...", e);
        }
    }

    // ── 第二步：回退到普通 YouTube 视频搜索 ─────────────────────────
    tracing::info!("Falling back to YouTube: ytsearch{}:{}", max_results, query);
    let results = run_yt_search(query, max_results, config).await?;
    tracing::info!("YouTube returned {} results", results.len());
    Ok(results)
}

/// YouTube Music 搜索：使用 music.youtube.com/search?q=... URL
/// yt-dlp 通过其 YouTubeMusic 提取器原生解析此页面
async fn run_music_search(query: &str, max_results: usize, config: &Config) -> Result<Vec<VideoInfo>> {
    let encoded = urlencoding::encode(query);
    // 加 &sp=EgWKAQIIAWoKEAoQAxAEEAkQBQ%3D%3D 过滤出 Songs 类型 (可选)
    let search_url = format!("https://music.youtube.com/search?q={}", encoded);

    tracing::debug!("YouTube Music URL: {}", search_url);

    let output = Command::new(&config.ytdlp_path)
        .args([
            &search_url,
            "--dump-json",
            "--flat-playlist",
            "--no-download",
            "--playlist-end", &max_results.to_string(),
            "--quiet",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to execute yt-dlp. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("yt-dlp YouTube Music search failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(json) = serde_json::from_str::<Value>(line) {
            if let Some(mut info) = parse_video_info(&json) {
                // 将 webpage_url 设为 music.youtube.com 格式
                info.webpage_url = Some(format!(
                    "https://music.youtube.com/watch?v={}", info.id
                ));
                info.media_type = Some("audio".to_string());
                results.push(info);
                if results.len() >= max_results { break; }
            }
        }
    }

    Ok(results)
}

/// 普通 YouTube 搜索：使用 ytsearch{N}: 前缀
async fn run_yt_search(query: &str, max_results: usize, config: &Config) -> Result<Vec<VideoInfo>> {
    let search_query = format!("ytsearch{}:{}", max_results, query);

    let output = Command::new(&config.ytdlp_path)
        .args([
            &search_query,
            "--dump-json",
            "--flat-playlist",
            "--no-download",
            "--no-playlist",
            "--quiet",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to execute yt-dlp. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("yt-dlp search failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(json) => {
                if let Some(info) = parse_video_info(&json) {
                    results.push(info);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to parse yt-dlp JSON line: {}", e);
            }
        }
    }

    Ok(results)
}


fn parse_video_info(json: &Value) -> Option<VideoInfo> {
    let id = json["id"].as_str().unwrap_or("").to_string();
    let title = json["title"].as_str().unwrap_or("Unknown Title").to_string();

    if id.is_empty() || title == "[Deleted video]" || title == "[Private video]" {
        return None;
    }

    let url = format!("https://www.youtube.com/watch?v={}", id);

    let duration = json["duration"].as_u64();

    let channel = json["channel"]
        .as_str()
        .or_else(|| json["uploader"].as_str())
        .map(|s| s.to_string());

    let thumbnail = json["thumbnail"].as_str().map(|s| s.to_string());
    let view_count = json["view_count"].as_u64();

    // 文件大小（字节）：优先取 filesize，再取 filesize_approx
    let filesize_approx = json["filesize"].as_u64()
        .or_else(|| json["filesize_approx"].as_u64());

    // 判断媒体类型：根据 vcodec / acodec
    let vcodec = json["vcodec"].as_str().unwrap_or("");
    let acodec = json["acodec"].as_str().unwrap_or("");
    let media_type = if vcodec != "none" && !vcodec.is_empty() && acodec != "none" && !acodec.is_empty() {
        Some("video+audio".to_string())
    } else if vcodec != "none" && !vcodec.is_empty() {
        Some("video".to_string())
    } else if acodec != "none" && !acodec.is_empty() {
        Some("audio".to_string())
    } else {
        // flat-playlist 模式下 codec 字段通常为空，默认为 video
        Some("video".to_string())
    };

    // 原始网页链接
    let webpage_url = json["webpage_url"]
        .as_str()
        .or_else(|| json["original_url"].as_str())
        .map(|s| s.to_string())
        .or_else(|| Some(url.clone()));

    // 上传日期
    let upload_date = json["upload_date"].as_str().map(|s| s.to_string());

    Some(VideoInfo {
        id,
        title,
        url,
        duration,
        channel,
        thumbnail,
        view_count,
        filesize_approx,
        media_type,
        webpage_url,
        upload_date,
    })
}
