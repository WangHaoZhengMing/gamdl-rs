#[macro_use]
mod i18n;
rust_i18n::i18n!("locales", fallback = "en");
mod chrome;
mod login;
mod model;
mod preferences;
mod view;
mod workspace;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use workspace::Workspace;

fn main() {
    if std::env::args().nth(1).as_deref() == Some(login::HELPER_ARG) {
        i18n::UiLanguage::from_code(&std::env::var("GAMDL_UI_LOCALE").unwrap_or_default()).apply();
        login::run_helper();
        return;
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Could not start background runtime: {error}");
            return;
        }
    };
    let path = preferences::path();
    let (preferences, error) = match runtime.block_on(preferences::load(&path)) {
        Ok(p) => (p, None),
        Err(error) => (preferences::Preferences::default(), Some(error)),
    };
    let handle = runtime.handle().clone();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            preferences.ui_language.apply();
            cx.set_app_identity("com.gamdl.rust", "Music Downloader");
            Theme::change(
                if preferences.dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                },
                None,
                cx,
            );
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = chrome::options(cx);
            if let Err(error) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Workspace::new(window, cx, handle, preferences, path, error))
            }) {
                eprintln!("Could not open desktop window: {error}");
                cx.quit();
            }
        });
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
}
