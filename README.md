# Apple Music Rust workspace

轻量原生重实现，Rust 2024。GPUI Kit 桌面端与独立 CLI 共用同一套任务队列。

目前支持歌曲、专辑和歌单 URL，专辑/歌单分页展开并按歌曲 ID 去重；优先 AAC 256kbps（`28:ctrp256`）。
媒体层支持点播 HLS、初始化段、字节范围、AES-128 和单密钥 CENC，使用 FFmpeg 无转码封装为 M4A。
包含标题、歌手、专辑、曲序、碟号和日期标签。封装后完整解码校验，成功才发布输出；不覆盖已有文件。

## CLI

```sh
cargo run -p cli -- parse 'https://music.apple.com/us/album/example/123?i=456'
cargo run -p cli -- doctor
cargo run -p cli -- --cookies /path/to/cookies.txt auth
cargo run -p cli -- --cookies /path/to/cookies.txt inspect 'https://music.apple.com/us/song/123'

# 下载一个或多个歌曲、专辑或歌单
cargo run -p cli -- --cookies /path/to/cookies.txt download \
  --output './Apple Music' --jobs 2 \
  'https://music.apple.com/us/album/example/123'
```

`parse` 完全离线。在线命令需要从 Apple Music 登录会话导出的 Netscape Cookie。
Cookie 的 `media-user-token` 必须属于 `music.apple.com` 且未过期。
认证还会访问 Apple 网页资源获取开发者 Token，网页结构变化可能导致失败。
下载需要有效订阅，运行环境需提供 FFmpeg。

下载进度以逐行 JSON 事件写到 stderr，最终统计 JSON 写到 stdout。
并发限制为 1–16 首歌曲，每首按顺序读取分片，默认并发 2 首。
Ctrl-C 取消正在执行及尚未开始的任务，临时文件会清理；单首失败不阻止其他任务。
失败或取消时返回非零退出码。重跑同一批链接会跳过已保存文件，重新处理之前失败的歌曲。
已有文件仅按路径判断，不自动检查或修复其内容。

输出路径为 `歌手/专辑/碟号-曲序 标题 [歌曲ID].m4a`，过滤路径分隔符、非法字符和 Windows 保留名称。
下载在目标目录内使用独立临时目录；最终通过不覆盖已有路径的硬链接原子发布，目标文件系统需支持硬链接。

## 认证与解密

Cookie 只提供账号会话，不能单独完成受保护媒体解密。
CLI 和桌面端默认启用 `widevine`，连接 Rust CDM、Apple 许可证交换和 CENC 解密。
默认内置与原 gamdl 同源的设备身份，普通用户无需另找 `.wvd`，只需导入 Cookie。
通过 `--wvd` 或配置 `wvd_path` 可覆盖内置身份；显式指定的文件无效时直接报错，不静默回退。
不依赖 Python，不要求 Wrapper 服务。轻量构建可使用 `--no-default-features` 关闭 CDM；此时 CENC 任务明确失败，不会把加密文件当作成功输出。
内置身份与原项目共享，服务方撤销后可能失效；来源和原项目许可保存在 `crates/media/assets/NOTICE.md`。

Cookie、CDM challenge、许可证和内容密钥不进入进度事件或配置内容。
AES 与 CENC 内容密钥由 zeroize 在释放时清理。FFmpeg 的 CENC 密钥作为子进程参数传递，可能对同一机器上有进程检查权限的用户可见。
`widevine` 0.1.0 发布元数据标记为 GPL-3.0；启用此可选依赖后，分发组合二进制需遵守其许可。CLI 和桌面端默认启用该依赖。

暂不支持 FairPlay/CBCS、CENC 密钥轮换、PlayReady、Wrapper、直播、分片时间轴不连续和变化的初始化段。
MV、资料库、ALAC/Atmos、封面及歌词输出仍待实现；歌单中的非歌曲项目不加入队列。

## 配置

通过 `--config config.toml` 显式加载，未知字段会报错：

```toml
cookies_path = "cookies.txt"
output_path = "Apple Music"
ffmpeg_path = "ffmpeg"
language = "zh-CN"
jobs = 2
# 可选：省略时使用内置身份
# wvd_path = "/path/to/device.wvd"
```

命令行 `--cookies`、`download --output`、`--jobs`、`--wvd` 覆盖配置。
不填写 `wvd_path` 时不要在 TOML 中写空字符串，直接省略该字段。

## 桌面

```sh
cargo run -p desktop
```

使用 `longbridge/gpui-kit` 0.7.3，无 WebView 或网页工程。
可输入多个空格分隔的链接、Cookie 路径、输出目录，开始下载或取消队列。设备文件路径留空即可使用内置身份。
队列显示每首的状态及分片进度；后台 Tokio 任务运行下载和许可证请求，不阻塞 UI 线程。
关闭主视图时取消队列。桌面端目前使用默认并发 2，不读取 CLI 的 TOML 配置。

Linux 需要显示服务、图形驱动和开发库。Debian/Ubuntu 可用 root 执行 `scripts/install-linux-deps.sh`。
云环境已通过用户目录开发包完成 desktop 类型检查，尚未进行图形运行验证：

```sh
PKG_CONFIG_PATH=/workspace/.native/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig:/workspace/.native/sysroot/usr/share/pkgconfig \
PKG_CONFIG_SYSROOT_DIR=/workspace/.native/sysroot cargo check -p desktop --features widevine
```

默认 `cargo build` / `cargo run` 只构建 CLI。

## 结构与验证

- `apple`：URL、Cookie、会话、目录分页、播放资源与许可证 API。
- `media`：HLS、HTTP 重试、AES/CDM、FFmpeg 封装校验及原子发布。
- `app`：配置、队列、并发、取消、事件与输出路径。
- `cli`：参数、Ctrl-C、JSON 进度和统计。
- `desktop`：GPUI Kit 队列界面。

```sh
cargo fmt --all -- --check
cargo test --locked -p apple -p media -p app -p cli
cargo test --locked -p apple -p media -p app -p cli --features cli/widevine
cargo clippy --locked -p apple -p media -p app -p cli --all-targets --features cli/widevine -- -D warnings
```

媒体集成测试使用本地 HTTP 服务及 FFmpeg 生成的音频，不需要账号或设备文件。
覆盖 AES/CENC 解密、错误密钥拒绝发布、HTTP 重试、标签、取消清理和不覆盖输出；队列测试验证并发上限、失败隔离及取消统计。
内置设备已通过本地解析和许可证 challenge 生成测试；**尚未使用真实账号验证 Apple Music 的播放资源和许可证交换。**
