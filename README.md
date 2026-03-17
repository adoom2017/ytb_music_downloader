# 🎵 音乐下载器 - YouTube Music Downloader

一款使用 Rust 编写的 YouTube 音乐批量下载工具，支持 **TUI 终端界面** 和 **Web 远程访问** 两种模式。

## 前置要求

```bash
# 1. 安装 yt-dlp
pip install yt-dlp
# 或 Windows: winget install yt-dlp

# 2. 安装 ffmpeg（用于音频转换）
# Windows: winget install ffmpeg
# macOS:   brew install ffmpeg
# Linux:   sudo apt install ffmpeg

# 3. 验证安装
yt-dlp --version
ffmpeg -version
```

## 安装 & 构建

```bash
git clone <repo>
cd ytb_music_downloader
cargo build --release
```

## 使用方式

### TUI 模式（本地终端界面）

```bash
cargo run          # 或
cargo run -- tui
```

**键盘快捷键**：

| 键位 | 功能 |
|------|------|
| `Tab` | 切换面板（搜索 → 结果 → 下载） |
| `Enter` | 执行搜索（在搜索面板） |
| `↑ / ↓` | 在列表中移动 |
| `d` / `Enter` | 下载当前选中项（在结果面板） |
| `Esc` | 返回搜索面板 |
| `q` | 退出程序 |

### Web 模式（远程浏览器访问）

```bash
cargo run -- web            # 默认端口 3000
cargo run -- web --port 8080  # 自定义端口
```

启动后在浏览器中访问 `http://localhost:3000`（或服务器 IP:端口）。

## 配置（环境变量）

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DOWNLOAD_DIR` | `./downloads` | 下载保存目录 |
| `PORT` | `3000` | Web 服务端口 |
| `BIND_HOST` | `127.0.0.1` | Web 服务监听地址 |
| `ALLOWED_ORIGINS` | 空 | 允许跨域访问的来源，多个用逗号分隔 |
| `YTDLP_PATH` | `yt-dlp` | yt-dlp 可执行文件路径 |

## Web API

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/api/search` | 搜索 YouTube，body: `{"query": "...", "max_results": 10}` |
| `POST` | `/api/download` | 下载单曲，body: `{"video": {...}}` |
| `POST` | `/api/download/batch` | 批量下载 |
| `GET` | `/api/downloads` | 获取所有下载任务状态 |
| `GET` | `/api/downloads/{id}` | 获取单个任务状态 |

## 项目结构

```
src/
├── main.rs          # 入口，CLI 解析（tui/web 子命令）
├── config.rs        # 配置管理
├── state.rs         # 共享状态（TUI 和 Web 共用）
├── search.rs        # yt-dlp ytsearch 搜索
├── download.rs      # yt-dlp 下载 + 进度解析 + 重试
├── tui/
│   ├── mod.rs
│   ├── app.rs       # TUI 事件循环
│   └── ui.rs        # ratatui 渲染
└── web/
    ├── mod.rs
    ├── server.rs    # axum 服务器
    └── routes.rs    # REST API Handlers
assets/
└── index.html       # Web 前端单页应用
```

## 开源工具

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) - 视频/音频下载
- [ratatui](https://ratatui.rs) - 终端 UI 框架
- [axum](https://github.com/tokio-rs/axum) - Web 框架
- [tokio](https://tokio.rs) - 异步运行时

> 本工具仅供个人学习使用，请遵守 YouTube 服务条款和版权法。
