use super::BrowserSession;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    platform::run_return::EventLoopExtRunReturn,
    window::{Window, WindowBuilder, WindowId},
};
use wry::{NewWindowFeatures, NewWindowResponse, WebView, WebViewBuilder};

const MUSIC_URL: &str = "https://music.apple.com/";
type Windows = Rc<RefCell<HashMap<WindowId, (Window, Rc<WebView>)>>>;

/// Only Apple-owned HTTPS pages can navigate or open authentication popups.
fn allowed_url(value: &str) -> bool {
    if value == "about:blank" {
        return true;
    }
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url
                .host_str()
                .is_some_and(|host| host == "apple.com" || host.ends_with(".apple.com"))
    })
}

fn create_window(
    target: &EventLoopWindowTarget<()>,
    windows: &Windows,
    opener: Option<NewWindowFeatures>,
    profile: &Path,
) -> Result<(Window, Rc<WebView>), String> {
    let window = WindowBuilder::new()
        .with_title(tr!("login.window_title"))
        .with_inner_size(LogicalSize::new(1000., 760.))
        .with_min_inner_size(LogicalSize::new(640., 560.))
        .build(target)
        .map_err(|_| tr!("login.window_create_failed"))?;
    let popup_target = target.clone();
    let popup_windows = windows.clone();
    let popup_profile = profile.to_owned();
    let mut context = wry::WebContext::new(Some(profile.to_owned()));
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_incognito(true)
        .with_devtools(false)
        .with_navigation_handler(|url| allowed_url(&url))
        .with_download_started_handler(|_, _| false)
        .with_new_window_req_handler(move |url, features| {
            if !allowed_url(&url) || popup_windows.borrow().len() >= 4 {
                return NewWindowResponse::Deny;
            }
            let Ok((window, webview)) = create_window(
                &popup_target,
                &popup_windows,
                Some(features),
                &popup_profile,
            ) else {
                return NewWindowResponse::Deny;
            };
            #[cfg(target_os = "windows")]
            let response = {
                use wry::WebViewExtWindows;
                NewWindowResponse::Create {
                    webview: webview.webview(),
                }
            };
            #[cfg(target_os = "macos")]
            let response = {
                use wry::WebViewExtMacOS;
                NewWindowResponse::Create {
                    webview: webview.webview().into_super(),
                }
            };
            #[cfg(target_os = "linux")]
            let response = {
                use wry::WebViewExtUnix;
                NewWindowResponse::Create {
                    webview: webview.webview(),
                }
            };
            popup_windows
                .borrow_mut()
                .insert(window.id(), (window, webview));
            response
        });
    let builder = if let Some(opener) = opener {
        #[cfg(target_os = "windows")]
        let builder = {
            use wry::WebViewBuilderExtWindows;
            builder.with_environment(opener.opener.environment)
        };
        #[cfg(target_os = "macos")]
        let builder = {
            use wry::WebViewBuilderExtMacOS;
            builder.with_webview_configuration(opener.opener.target_configuration)
        };
        #[cfg(target_os = "linux")]
        let builder = {
            use wry::WebViewBuilderExtUnix;
            builder.with_related_view(opener.opener.webview)
        };
        // Do not set a URL: the engine supplies it and preserves window.opener.
        builder
    } else {
        builder.with_url(MUSIC_URL)
    };
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(&window);
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let container = window.default_vbox().ok_or(tr!("login.gtk_failed"))?;
        builder.build_gtk(container)
    };
    let webview = webview.map_err(|_| tr!("login.webview_failed"))?;
    Ok((window, Rc::new(webview)))
}

fn session_cookie(webview: &WebView) -> Result<Option<BrowserSession>, String> {
    let cookies = webview
        .cookies_for_url(MUSIC_URL)
        .map_err(|_| tr!("login.cookie_read_failed"))?;
    for cookie in cookies {
        if cookie.name() != "media-user-token"
            || !matches!(
                cookie.domain(),
                Some("music.apple.com" | ".music.apple.com")
            )
        {
            continue;
        }
        let expires = cookie
            .expires_datetime()
            .and_then(|date| u64::try_from(date.unix_timestamp()).ok());
        if expires.is_some_and(|expiry| expiry <= super::now()) {
            continue;
        }
        return BrowserSession::new(cookie.value().to_owned(), expires).map(Some);
    }
    Ok(None)
}

pub(super) fn run() -> Result<Option<BrowserSession>, String> {
    let mut event_loop = EventLoop::new();
    let standalone_profile;
    let profile = match std::env::var_os(super::PROFILE_ENV) {
        Some(path) => PathBuf::from(path),
        None => {
            standalone_profile = tempfile::Builder::new()
                .prefix("gamdl-login-")
                .tempdir()
                .map_err(|_| tr!("login.profile_failed"))?;
            standalone_profile.path().to_owned()
        }
    };
    let windows: Windows = Default::default();
    let (window, webview) = create_window(&event_loop, &windows, None, &profile)?;
    let main_id = window.id();
    windows.borrow_mut().insert(main_id, (window, webview));
    let mut result = Ok(None);
    let mut next_check = Instant::now() + Duration::from_secs(1);
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    event_loop.run_return(|event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(next_check.min(deadline));
        match event {
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested | WindowEvent::Destroyed,
                ..
            } => {
                let removed = windows.borrow_mut().remove(&window_id);
                drop(removed);
                if window_id == main_id {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::MainEventsCleared if Instant::now() >= deadline => {
                result = Err(tr!("login.timeout"));
                *control_flow = ControlFlow::Exit;
            }
            Event::MainEventsCleared if Instant::now() >= next_check => {
                next_check = Instant::now() + Duration::from_secs(1);
                // Cookie APIs may pump native messages, so release the registry
                // borrow before calling them (a popup can close while waiting).
                let views: Vec<_> = windows
                    .borrow()
                    .values()
                    .map(|(_, view)| view.clone())
                    .collect();
                for view in views {
                    match session_cookie(&view) {
                        Ok(Some(session)) => {
                            result = Ok(Some(session));
                            *control_flow = ControlFlow::Exit;
                            break;
                        }
                        Ok(None) => (),
                        Err(error) => {
                            result = Err(error);
                            *control_flow = ControlFlow::Exit;
                            break;
                        }
                    }
                }
            }
            _ => (),
        }
    });
    // Drop WebViews before the event loop, including popup callback cycles.
    let remaining = std::mem::take(&mut *windows.borrow_mut());
    drop(remaining);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_navigation_rejects_external_origins_and_lookalikes() {
        for url in [
            MUSIC_URL,
            "https://idmsa.apple.com/appleauth/auth/signin",
            "https://account.apple.com/",
            "about:blank",
        ] {
            assert!(allowed_url(url), "{url}");
        }
        for url in [
            "http://music.apple.com/",
            "https://apple.com.evil.test/",
            "https://evilapple.com/",
            "https://apple.com@evil.test/",
            "https://user@apple.com/",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(!allowed_url(url), "{url}");
        }
    }
}
