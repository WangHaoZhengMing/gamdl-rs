use gpui_kit::component::{
    Disableable,
    button::*,
    input::{Input, InputState},
};
use gpui_kit::*;

#[derive(Clone, Copy)]
enum Operation {
    Parse,
    Auth,
    Inspect,
}

enum Status {
    Ready,
    Working,
    Done(String),
}

struct Home {
    url: Entity<InputState>,
    cookies: Entity<InputState>,
    output: Entity<InputState>,
    wvd: Entity<InputState>,
    queue: Vec<Row>,
    cancel: Option<app::CancellationToken>,
    status: Status,
    runtime: tokio::runtime::Handle,
    task: Option<Task<()>>,
}

impl Home {
    fn new(window: &mut Window, cx: &mut Context<Self>, runtime: tokio::runtime::Handle) -> Self {
        Self {
            url: cx.new(|cx| InputState::new(window, cx).placeholder("https://music.apple.com/…")),
            cookies: cx.new(|cx| InputState::new(window, cx).default_value("cookies.txt")),
            output: cx.new(|cx| InputState::new(window, cx).default_value("Apple Music")),
            wvd: cx.new(|cx| {
                InputState::new(window, cx).placeholder("留空使用内置身份；也可填写 .wvd 路径")
            }),
            queue: Vec::new(),
            cancel: None,
            status: Status::Ready,
            runtime,
            task: None,
        }
    }

    fn start_download(&mut self, cx: &mut Context<Self>) {
        if matches!(self.status, Status::Working) {
            return;
        }
        let urls = self
            .url
            .read(cx)
            .value()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let wvd = self.wvd.read(cx).value().to_string();
        let config = app::Config {
            cookies_path: self.cookies.read(cx).value().to_string().into(),
            output_path: self.output.read(cx).value().to_string().into(),
            wvd_path: if wvd.trim().is_empty() {
                None
            } else {
                Some(wvd.into())
            },
            ..Default::default()
        };
        let cancel = app::CancellationToken::new();
        self.cancel = Some(cancel.clone());
        self.queue.clear();
        self.status = Status::Working;
        let (events, mut receiver) = tokio::sync::mpsc::channel(128);
        let job = self
            .runtime
            .spawn(app::download(config, urls, events, cancel));
        self.task = Some(cx.spawn(async move |view, cx| {
            while let Some(event) = receiver.recv().await {
                if view
                    .update(cx, |view, cx| {
                        view.apply(event);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
            let message = match job.await {
                Ok(Ok(summary)) => format!(
                    "完成 {} · 跳过 {} · 失败 {} · 取消 {}",
                    summary.saved, summary.skipped, summary.failed, summary.cancelled
                ),
                Ok(Err(error)) => error.to_string(),
                Err(error) => format!("Task failed: {error}"),
            };
            let _ = view.update(cx, |view, cx| {
                view.cancel = None;
                view.status = Status::Done(message);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn apply(&mut self, event: app::Event) {
        match event {
            app::Event::Queued { items } => {
                self.queue = items
                    .into_iter()
                    .map(|item| Row {
                        item,
                        state: app::State::Pending,
                        progress: String::new(),
                    })
                    .collect()
            }
            app::Event::State { id, state } => {
                if let Some(row) = self.queue.get_mut(id) {
                    row.state = state;
                }
            }
            app::Event::Progress {
                id,
                completed,
                total,
                bytes,
            } => {
                if let Some(row) = self.queue.get_mut(id) {
                    row.progress = format!(
                        "{completed}/{total} · {:.1} MiB",
                        bytes as f64 / 1_048_576.0
                    );
                }
            }
            app::Event::Planning | app::Event::Finished { .. } => (),
        }
    }

    fn run(&mut self, operation: Operation, cx: &mut Context<Self>) {
        if matches!(self.status, Status::Working) {
            return;
        }
        let url = self.url.read(cx).value().to_string();
        let config = app::Config {
            cookies_path: self.cookies.read(cx).value().to_string().into(),
            ..Default::default()
        };
        self.status = Status::Working;
        let job = self.runtime.spawn(async move {
            let result = match operation {
                Operation::Parse => app::parse_url(&url).map(serde_json::to_value),
                Operation::Auth => app::check_auth(&config).await.map(serde_json::to_value),
                Operation::Inspect => app::inspect(&config, &url).await.map(serde_json::to_value),
            };
            match result {
                Ok(Ok(value)) => {
                    serde_json::to_string_pretty(&value).unwrap_or_else(|error| error.to_string())
                }
                Ok(Err(error)) => error.to_string(),
                Err(error) => error.to_string(),
            }
        });
        self.task = Some(cx.spawn(async move |view, cx| {
            let message = match job.await {
                Ok(message) => message,
                Err(error) => format!("Task failed: {error}"),
            };
            let _ = view.update(cx, |view, cx| {
                view.status = Status::Done(message);
                cx.notify();
            });
        }));
        cx.notify();
    }
}

struct Row {
    item: app::QueueItem,
    state: app::State,
    progress: String,
}
impl Row {
    fn label(&self) -> String {
        let status = match &self.state {
            app::State::Pending => "等待中".into(),
            app::State::Resolving => "解析资源".into(),
            app::State::Downloading => format!("下载中 {}", self.progress),
            app::State::Decrypting => "获取许可证".into(),
            app::State::Muxing => "封装中".into(),
            app::State::Verifying => "校验中".into(),
            app::State::Complete { .. } => "已完成".into(),
            app::State::Skipped { .. } => "文件已存在".into(),
            app::State::Failed { message } => format!("失败：{message}"),
            app::State::Cancelled => "已取消".into(),
        };
        format!("{} · {} — {status}", self.item.title, self.item.artist)
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
    }
}

impl Render for Home {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = matches!(self.status, Status::Working);
        let message = match &self.status {
            Status::Ready => "输入链接以解析或查询元数据。".to_string(),
            Status::Working => "正在处理…".to_string(),
            Status::Done(message) => message.clone(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_8()
            .child(div().text_2xl().child("Apple Music"))
            .child("链接（多个链接以空格分隔）")
            .child(Input::new(&self.url))
            .child("Cookie 文件路径")
            .child(Input::new(&self.cookies))
            .child("输出目录")
            .child(Input::new(&self.output))
            .child("设备身份（可选覆盖）")
            .child(Input::new(&self.wvd))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("parse")
                            .label("解析链接")
                            .disabled(busy)
                            .on_click(cx.listener(|view, _, _, cx| view.run(Operation::Parse, cx))),
                    )
                    .child(
                        Button::new("auth")
                            .label("检查登录")
                            .disabled(busy)
                            .on_click(cx.listener(|view, _, _, cx| view.run(Operation::Auth, cx))),
                    )
                    .child(
                        Button::new("inspect")
                            .primary()
                            .label("查询信息")
                            .disabled(busy)
                            .on_click(
                                cx.listener(|view, _, _, cx| view.run(Operation::Inspect, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("download")
                            .primary()
                            .label("开始下载")
                            .disabled(busy)
                            .on_click(cx.listener(|view, _, _, cx| view.start_download(cx))),
                    )
                    .child(
                        Button::new("cancel")
                            .label("取消队列")
                            .disabled(self.cancel.is_none())
                            .on_click(cx.listener(|view, _, _, cx| {
                                if let Some(cancel) = &view.cancel {
                                    cancel.cancel();
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(div().child(message))
            .child(
                div()
                    .id("queue")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(self.queue.iter().map(|row| div().py_2().child(row.label()))),
            )
    }
}

fn main() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Could not start background runtime: {error}");
            return;
        }
    };
    let handle = runtime.handle().clone();
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        if let Err(error) = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| Home::new(window, cx, handle))
        }) {
            eprintln!("Could not open desktop window: {error}");
            cx.quit();
        }
    });
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
}
