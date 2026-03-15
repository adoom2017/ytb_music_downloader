mod config;
mod download;
mod logger;
mod search;
mod state;
mod tui;
mod web;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use state::new_shared_state;

#[derive(Parser)]
#[command(
    name = "ytb-music-downloader",
    about = "🎵 YouTube 音乐下载器 - 支持 TUI 和 Web 两种模式",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 启动终端 TUI 界面（默认）
    Tui,
    /// 启动 Web 服务器，通过浏览器访问
    Web {
        /// 监听端口 (默认 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load();
    config.ensure_download_dir()?;

    // 初始化日志并获取动态修改级别的句柄
    let is_tui = !matches!(cli.command, Some(Commands::Web { .. }));
    let log_handle = logger::init_logger(&config, is_tui)?;

    let shared = new_shared_state(log_handle, config.log_level.clone());

    match cli.command.unwrap_or(Commands::Tui) {
        Commands::Tui => {
            tui::run_tui(config, shared).await?;
        }
        Commands::Web { port } => {
            config.web_port = port;
            web::start_web_server(config, shared).await?;
        }
    }

    Ok(())
}
