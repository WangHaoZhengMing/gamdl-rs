# 实现进度

已实现：

- 五 crate workspace、独立 CLI 和 GPUI Kit 桌面队列。
- 歌曲、专辑和歌单解析；目录分页、跨链接歌曲去重。
- Cookie 会话、播放资源、Widevine 许可证 API。
- HLS 变体、分片、初始化段、字节范围和有限重试。
- AES-128、可选 Rust Widevine CDM、FFmpeg CENC 解密。
- M4A 无转码封装、基础标签、完整解码校验和原子发布。
- 有界歌曲并发、取消、失败隔离、终态事件和汇总。
- CLI Ctrl-C；GUI 开始/取消队列及逐首状态。

默认内置原 gamdl 的设备身份，外部 .wvd 可覆盖；内置设备解析与 challenge 生成已测试。
真实 Apple Music 账号的许可证交换和图形桌面运行尚未验证。
支持范围及运行命令以 README.md 为准。

后续重点：真实账号联调；封面与歌词；按实际样本扩展媒体格式。保持单一下载链路，不引入网页工程或 Python。
