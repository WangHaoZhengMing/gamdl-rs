# Running a release / 使用发行包

Each archive contains the desktop application, `gamdl-rs-cli`, and license notices.
Extract the entire archive before launching. Install FFmpeg separately; it must be
on PATH or selected in the application's preferences. FFmpeg is not bundled.

每个压缩包包含桌面应用、命令行工具和许可声明。请先完整解压。
FFmpeg 需要单独安装，并加入 PATH，或在应用的偏好设置中选择其路径。

## Windows (x86_64)

Launch `gamdl-rs.exe`. The Apple Music login window requires Microsoft Edge WebView2
Runtime. Windows 10/11 normally include it; install the Evergreen Runtime from
Microsoft if it is missing. Use `gamdl-rs-cli.exe --help` for the CLI.

打开 `gamdl-rs.exe`。Apple Music 登录窗口需要 Microsoft Edge WebView2 Runtime；
缺少时请从微软安装 Evergreen Runtime。命令行工具为 `gamdl-rs-cli.exe`。

## macOS (Intel / Apple Silicon)

Choose the archive matching your processor (`x86_64` = Intel, `aarch64` = Apple
Silicon). Drag `Music Downloader.app` into Applications. The app is ad-hoc signed,
without Developer ID signing or Apple notarization. macOS may require approving
the first launch in System Settings > Privacy & Security. The CLI is the separate
`gamdl-rs-cli` executable. If FFmpeg is not found when opening the app from Finder,
select its full path in preferences.

选择适合芯片的压缩包，将 `Music Downloader.app` 拖入「应用程序」。当前应用仅做临时签名，
尚无 Developer ID 签名或 Apple 公证；首次启动可能需要在「系统设置 → 隐私与安全性」允许打开。
如果从 Finder 启动时无法找到 FFmpeg，请在偏好设置中选择其完整路径。

## Linux (x86_64, Ubuntu 24.04 or compatible)

Launch `./gamdl-rs` in a desktop session with Vulkan-capable graphics drivers.
The build uses Ubuntu 24.04 (glibc 2.39); older distributions are not supported by
this archive. Install the shared libraries, web engine, keyring service, and FFmpeg:

```sh
sudo apt-get update
sudo apt-get install ffmpeg libasound2t64 libfontconfig1 libfreetype6 \
  libwayland-client0 libwayland-cursor0 libwayland-egl1 \
  libxkbcommon0 libxkbcommon-x11-0 libxcb1 libxcb-shape0 libxcb-xfixes0 \
  libxcb-randr0 libxcb-render0 libxcb-shm0 libxcb-xinput0 libxcb-xkb1 \
  libvulkan1 libgtk-3-0t64 libwebkit2gtk-4.1-0 libdbus-1-3 gnome-keyring
```

Linux 包基于 Ubuntu 24.04 构建，需要兼容的系统、桌面会话和图形驱动。安装上面的依赖后运行
`./gamdl-rs`；账号凭据保存需要桌面会话提供 Secret Service（例如 GNOME Keyring）。

## Checksums / 校验

The release includes `SHA256SUMS.txt`. On Linux, run `sha256sum --check
SHA256SUMS.txt` from the directory containing all downloaded archives; on macOS,
use `shasum -a 256 -c SHA256SUMS.txt`. On Windows, compare an archive's hash with
`Get-FileHash <archive.zip> -Algorithm SHA256` in PowerShell.

Release 附带 `SHA256SUMS.txt`，可核对压缩包的 SHA-256，确认下载完整。
