# 阶段 1：构建
FROM rust:slim AS builder

WORKDIR /usr/src/app
# 复制源码
COPY . .
# 编译 Release 版本
RUN cargo build --release

# 阶段 2：运行环境
FROM debian:bookworm-slim

# 安装 yt-dlp 和 ffmpeg 依赖
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    ffmpeg \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 下载最新的 yt-dlp 二进制文件到 /usr/local/bin
RUN curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp

WORKDIR /app

# 从 builder 拷贝二进制
COPY --from=builder /usr/src/app/target/release/ytb-music-downloader /app/ytb-music-downloader

# 创建必要的目录
RUN mkdir -p /app/downloads /app/logs

# 暴露端口
EXPOSE 3000

# 启动 Web 服务器
CMD ["/app/ytb-music-downloader", "web", "--port", "3000"]
