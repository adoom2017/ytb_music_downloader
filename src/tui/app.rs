use crate::config::Config;
use crate::download;
use crate::search;
use crate::state::{DownloadTask, SearchState, SharedState};
use crate::tui::ui;
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

/// TUI 应用本地视图状态（与 SharedState 分开，避免锁冲突）
pub struct TuiApp {
    pub config: Config,
    pub shared: SharedState,
    pub query_input: String,
    pub cursor_pos: usize,
    pub focus: Focus,
    pub result_selected: usize,
    pub download_selected: usize,
    pub status_message: Option<String>,
    pub is_searching: bool,
    pub download_tx: mpsc::Sender<DownloadTask>,
    pub show_settings: bool,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Focus {
    Search,
    Results,
    Downloads,
}

pub async fn run_tui(config: Config, state: SharedState) -> Result<()> {
    // 启用 raw mode
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 下载任务 channel（让 TUI 触发后台下载）
    let (tx, mut rx) = mpsc::channel::<DownloadTask>(32);

    // 后台下载 worker
    {
        let state_c = state.clone();
        let config_c = config.clone();
        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                let id = task.id.clone();
                let video = task.video.clone();
                let s = state_c.clone();
                let c = config_c.clone();
                tokio::spawn(async move {
                    download::download_video(id, video, c, s).await;
                });
            }
        });
    }

    let mut app = TuiApp {
        config,
        shared: state,
        query_input: String::new(),
        cursor_pos: 0,
        focus: Focus::Search,
        result_selected: 0,
        download_selected: 0,
        status_message: Some("按 Enter 搜索，Tab 切换，d 下载，s 设置，q 退出".into()),
        is_searching: false,
        download_tx: tx,
        show_settings: false,
    };

    loop {
        // 渲染
        terminal.draw(|f| ui::render(f, &app))?;

        // 事件处理（超时 200ms 防止 CPU 空转）
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    // Settings panel interactions
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') if app.show_settings => {
                        app.show_settings = false;
                    }
                    KeyCode::Char(c) if app.show_settings => {
                        if let Some(level) = match c {
                            '1' => Some("trace"),
                            '2' => Some("debug"),
                            '3' => Some("info"),
                            '4' => Some("warn"),
                            '5' => Some("error"),
                            _ => None,
                        } {
                            let mut s = app.shared.lock().await;
                            if let Some(handle) = &s.log_handle {
                                use tracing_subscriber::filter::EnvFilter;
                                let new_filter = EnvFilter::new(format!("ytb_music_downloader={}", level));
                                if handle.reload(new_filter).is_ok() {
                                    s.current_log_level = level.to_string();
                                    app.status_message = Some(format!("已切换日志级别: {}", level));
                                }
                            }
                        }
                    }
                    // Global quit & panel switch
                    KeyCode::Char('q') if app.focus != Focus::Search => {
                        break;
                    }
                    KeyCode::Char('s') if app.focus != Focus::Search => {
                        app.show_settings = true;
                    }
                    KeyCode::Tab => {
                        app.focus = match app.focus {
                            Focus::Search => Focus::Results,
                            Focus::Results => Focus::Downloads,
                            Focus::Downloads => Focus::Search,
                        };
                    }
                    KeyCode::Esc => {
                        app.focus = Focus::Search;
                    }
                    // Search input
                    KeyCode::Char(c) if app.focus == Focus::Search => {
                        app.query_input.insert(app.cursor_pos, c);
                        app.cursor_pos += c.len_utf8();
                    }
                    KeyCode::Backspace if app.focus == Focus::Search => {
                        if app.cursor_pos > 0 {
                            if let Some(c) = app.query_input[..app.cursor_pos].chars().last() {
                                app.cursor_pos -= c.len_utf8();
                                app.query_input.remove(app.cursor_pos);
                            }
                        }
                    }
                    KeyCode::Enter if app.focus == Focus::Search => {
                        if !app.query_input.trim().is_empty() && !app.is_searching {
                            let query = app.query_input.trim().to_string();
                            let max = app.config.max_search_results;
                            let config = app.config.clone();
                            let shared = app.shared.clone();

                            app.is_searching = true;
                            app.status_message = Some(format!("正在搜索 \"{}\"…", query));
                            app.result_selected = 0;

                            // 异步搜索，搜索完成后更新 shared state
                            tokio::spawn(async move {
                                match search::search_youtube(&query, max, &config).await {
                                    Ok(results) => {
                                        let mut s = shared.lock().await;
                                        s.search_results = results;
                                        s.search_state = SearchState::Done;
                                        s.last_query = query;
                                    }
                                    Err(e) => {
                                        let mut s = shared.lock().await;
                                        s.search_state = SearchState::Error(e.to_string());
                                    }
                                }
                            });
                            app.focus = Focus::Results;
                        }
                    }
                    // Navigate results
                    KeyCode::Up if app.focus == Focus::Results => {
                        if app.result_selected > 0 {
                            app.result_selected -= 1;
                        }
                    }
                    KeyCode::Down if app.focus == Focus::Results => {
                        let count = app.shared.try_lock().map(|s| s.search_results.len()).unwrap_or(0);
                        if app.result_selected + 1 < count {
                            app.result_selected += 1;
                        }
                    }
                    // Download selected
                    KeyCode::Char('d') | KeyCode::Enter if app.focus == Focus::Results => {
                        let video = {
                            let s = app.shared.try_lock().ok();
                            s.and_then(|s| s.search_results.get(app.result_selected).cloned())
                        };
                        if let Some(video) = video {
                            let task = DownloadTask::new(video.clone());
                            {
                                let mut s = app.shared.lock().await;
                                s.add_download(task.clone());
                            }
                            app.status_message = Some(format!("开始下载: {}", video.title));
                            let _ = app.download_tx.send(task).await;
                        }
                    }
                    // Navigate downloads
                    KeyCode::Up if app.focus == Focus::Downloads => {
                        if app.download_selected > 0 {
                            app.download_selected -= 1;
                        }
                    }
                    KeyCode::Down if app.focus == Focus::Downloads => {
                        let count = app.shared.try_lock().map(|s| s.downloads.len()).unwrap_or(0);
                        if app.download_selected + 1 < count {
                            app.download_selected += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        // 同步搜索完成状态
        if app.is_searching {
            let state = app.shared.try_lock().ok().map(|s| s.search_state.clone());
            match state {
                Some(SearchState::Done) => {
                    app.is_searching = false;
                    let count = app.shared.try_lock().map(|s| s.search_results.len()).unwrap_or(0);
                    let q = app.shared.try_lock().map(|s| s.last_query.clone()).unwrap_or_default();
                    app.status_message = Some(format!("搜索 \"{}\" 完成，共 {} 条结果", q, count));
                    if let Ok(mut s) = app.shared.try_lock() { s.search_state = SearchState::Idle; }
                }
                Some(SearchState::Error(e)) => {
                    app.is_searching = false;
                    app.status_message = Some(format!("搜索失败: {}", e));
                    if let Ok(mut s) = app.shared.try_lock() { s.search_state = SearchState::Idle; }
                }
                _ => {}
            }
        }
    }

    // 恢复终端
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
