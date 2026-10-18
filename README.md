# Apple Music Rust workspace

！use at your own risk！

轻量原生重实现，Rust 2024。GPUI Kit 桌面端与独立 CLI 共用同一套任务队列。
<p align="center">
  <img src="https://pic1.imgdb.cn/i/034htOLSKwkV7zeINHoOKb.png" width="48%" />
  <img src="https://pic1.imgdb.cn/i/034htQmkATA2KpIZhxAGUz.png" width="48%" />
</p>
多语言支持

![pasted_1792302526622_deqphr.png](https://pic1.imgdb.cn/i/034htTtXHAWAHIzwEcBrlg.png)

目前支持歌曲、专辑和歌单 URL，专辑/歌单分页展开并按歌曲 ID 去重；优先 AAC 256kbps（`28:ctrp256`）。
媒体层支持点播 HLS、初始化段、字节范围、AES-128 和单密钥 CENC，使用 FFmpeg 无转码封装为 M4A。
包含标题、歌手、专辑、曲序、碟号和日期标签。默认还嵌入歌词和封面。封装后完整解码校验，成功才发布输出；不覆盖已有文件。

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
MV、资料库、ALAC/Atmos 仍待实现；歌单中的非歌曲项目不加入队列。

## 歌词与封面

每首歌曲优先请求逐字歌词；不可用或实际没有逐字时间时，回退逐行同步歌词，再回退纯文本。
没有歌词或封面时仍保存音频；请求失败会在活动记录和 CLI 事件中给出提示。

默认使用 M4A `©lyr` 标签嵌入 Enhanced LRC（逐字）、普通 LRC（逐行）或纯文本；封面写入 `covr`。
歌曲 ID 和对应地区的歌曲网址分别写入 freeform 标签 `com.gamdl-rs:APPLE_MUSIC_ID` 和 `com.gamdl-rs:APPLE_MUSIC_URL`，随 M4A 文件保存。网址使用规范化歌曲链接，不保留语言或分享参数；播放器不一定展示自定义标签。已存在而跳过的文件不会自动补写。
原始 TTML 另外保存在应用自定义 freeform 标签 `com.gamdl-rs:LYRICS_TTML`，保留人声、时间和其他原始信息。
`©lyr` 本质上是文本标签，逐字显示取决于播放器是否理解 Enhanced LRC；自定义 TTML 标签不代表所有播放器都能识别。
已有音频文件仍按原规则跳过，不会自动补写旧文件的封面与歌词。

可选额外保存文件：

```sh
cargo run -p cli -- download --save-cover --save-lyrics --lyrics-format lrc 'Apple Music URL'
```

文件与歌曲同名：封面 `.jpg`/`.png`，同步歌词 `.lrc`（逐字使用 Enhanced LRC），无同步时间时 `.txt`。
选择 `--lyrics-format ttml` 时保存原始 `.ttml`；独立文件不会覆盖已有文件。
独立文件在音频最终发布前写入；若最终保存失败，已经成功发布的独立封面/歌词可能保留，但不会留下未校验的音频。
封面使用 Apple 返回的图片模板请求 1200×1200，不增加图片转换依赖。

当前 Apple 歌词 API 的真实账号联调仍待验证；不保证每首歌或每个地区都有逐字歌词。

## 配置

通过 `--config config.toml` 显式加载，未知字段会报错：

```toml
cookies_path = "cookies.txt"
output_path = "Apple Music"
ffmpeg_path = "ffmpeg"
language = "zh-CN"
jobs = 2
save_cover = false
save_lyrics = false
lyrics_format = "lrc"
# 可选：省略时使用内置身份
# wvd_path = "/path/to/device.wvd"
```

命令行 `--cookies`、`download --output`、`--jobs`、`--wvd` 覆盖配置。
不填写 `wvd_path` 时不要在 TOML 中写空字符串，直接省略该字段。

## 桌面

```sh
cargo run -p desktop
```

主界面使用 `longbridge/gpui-kit` 0.7.3；Apple Music 网页登录使用独立的 Wry 窗口。
桌面工作区提供：

默认保存到系统下载目录下的 `AMd`，通过跨平台目录 API 获取下载位置（支持重定向或本地化的下载目录）。未提供系统下载位置时回退到用户目录下的 `Downloads/AMd`；已有输出目录配置优先。

主窗口专注下载流程，不设侧栏。右上角的设置与活动记录图标分别打开独立窗口；重复点击激活已有窗口，设置和记录共享当前会话状态。

界面支持简体中文、英文、德文和日文，在「偏好设置 → 外观 → 界面语言」选择 `简体中文`、`English`、`Deutsch` 或 `日本語` 即时切换，自动保存，下次启动恢复。界面语言与「元数据语言」独立，切换时保留输入、歌曲选择和下载任务。
翻译使用 `rust-i18n`，资源位于 `crates/desktop/locales/` 下的 `zh-CN.json`、`en.json`、`de.json` 和 `ja.json`，编译时嵌入程序，无需额外分发语言文件；缺失翻译回退到英文。设置搜索、下拉框与输入菜单通过翻译扩展同步本地化。Apple 登录网页的内容由 Apple 控制，原始服务错误详情保留其原文。

- **下载工作台**：分为「输入链接 → 选择歌曲 → 下载进度」三步。解析完整展开歌曲、专辑和歌单并去重，不启动下载；结果列表支持左侧复选框、多选、全选和搜索，确认后仅下载勾选的歌曲。可返回修改链接，解析支持取消。
- **下载队列**：真实数量统计、分片进度、虚拟列表、歌手/标题/ID 搜索、状态筛选、取消、重试未完成、打开已保存文件位置、复制失败原因。
- **偏好设置**：Cookie、输出目录、FFmpeg 和外部设备文件的原生选择器；账号/订阅检查、媒体工具检测、1–16 首并发、元数据语言、浅色/深色主题。
- **活动记录**：本次会话最近 300 条操作反馈，失败原因可查看，不记录 Cookie、challenge、许可证或密钥。

修改设置后自动保存，下次启动自动恢复。设置位于各系统的配置目录 `gamdl-rs/desktop.toml`：
Windows 为 `%APPDATA%`，macOS 为 `~/Library/Application Support`，Linux 为 `$XDG_CONFIG_HOME` 或 `~/.config`。
桌面配置的 `[download]` 保存共享下载配置，顶层 `dark` 保存主题，不会自动读取或覆盖 CLI 配置。
账号状态不会持久化，解析成功后自动同步订阅与地区；修改 Cookie 后重新解析。活动记录与队列仅保留在当前进程。
重试使用每首歌曲原来的地区链接，并跳过已经保存的文件。

Windows、macOS 和 Linux 均使用 GPUI Kit 自绘标题栏主体：Windows 保留系统命中区和窗口控制语义；
macOS 保留原生红绿灯并自定义位置、背景和内容；Linux 请求客户端装饰，提供边缘缩放和窗口管理器支持的按钮。
拖动、双击和系统窗口菜单由 Kit 对接系统。若 Linux 合成器强制服务端装饰，遵循其策略，避免重复窗口按钮。
关闭主视图会取消下载，关闭最后一个窗口退出程序。

Linux 需要显示服务、图形驱动和开发库。Debian/Ubuntu 可用 root 执行 `scripts/install-linux-deps.sh`。
云环境使用用户目录开发包；可通过以下命令检查 desktop：

```sh
PKG_CONFIG_PATH=/workspace/.native/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig:/workspace/.native/sysroot/usr/share/pkgconfig \
PKG_CONFIG_SYSROOT_DIR=/workspace/.native/sysroot cargo check -p desktop --features widevine
```

默认 `cargo build` / `cargo run` 只构建 CLI。

## 构建与发布

GitHub Actions 的 `Release` 工作流在推送 `v*` 标签时自动构建并发布；也可在 Actions 页面手动运行，填写一个已经推送的版本标签。标签必须与根目录 `Cargo.toml` 的 `[workspace.package].version` 一致，例如版本 `1.0.0` 对应 `v1.0.0`，预发布版本 `1.1.0-rc.1` 对应 `v1.1.0-rc.1`。

```sh
# 先提交并推送准备发布的代码，再创建版本标签
git tag v1.0.0
git push origin v1.0.0
```

工作流使用 Rust 1.99.0、锁定的依赖和默认功能，先测试再构建桌面端与 CLI。四个目标全部成功后，统一上传到该标签的 GitHub Release，并使用 `CHANGELOG.md` 作为发布说明：

| 平台 | 架构 | 发布文件 |
| --- | --- | --- |
| Windows | x86_64 | `gamdl-rs-v版本-windows-x86_64.zip` |
| macOS | Intel x86_64 | `gamdl-rs-v版本-macos-x86_64.zip` |
| macOS | Apple Silicon aarch64 | `gamdl-rs-v版本-macos-aarch64.zip` |
| Linux | x86_64，Ubuntu 24.04 或兼容系统 | `gamdl-rs-v版本-linux-x86_64.tar.gz` |

每个包包含桌面应用、`gamdl-rs-cli`、使用说明和许可声明；macOS 桌面应用为 `Music Downloader.app`。Release 另附 `SHA256SUMS.txt`，带 `-rc.1` 等后缀的版本自动标记为预发布。重跑会更新已有 Release 的同名文件，保留发布说明。使用仓库自带的 `GITHUB_TOKEN`，无需另配发布密钥。

FFmpeg 暂不随包附带；Windows 登录依赖 WebView2，Linux 需要图形环境、WebKitGTK 和 Secret Service。macOS 目前只有临时签名，尚无 Developer ID 签名和 Apple 公证。详细运行步骤见随包附带的 [RUNNING.md](scripts/release-running.md)。

## 结构与验证

- `apple`：URL、Cookie、会话、目录分页、播放资源与许可证 API。
- `media`：HLS、HTTP 重试、AES/CDM、FFmpeg 封装校验及原子发布。
- `app`：配置、队列、并发、取消、事件与输出路径。
- `cli`：参数、Ctrl-C、JSON 进度和统计。
- `desktop`：GPUI Kit 队列界面。

```sh
cargo fmt --all -- --check
cargo test --locked -p apple -p media -p app -p cli
cargo test --locked -p desktop --bin desktop
cargo test --locked -p apple -p media -p app -p cli --features cli/widevine
cargo clippy --locked -p apple -p media -p app -p cli --all-targets --features cli/widevine -- -D warnings
```

媒体集成测试使用本地 HTTP 服务及 FFmpeg 生成的音频，不需要账号或设备文件。
覆盖 AES/CENC 解密、错误密钥拒绝发布、HTTP 重试、标签、取消清理和不覆盖输出；队列测试验证并发上限、失败隔离及取消统计。
内置设备已通过本地解析和许可证 challenge 生成测试；**尚未使用真实账号验证 Apple Music 的播放资源和许可证交换。**


## FFmpeg 分发评估

目前仍需要可用 FFmpeg，尚未将其打包。虽然有纯 rust 替代方案但是后续的高级功能拓展仍要依赖 ffmpeg 。
