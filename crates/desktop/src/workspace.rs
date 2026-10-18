use crate::{
    login::{self, BrowserSession},
    model::{self, DownloadStep, Filter, Page, Queue, Selection},
    preferences::{self, AuthMethod, Preferences},
};
mod authentication;
use gpui_kit::component::{
    Theme, ThemeMode,
    input::{InputEvent, InputState, TextareaState},
};
use gpui_kit::*;
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Resolve,
    Auth,
    Doctor,
    Login,
    Restore,
    Logout,
    VerifyLogin,
}
pub enum ToolResult {
    Resolved(app::Resolution),
    Auth(app::Account),
    Doctor(app::ToolVersion),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Cookies,
    Output,
    Device,
    Ffmpeg,
}
#[derive(Clone, Copy)]
pub enum Tone {
    Info,
    Success,
    Error,
}
#[derive(Clone, Copy)]
pub enum SaveStatus {
    Unchanged,
    Saving,
    Saved,
    Failed,
}
pub struct Notice {
    pub tone: Tone,
    pub text: String,
}
pub struct Activity {
    pub time: String,
    pub tone: Tone,
    pub text: String,
}

pub struct Workspace {
    pub auxiliary_windows: Vec<(Page, AnyWindowHandle)>,
    pub step: DownloadStep,
    pub selection: Selection,
    pub selection_search: Entity<InputState>,
    pub resolve_cancel: Option<app::CancellationToken>,
    pub page: Page,
    pub filter: Filter,
    pub queue: Queue,
    pub urls: Entity<TextareaState>,
    pub search: Entity<InputState>,
    pub cookies: Entity<InputState>,
    pub output: Entity<InputState>,
    pub device: Entity<InputState>,
    pub ffmpeg: Entity<InputState>,
    pub language: Entity<InputState>,
    pub jobs: usize,
    pub save_cover: bool,
    pub save_lyrics: bool,
    pub lyrics_format: app::LyricsFormat,
    pub dark: bool,
    pub ui_language: crate::i18n::UiLanguage,
    pub settings_locale_changed: bool,
    pub notice: Option<Notice>,
    pub activity: Vec<Activity>,
    pub account: Option<app::Account>,
    pub auth_method: AuthMethod,
    pub browser_session: Option<BrowserSession>,
    pub login_cancel: Option<app::CancellationToken>,
    pub tool_version: Option<String>,
    pub tool_busy: bool,
    pub active_tool: Option<Tool>,
    pub choosing: Option<Field>,
    pub save_status: SaveStatus,
    pub cancelling: bool,
    pub cancel: Option<app::CancellationToken>,
    pub config_path: PathBuf,
    runtime: tokio::runtime::Handle,
    queue_task: Option<Task<()>>,
    tool_task: Option<Task<()>>,
    login_task: Option<Task<()>>,
    autosave_task: Option<Task<()>>,
    autosave_revision: std::sync::Arc<std::sync::atomic::AtomicU64>,
    autosave_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    _subscriptions: Vec<Subscription>,
}
impl Workspace {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        runtime: tokio::runtime::Handle,
        preferences: Preferences,
        config_path: PathBuf,
        error: Option<String>,
    ) -> Self {
        let config = preferences.config;
        let urls =
            cx.new(|cx| TextareaState::new(window, cx).placeholder(tr!("input.urls_placeholder")));
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("input.queue_search")));
        let subscriptions = vec![
            cx.subscribe(&urls, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.selection = Selection::default();
                }
                cx.notify();
            }),
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        let input = |value: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let cookies = input(
            config.cookies_path.to_string_lossy().into_owned(),
            window,
            cx,
        );
        let cookie_subscription = cx.subscribe(&cookies, |view, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                view.account = None;
                if view.step == DownloadStep::Selection {
                    view.selection = Selection::default();
                    view.step = DownloadStep::Input;
                }
            }
            cx.notify();
        });
        let mut subscriptions = subscriptions;
        subscriptions.push(cookie_subscription);
        let selection_search =
            cx.new(|cx| InputState::new(window, cx).placeholder(tr!("input.selection_search")));
        subscriptions.push(cx.subscribe(&selection_search, |_, _, _: &InputEvent, cx| cx.notify()));
        let mut view = Self {
            auxiliary_windows: Vec::new(),
            step: DownloadStep::Input,
            selection: Selection::default(),
            selection_search,
            resolve_cancel: None,
            page: Page::Downloads,
            filter: Filter::All,
            queue: Queue::default(),
            urls,
            search,
            cookies,
            output: input(
                config.output_path.to_string_lossy().into_owned(),
                window,
                cx,
            ),
            device: input(
                config
                    .wvd_path
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                window,
                cx,
            ),
            ffmpeg: input(
                config.ffmpeg_path.to_string_lossy().into_owned(),
                window,
                cx,
            ),
            language: input(config.language, window, cx),
            jobs: config.jobs.clamp(1, 16),
            save_cover: config.save_cover,
            save_lyrics: config.save_lyrics,
            lyrics_format: config.lyrics_format,
            dark: preferences.dark,
            ui_language: preferences.ui_language,
            settings_locale_changed: false,
            notice: None,
            activity: Vec::new(),
            account: None,
            auth_method: preferences.auth_method,
            browser_session: None,
            login_cancel: None,
            tool_version: None,
            tool_busy: false,
            active_tool: None,
            choosing: None,
            save_status: SaveStatus::Unchanged,
            cancelling: false,
            cancel: None,
            config_path,
            runtime,
            queue_task: None,
            tool_task: None,
            login_task: None,
            autosave_task: None,
            autosave_revision: Default::default(),
            autosave_lock: Default::default(),
            _subscriptions: subscriptions,
        };
        view._subscriptions.push(
            cx.subscribe(&view.ffmpeg, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    view.tool_version = None;
                    cx.notify();
                }
            }),
        );
        for field in [
            &view.cookies,
            &view.output,
            &view.device,
            &view.ffmpeg,
            &view.language,
        ] {
            view._subscriptions
                .push(cx.subscribe(field, |view, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        view.autosave(cx);
                    }
                }));
        }
        if let Some(error) = error {
            view.message(Tone::Error, error);
        }
        if view.auth_method == AuthMethod::Browser {
            view.restore_login(cx);
        }
        view
    }
    pub fn busy(&self) -> bool {
        self.cancel.is_some()
    }
    pub fn set_ui_language(&mut self, language: crate::i18n::UiLanguage, cx: &mut Context<Self>) {
        if self.ui_language == language {
            return;
        }
        self.ui_language = language;
        self.settings_locale_changed = true;
        language.apply();
        let urls = self.urls.clone();
        let search = self.search.clone();
        let selection_search = self.selection_search.clone();
        let auxiliary = self.auxiliary_windows.clone();
        cx.defer(move |cx| {
            if let Some(handle) = cx.active_window().or_else(|| cx.windows().first().copied()) {
                let _ = handle.update(cx, |_, window, cx| {
                    urls.update(cx, |state, cx| {
                        state.set_placeholder(tr!("input.urls_placeholder"), window, cx)
                    });
                    search.update(cx, |state, cx| {
                        state.set_placeholder(tr!("input.queue_search"), window, cx)
                    });
                    selection_search.update(cx, |state, cx| {
                        state.set_placeholder(tr!("input.selection_search"), window, cx)
                    });
                });
            }
            for (page, handle) in auxiliary {
                let _ = handle.update(cx, |_, window, _| window.set_window_title(&page.title()));
            }
            cx.refresh_windows();
        });
        self.autosave(cx);
        cx.notify();
    }
    pub fn autosave(&mut self, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering;
        let revision = self.autosave_revision.fetch_add(1, Ordering::SeqCst) + 1;
        // Persist presentation preferences even while a path is being edited.
        // Required download fields are validated only when starting an operation.
        let config = self.config_values(cx);
        self.save_status = SaveStatus::Saving;
        cx.notify();
        let preferences = Preferences {
            config,
            dark: self.dark,
            auth_method: self.auth_method,
            ui_language: self.ui_language,
        };
        let path = self.config_path.clone();
        let latest = self.autosave_revision.clone();
        let lock = self.autosave_lock.clone();
        let runtime = self.runtime.clone();
        self.autosave_task = Some(cx.spawn(async move |view, cx| {
            if runtime
                .spawn(async { tokio::time::sleep(std::time::Duration::from_millis(400)).await })
                .await
                .is_err()
            {
                return;
            }
            let result = runtime
                .spawn(async move {
                    let _guard = lock.lock().await;
                    if latest.load(Ordering::SeqCst) != revision {
                        return Ok(());
                    }
                    preferences::save(&path, &preferences).await
                })
                .await;
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(error) => Some(error.to_string()),
            };
            let _ = view.update(cx, |view, cx| {
                if view.autosave_revision.load(Ordering::SeqCst) != revision {
                    return;
                }
                if let Some(error) = error {
                    view.save_status = SaveStatus::Failed;
                    view.message(Tone::Error, tr!("error.autosave", error = error));
                } else {
                    view.save_status = SaveStatus::Saved;
                }
                cx.notify();
            });
        }));
    }
    pub fn new_download(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy {
            return;
        }
        self.step = DownloadStep::Input;
        self.selection = Selection::default();
        self.notice = None;
        cx.notify();
    }
    pub fn message(&mut self, tone: Tone, text: impl Into<String>) {
        let text = text.into();
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            % 86400;
        self.activity.push(Activity {
            time: format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            ),
            tone,
            text: text.clone(),
        });
        if self.activity.len() > 300 {
            self.activity.remove(0);
        }
        self.notice = Some(Notice { tone, text });
    }
    fn config_values(&self, cx: &App) -> app::Config {
        let cookies = self.cookies.read(cx).value().trim().to_owned();
        let output = self.output.read(cx).value().trim().to_owned();
        let device = self.device.read(cx).value().trim().to_owned();
        let ffmpeg = self.ffmpeg.read(cx).value().trim().to_owned();
        let language = self.language.read(cx).value().trim().to_owned();
        app::Config {
            cookies_path: cookies.into(),
            output_path: output.into(),
            wvd_path: (!device.is_empty()).then(|| device.into()),
            ffmpeg_path: ffmpeg.into(),
            language,
            jobs: self.jobs,
            save_cover: self.save_cover,
            save_lyrics: self.save_lyrics,
            lyrics_format: self.lyrics_format,
        }
    }
    pub fn config(&self, cx: &App) -> Result<app::Config, String> {
        let config = self.config_values(cx);
        if config.cookies_path.as_os_str().is_empty() && self.auth_method == AuthMethod::CookieFile
        {
            return Err(tr!("validation.cookies"));
        }
        if config.output_path.as_os_str().is_empty() {
            return Err(tr!("validation.output"));
        }
        if config.ffmpeg_path.as_os_str().is_empty() {
            return Err(tr!("validation.ffmpeg"));
        }
        if config.language.is_empty() {
            return Err(tr!("validation.language"));
        }
        Ok(config)
    }
    pub fn start(&mut self, retry: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy {
            return;
        }
        let urls = if retry {
            self.queue.retry_urls()
        } else {
            if self.step != DownloadStep::Selection {
                return;
            }
            self.selection.urls()
        };
        if urls.is_empty() {
            self.message(
                Tone::Info,
                if retry {
                    tr!("download.nothing_to_retry")
                } else {
                    tr!("selection.required")
                },
            );
            cx.notify();
            return;
        }
        for url in &urls {
            if let Err(error) = app::parse_url(url) {
                self.message(Tone::Error, crate::i18n::app_error(&error));
                cx.notify();
                return;
            }
        }
        let config = match self.config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.message(Tone::Error, error);
                cx.notify();
                return;
            }
        };
        let credentials = match self.credentials() {
            Ok(credentials) => credentials,
            Err(error) => {
                self.message(Tone::Error, error);
                cx.notify();
                return;
            }
        };
        self.page = Page::Downloads;
        self.step = DownloadStep::Progress;
        self.filter = Filter::All;
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.queue = Queue::default();
        self.cancelling = false;
        self.message(
            Tone::Info,
            tr!("download.starting", p0 = urls.len(), p1 = config.jobs),
        );
        let cancel = app::CancellationToken::new();
        self.cancel = Some(cancel.clone());
        let (events, mut receiver) = tokio::sync::mpsc::channel(128);
        let job = self.runtime.spawn(app::download_with_credentials(
            config,
            urls,
            events,
            cancel,
            credentials,
        ));
        self.queue_task = Some(cx.spawn(async move |view, cx| {
            while let Some(event) = receiver.recv().await {
                if view
                    .update(cx, |view, cx| {
                        view.event(event);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
            let result = job.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(Ok(summary)) => {
                        let tone = if summary.failed > 0 {
                            Tone::Error
                        } else if summary.cancelled > 0 {
                            Tone::Info
                        } else {
                            Tone::Success
                        };
                        view.message(
                            tone,
                            tr!(
                                "download.summary",
                                p0 = summary.saved,
                                p1 = summary.skipped,
                                p2 = summary.failed,
                                p3 = summary.cancelled
                            ),
                        );
                    }
                    Ok(Err(error)) => {
                        view.queue.stopped();
                        if view.cancelling {
                            view.message(Tone::Info, tr!("download.cancelled"));
                        } else {
                            view.message(Tone::Error, crate::i18n::app_error(&error));
                        }
                    }
                    Err(error) => {
                        view.queue.stopped();
                        view.message(
                            Tone::Error,
                            tr!("error.download_interrupted", error = error),
                        );
                    }
                }
                view.cancel = None;
                view.cancelling = false;
                cx.notify();
            });
        }));
        cx.notify();
    }
    fn event(&mut self, event: app::Event) {
        if let app::Event::Warning { message, .. } = &event {
            self.message(Tone::Info, message.clone());
        }
        if let app::Event::State {
            id,
            state: app::State::Failed { message },
        } = &event
        {
            let title = self
                .queue
                .rows
                .iter()
                .find(|row| row.item.id == *id)
                .map(|row| row.item.title.clone())
                .unwrap_or_else(|| tr!("track.fallback"));
            self.message(Tone::Error, format!("{title}：{message}"));
        }
        self.queue.apply(&event);
    }
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if self.cancelling {
            return;
        }
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
            self.cancelling = true;
            self.message(Tone::Info, tr!("download.stopping"));
            cx.notify();
        }
    }
    pub fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let mut urls = model::urls(self.urls.read(cx).value().as_ref());
            let previous = urls.len();
            urls.extend(model::urls(&text));
            let urls = model::urls(&urls.join("\n"));
            let added = urls.len().saturating_sub(previous);
            let text = urls.join("\n");
            self.urls
                .update(cx, |state, cx| state.set_value(text, window, cx));
            self.message(
                Tone::Info,
                if added == 0 {
                    tr!("input.no_new_links")
                } else {
                    tr!("input.links_added", added = added)
                },
            );
        } else {
            self.message(Tone::Info, tr!("input.clipboard_empty"));
        }
        cx.notify();
    }
    pub fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        self.autosave(cx);
        Theme::change(
            if self.dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        cx.notify();
    }
    pub fn choose(&mut self, field: Field, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy || self.choosing.is_some() {
            return;
        }
        self.choosing = Some(field);
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: field != Field::Output,
            directories: field == Field::Output,
            multiple: false,
            prompt: Some(tr!("action.choose").into()),
        });
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            let _ = view.update(cx, |view, cx| {
                view.choosing = None;
                cx.notify();
            });
            match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let _ = view.update_in(cx, |view, window, cx| {
                            let entity = match field {
                                Field::Cookies => {
                                    view.account = None;
                                    &view.cookies
                                }
                                Field::Output => &view.output,
                                Field::Device => &view.device,
                                Field::Ffmpeg => &view.ffmpeg,
                            }
                            .clone();
                            entity.update(cx, |state, cx| {
                                state.set_value(path.to_string_lossy().into_owned(), window, cx)
                            });
                            cx.notify();
                        });
                    }
                }
                Ok(Ok(None)) => (),
                _ => {
                    let _ = view.update(cx, |view, cx| {
                        view.message(Tone::Error, tr!("error.file_picker"));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    pub fn run(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.tool_busy || self.busy() {
            return;
        }
        let config = match self.config(cx) {
            Ok(config) => config,
            Err(error) => {
                self.message(Tone::Error, error);
                cx.notify();
                return;
            }
        };
        let credentials = if tool == Tool::Doctor {
            app::Credentials::CookieFile
        } else {
            match self.credentials() {
                Ok(credentials) => credentials,
                Err(error) => {
                    self.message(Tone::Error, error);
                    cx.notify();
                    return;
                }
            }
        };
        let urls = model::urls(self.urls.read(cx).value().as_ref());
        if tool == Tool::Resolve {
            if urls.is_empty() {
                self.message(Tone::Info, tr!("input.links_required"));
                cx.notify();
                return;
            }
            for url in &urls {
                if let Err(error) = app::parse_url(url) {
                    self.message(Tone::Error, crate::i18n::app_error(&error));
                    cx.notify();
                    return;
                }
            }
        }
        self.tool_busy = true;
        self.active_tool = Some(tool);
        let cancel = app::CancellationToken::new();
        if matches!(tool, Tool::Resolve) {
            self.resolve_cancel = Some(cancel.clone());
            self.selection = Selection::default();
            self.notice = None;
        }
        let job = self.runtime.spawn(async move {
            match tool {
                Tool::Resolve => {
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => Err(tr!("resolve.cancelled_short")),
                        result = app::resolve_with_credentials(&config, &urls, &credentials) => result.map(ToolResult::Resolved).map_err(|error| crate::i18n::app_error(&error)),
                    }
                }
                Tool::Auth => app::check_auth_with_credentials(&config, &credentials)
                    .await
                    .map(ToolResult::Auth)
                    .map_err(|error| crate::i18n::app_error(&error)),
                Tool::Doctor => app::check_ffmpeg(&config)
                    .await
                    .map(ToolResult::Doctor)
                    .map_err(|error| crate::i18n::app_error(&error)),
                Tool::Login | Tool::Restore | Tool::Logout | Tool::VerifyLogin => unreachable!("login operations use their own task"),
            }
        });
        self.tool_task = Some(cx.spawn(async move |view, cx| {
            let result = job.await;
            let _ = view.update_in(cx, |view, window, cx| {
                let cancelled = view
                    .resolve_cancel
                    .as_ref()
                    .is_some_and(|token| token.is_cancelled());
                view.tool_busy = false;
                view.active_tool = None;
                view.resolve_cancel = None;
                if cancelled {
                    view.message(Tone::Info, tr!("resolve.cancelled"));
                    cx.notify();
                    return;
                }
                match result {
                    Ok(Ok(ToolResult::Resolved(resolution))) => {
                        view.account = Some(resolution.account);
                        view.selection.replace(resolution.tracks);
                        view.selection_search
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        if view.selection.tracks.is_empty() {
                            view.message(Tone::Info, tr!("resolve.no_tracks"));
                        } else {
                            view.step = DownloadStep::Selection;
                            view.message(
                                Tone::Success,
                                tr!("resolve.found", p0 = view.selection.tracks.len()),
                            );
                        }
                    }
                    Ok(Ok(ToolResult::Auth(account))) => {
                        view.message(
                            if account.active_subscription {
                                Tone::Success
                            } else {
                                Tone::Error
                            },
                            tr!(
                                "account.connected",
                                p0 = account.storefront,
                                p1 = if account.active_subscription {
                                    tr!("account.subscription_active")
                                } else {
                                    tr!("account.subscription_inactive")
                                }
                            ),
                        );
                        view.account = Some(account);
                    }
                    Ok(Ok(ToolResult::Doctor(version))) => {
                        view.message(Tone::Success, tr!("tools.ffmpeg_ready"));
                        view.tool_version = Some(version.version);
                    }
                    Ok(Err(error)) => {
                        if matches!(tool, Tool::Auth) {
                            view.account = None;
                        }
                        view.message(Tone::Error, error);
                    }
                    Err(error) => view.message(
                        Tone::Error,
                        tr!("error.operation_interrupted", error = error),
                    ),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if let Some(cancel) = &self.login_cancel {
            cancel.cancel();
        }
        if let Some(cancel) = &self.resolve_cancel {
            cancel.cancel();
        }
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
}
