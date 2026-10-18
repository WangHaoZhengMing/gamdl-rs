//! Native window integration: app-drawn chrome with platform window semantics.
use gpui_kit::component::TitleBar;
use gpui_kit::*;

pub fn options(cx: &App) -> WindowOptions {
    let mut options = TitleBar::window_options();
    options.window_bounds = Some(WindowBounds::Windowed(Bounds::centered(
        None,
        size(px(1180.), px(850.)),
        cx,
    )));
    options.window_min_size = Some(size(px(980.), px(720.)));
    options.app_id = Some("com.gamdl.rust".into());
    if let Some(titlebar) = &mut options.titlebar {
        titlebar.title = Some("Music · Downloader".into());
        #[cfg(target_os = "macos")]
        {
            titlebar.traffic_light_position = Some(point(px(14.), px(16.)));
        }
    }
    #[cfg(target_os = "linux")]
    {
        options.window_decorations = Some(WindowDecorations::Client);
    }
    options
}
