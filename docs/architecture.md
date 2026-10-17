# Rust 重实现草案

参考：glomatico/gamdl，commit bc3bcd25ed61dcdb78c28ffb0ef32053b618c7e3。
按 Rust 模块和类型重新实现，不逐文件翻译 Python；当前进度见 progress.md。

## Workspace

- `app`：领域类型、配置、任务调度、进度事件与取消。提供 CLI 和 GUI 共用的应用入口。
- `apple`：Apple Music URL、Netscape Cookie 导入、会话、目录 API、分页、播放资源解析。
- `media`：HLS 解析、分片下载、重试、封装、标签、歌词与封面输出；受保护媒体后端的边界也放这里，先不额外拆 crate。
- `cli`：clap 参数和终端进度，独立二进制。
- `desktop`：基于 GPUI Kit 的原生桌面界面，输入、队列、进度、Cookie 路径、输出设置。

依赖方向：cli/desktop -> app -> apple/media；apple 和 media 不依赖 app。
API DTO 留在 apple，媒体类型留在 media；app 映射为应用任务，避免循环依赖。
只有真实复用需求出现时才提取公共类型 crate。

## 取舍

首版覆盖歌曲、专辑、歌单，AAC、基础标签、封面和 LRC。MV、艺人批量、资料库、ALAC/Atmos、Wrapper、多 DRM 后端和外部下载器后置。
一个 Tokio 运行时，reqwest + rustls、serde、thiserror、tracing；CLI 使用 clap。GUI 不阻塞 UI 线程，通过有界事件通道接收任务状态，合并高频进度更新。
Widevine CDM 通过可选 feature 接入，CLI/desktop 默认启用并内置原项目设备身份，外部文件可覆盖；可用 --no-default-features 关闭。配置采用 TOML，首版不加数据库、服务端、WebView 或前端构建链。
FFmpeg 先作为可检测的外部封装工具，不自行实现完整 MP4 引擎。

## 认证与媒体能力

Cookie 导入和会话检查先落地；不把 Cookie 内容写入配置或日志。
原项目还需要开发者 Bearer token，并从 Apple 网页资源提取它；此逻辑应隔离并处理缓存和失效。
Cookie 不提供媒体解密能力。现采用Widevine Rust CDM 与原项目内置设备身份（可由本地 .wvd 覆盖），配合 Apple 许可证交换和 FFmpeg CENC 解密；本地媒体链路已验证，真实账号联调尚未完成。

## 顺序

1. Workspace、Cookie 会话检查、URL 解析、CLI inspect。
2. 验证媒体能力，完成单曲下载、封装、标签和失败清理。
3. 专辑/歌单分页、并发限制、取消与重试。
4. GPUI Kit 窗口和下载队列，复用同一套 app。

UI 使用 longbridge/gpui-kit 0.7.3，已锁定依赖并通过类型检查。


## Rust 风格约定

- 用 struct 表达数据，用 enum 表达资源类型、任务状态和进度事件；通过 match 显式处理状态。
- 以模块和普通函数组织流程；impl 只承载类型相关操作，不建立 Manager/Service/Base 类层次。
- 使用组合与显式依赖传递；有实际替换需求时才引入 trait，有限的实现优先 enum，避免到处使用 dyn Trait。
- 库使用 Result 和 thiserror 定义有意义的错误，CLI/桌面入口补充上下文；避免正常路径中的 unwrap。
- 优先所有权与消息传递；仅对真正共享的数据使用 Arc，缩小锁的范围，不跨 await 持锁。
- GPUI Kit 的视图和 Entity 遵循框架模型，业务状态与任务执行留在 app。
