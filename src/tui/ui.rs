use crate::state::{DownloadStatus, SearchState};
use crate::tui::app::{Focus, TuiApp};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame,
};

const ACCENT: Color = Color::Rgb(139, 92, 246);
const ACCENT2: Color = Color::Rgb(236, 72, 153);
const BG: Color = Color::Rgb(10, 10, 15);
const BG2: Color = Color::Rgb(22, 22, 31);
const MUTED: Color = Color::Rgb(71, 85, 105);
const SUCCESS: Color = Color::Rgb(16, 185, 129);
const ERROR: Color = Color::Rgb(239, 68, 68);
const WARNING: Color = Color::Rgb(245, 158, 11);

pub fn render(f: &mut Frame, app: &TuiApp) {
    let size = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BG)), size);

    // Main layout: header / body / status-bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(3), // search bar
            Constraint::Min(0),    // search results + downloads
            Constraint::Length(1), // status bar
        ])
        .split(size);

    render_header(f, chunks[0]);
    render_search_bar(f, app, chunks[1]);
    render_body(f, app, chunks[2]);
    render_status_bar(f, app, chunks[3]);

    if app.show_settings {
        render_settings_popup(f, app);
    }
}

fn render_header(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled("🎵 ", Style::default()),
        Span::styled(
            "音乐下载器",
            Style::default()
                .fg(Color::Rgb(196, 181, 253))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  YouTube Music Downloader", Style::default().fg(MUTED)),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(30, 30, 46)))
            .border_type(BorderType::Plain)
            .style(Style::default().bg(BG2)),
    );
    f.render_widget(title, area);
}

fn render_search_bar(f: &mut Frame, app: &TuiApp, area: Rect) {
    let focused = app.focus == Focus::Search;
    let border_color = if focused {
        ACCENT
    } else {
        Color::Rgb(30, 30, 46)
    };

    let display = if app.is_searching {
        format!("🔍 搜索中: {}…", app.query_input)
    } else {
        format!("🔍 {}", app.query_input)
    };

    let cursor_char = if focused && !app.is_searching {
        "▌"
    } else {
        ""
    };

    let text = Paragraph::new(format!("{}{}", display, cursor_char))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    " 搜索 [Enter 搜索  Tab 切面板] ",
                    Style::default()
                        .fg(if focused { ACCENT } else { MUTED })
                        .add_modifier(Modifier::BOLD),
                ))
                .style(Style::default().bg(BG2)),
        );
    f.render_widget(text, area);
}

fn render_body(f: &mut Frame, app: &TuiApp, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    render_results(f, app, chunks[0]);
    render_downloads(f, app, chunks[1]);
}

fn render_results(f: &mut Frame, app: &TuiApp, area: Rect) {
    let focused = app.focus == Focus::Results;
    let border_color = if focused {
        ACCENT
    } else {
        Color::Rgb(30, 30, 46)
    };

    let state_guard = app.shared.try_lock().ok();

    let (results, search_state, _query) = if let Some(s) = &state_guard {
        (
            s.search_results.clone(),
            s.search_state.clone(),
            s.last_query.clone(),
        )
    } else {
        (vec![], SearchState::Idle, String::new())
    };

    let title_suffix = match &search_state {
        SearchState::Searching => " ⏳ ".to_string(),
        SearchState::Done if !results.is_empty() => format!(" ({}) ", results.len()),
        SearchState::Error(_) => " ✗ ".to_string(),
        _ => " ".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" 搜索结果{}", title_suffix),
            Style::default()
                .fg(if focused { ACCENT } else { MUTED })
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left)
        .style(Style::default().bg(BG));

    if results.is_empty() {
        let msg = match &search_state {
            SearchState::Error(e) => format!("搜索失败: {}", e),
            SearchState::Searching => "搜索中…".to_string(),
            _ => "请在上方输入关键词后按 Enter 搜索".to_string(),
        };
        let p = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .style(Style::default().fg(MUTED))
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let selected_marker = if i == app.result_selected {
                "▶ "
            } else {
                "  "
            };
            let duration = match v.duration {
                Some(s) => {
                    let m = s / 60;
                    let sec = s % 60;
                    format!("{:02}:{:02}", m, sec)
                }
                None => "--:--".into(),
            };
            let channel = v
                .channel
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(20)
                .collect::<String>();

            let size_str = v
                .filesize_approx
                .map(|s| format!("{:.1}MB", s as f64 / 1048576.0))
                .unwrap_or_else(|| "未知大小".into());
            let date_str = v.upload_date.as_deref().unwrap_or("未知日期");
            let media_type_str = match v.media_type.as_deref() {
                Some("audio") => "🎵音频",
                Some("video") => "🎬视频",
                Some("video+audio") => "🎬+🎵音视频",
                _ => "未知类型",
            };

            let line1 = Line::from(vec![
                Span::styled(
                    selected_marker,
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    truncate(&v.title, 60),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(if i == app.result_selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]);
            let line2 = Line::from(vec![
                Span::raw("   "),
                Span::styled(format!("⏱ {}  ", duration), Style::default().fg(MUTED)),
                Span::styled(format!("👤 {}  ", channel), Style::default().fg(MUTED)),
                Span::styled(
                    format!("💾 {}  ", size_str),
                    Style::default().fg(Color::Rgb(14, 165, 233)),
                ), // Sky blue
                Span::styled(format!("📅 {}  ", date_str), Style::default().fg(MUTED)),
                Span::styled(
                    format!("[{}]", media_type_str),
                    Style::default().fg(ACCENT2),
                ),
            ]);
            ListItem::new(vec![line1, line2])
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.result_selected));

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(Color::Rgb(30, 20, 50)))
        .highlight_symbol("");

    f.render_stateful_widget(list, area, &mut list_state);
}

fn render_downloads(f: &mut Frame, app: &TuiApp, area: Rect) {
    let focused = app.focus == Focus::Downloads;
    let border_color = if focused {
        ACCENT2
    } else {
        Color::Rgb(30, 30, 46)
    };

    let state_guard = app.shared.try_lock().ok();
    let tasks: Vec<_> = if let Some(s) = &state_guard {
        s.downloads_ordered().into_iter().cloned().collect()
    } else {
        vec![]
    };

    // We need to split the area into rows for each download (with progress gauge)
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" 下载队列 ({}) ", tasks.len()),
            Style::default()
                .fg(if focused { ACCENT2 } else { MUTED })
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));

    if tasks.is_empty() {
        let p = Paragraph::new("暂无下载任务\n按 d 键下载选中的搜索结果")
            .alignment(Alignment::Center)
            .style(Style::default().fg(MUTED))
            .block(block)
            .wrap(Wrap { trim: true });
        f.render_widget(p, area);
        return;
    }

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Draw tasks: each takes 3 rows
    let visible_tasks = ((inner.height as usize) / 3).max(1);
    let start = if tasks.len() > visible_tasks {
        tasks.len().saturating_sub(visible_tasks)
    } else {
        0
    };

    for (i, task) in tasks[start..].iter().enumerate() {
        let y = inner.y + (i * 3) as u16;
        if y + 2 >= inner.y + inner.height {
            break;
        }
        let row_area = Rect {
            x: inner.x,
            y,
            width: inner.width,
            height: 3,
        };
        render_download_item(f, task, row_area);
    }
}

fn render_download_item(f: &mut Frame, task: &crate::state::DownloadTask, area: Rect) {
    let title_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    let gauge_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: 1,
    };
    let sep_area = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: 1,
    };

    let (status_label, status_color, progress) = match &task.status {
        DownloadStatus::Queued => ("队列中", MUTED, 0.0_f64),
        DownloadStatus::Downloading { progress } => {
            ("下载中", ACCENT, (*progress as f64).min(100.0))
        }
        DownloadStatus::Converting => ("转换中", WARNING, 99.0),
        DownloadStatus::Done { .. } => ("✓ 完成", SUCCESS, 100.0),
        DownloadStatus::Failed { .. } => ("✗ 失败", ERROR, 0.0),
    };

    let title_line = Line::from(vec![
        Span::styled(
            truncate(&task.video.title, (area.width as usize).saturating_sub(14)),
            Style::default().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", status_label),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(title_line), title_area);

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(status_color).bg(Color::Rgb(20, 20, 30)))
        .ratio((progress / 100.0).clamp(0.0, 1.0))
        .label(format!("{:.0}%", progress));
    f.render_widget(gauge, gauge_area);

    // separator line
    let sep = Paragraph::new("─".repeat(area.width as usize))
        .style(Style::default().fg(Color::Rgb(30, 30, 46)));
    f.render_widget(sep, sep_area);
}

fn render_status_bar(f: &mut Frame, app: &TuiApp, area: Rect) {
    let msg = app.status_message.as_deref().unwrap_or("");
    let help = "Tab:切换  ↑↓:移动  d/Enter:下载  q:退出(空搜索)";
    let left = Paragraph::new(msg).style(
        Style::default()
            .fg(Color::Rgb(148, 163, 184))
            .bg(Color::Rgb(15, 15, 22)),
    );
    let right = Paragraph::new(help)
        .alignment(Alignment::Right)
        .style(Style::default().fg(MUTED).bg(Color::Rgb(15, 15, 22)));

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    f.render_widget(left, chunks[0]);
    f.render_widget(right, chunks[1]);
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        chars[..max_chars.saturating_sub(1)]
            .iter()
            .collect::<String>()
            + "…"
    }
}

fn render_settings_popup(f: &mut Frame, app: &TuiApp) {
    let size = f.area();

    // Popup in the bottom-right corner
    let popup_width = 40;
    let popup_height = 13;
    let x = size.width.saturating_sub(popup_width).saturating_sub(2);
    let y = size.height.saturating_sub(popup_height).saturating_sub(2);
    let area = Rect::new(
        x,
        y,
        popup_width.min(size.width),
        popup_height.min(size.height),
    );

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" ⚙ 设 置 ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT));

    f.render_widget(block, area);

    let current_level = app
        .shared
        .try_lock()
        .map(|s| s.current_log_level.clone())
        .unwrap_or_else(|_| "info".into());

    let lines = vec![
        Line::from("日志级别 (按数字键切换):"),
        Line::from(vec![
            Span::styled(
                " [1] trace  ",
                if current_level == "trace" {
                    Style::default().fg(SUCCESS)
                } else {
                    Style::default().fg(MUTED)
                },
            ),
            Span::styled(
                " [2] debug  ",
                if current_level == "debug" {
                    Style::default().fg(SUCCESS)
                } else {
                    Style::default().fg(MUTED)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " [3] info   ",
                if current_level == "info" {
                    Style::default().fg(SUCCESS)
                } else {
                    Style::default().fg(MUTED)
                },
            ),
            Span::styled(
                " [4] warn   ",
                if current_level == "warn" {
                    Style::default().fg(SUCCESS)
                } else {
                    Style::default().fg(MUTED)
                },
            ),
        ]),
        Line::from(vec![Span::styled(
            " [5] error  ",
            if current_level == "error" {
                Style::default().fg(SUCCESS)
            } else {
                Style::default().fg(MUTED)
            },
        )]),
        Line::from(""),
        Line::from(vec![
            Span::raw(" 日志目录: "),
            Span::styled(
                app.config.log_dir.to_string_lossy().to_string(),
                Style::default().fg(ACCENT2),
            ),
        ]),
        Line::from(vec![
            Span::raw(" 保留天数: "),
            Span::styled(
                format!("{} 天", app.config.log_keep_days),
                Style::default().fg(ACCENT2),
            ),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "          [s/Esc] 关闭          ",
            Style::default().fg(MUTED),
        )]),
    ];

    let content_area = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    f.render_widget(Paragraph::new(lines), content_area);
}
