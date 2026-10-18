# 更新记录

## 1.0.0

首个正式版本，提供原生桌面应用与独立命令行工具。

### 功能

- 支持 Apple Music 歌曲、专辑和歌单链接，解析后选择歌曲并加入下载队列。
- 提供独立 Apple Music 网页登录窗口，支持系统凭据存储，也保留 Cookie 文件导入。
- 下载队列支持进度展示、搜索、状态筛选、取消、失败重试和打开文件位置。
- 支持歌词和封面嵌入，可另外保存歌词与封面文件。
- 支持简体中文、英文、德文和日文，在偏好设置中即时切换。
- 优化等待动画、Toast 反馈和下载页面布局，设置自动保存。
- 默认输出到系统下载目录中的 `AMd`，已有自定义输出目录优先。

### 下载与运行

- Windows x86_64：ZIP 包，运行 `gamdl-rs.exe`。
- macOS Intel / Apple Silicon：分别提供 ZIP 包，包含 `Music Downloader.app`。
- Linux x86_64：tar.gz 包，面向 Ubuntu 24.04 或兼容系统。
- 各平台包同时提供 `gamdl-rs-cli`，Release 附带 `SHA256SUMS.txt`。

运行需要单独安装 FFmpeg。Windows 网页登录需要 WebView2；Linux 需要桌面图形环境、
WebKitGTK 和 Secret Service。macOS 应用目前仅做临时签名，尚无 Developer ID 签名或 Apple 公证。

下载需要有效的 Apple Music 订阅。真实账号的播放资源、歌词和许可证交换仍需进一步验证；
当前不支持 ALAC/Atmos、MV、FairPlay/CBCS 和资料库下载。详细说明见 README 与随包的 RUNNING.md。
