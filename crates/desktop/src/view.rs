mod activity;
mod downloads;
mod flow;
mod motion;
mod settings;
use motion::animated_button;

use crate::{
    model::{self, DownloadStep, Filter, Page},
    workspace::{Field, Tone, Tool, Workspace},
};
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, TitleBar, WindowBorder, WindowExt,
    button::*,
    input::{Input, Textarea},
    notification::Notification,
    progress::Progress,
    spinner::Spinner,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

fn accent() -> Hsla {
    rgb(0xe64c70).into()
}
fn tone_color(tone: Tone) -> Hsla {
    match tone {
        Tone::Info => rgb(0x718096).into(),
        Tone::Success => rgb(0x22996b).into(),
        Tone::Error => rgb(0xe14c58).into(),
    }
}
fn card(cx: &App) -> Div {
    div()
        .bg(cx.theme().background)
        .border_1()
        .border_color(cx.theme().border)
        .rounded_lg()
}
fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}
fn heading(title: String, description: String, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(muted(description, cx))
}
fn badge(text: impl Into<SharedString>, color: Hsla, cx: &App) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(cx.theme().muted)
        .text_xs()
        .text_color(color)
        .child(text.into())
}

impl Workspace {
    fn titlebar(&self, cx: &mut Context<Self>) -> TitleBar {
        TitleBar::new()
            .h(px(40.))
            .border_0()
            .bg(cx.theme().background)
    }
    // Present once, in the active workspace window. The component owns toast
    // animation, hover-paused expiry, dismissal and reduced-motion behavior.
    fn present_notice(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(notice) = self.notice.take() else {
            return;
        };
        let preview = model::notice_preview(&notice.text);
        let note = match notice.tone {
            Tone::Info => Notification::info(preview),
            Tone::Success => Notification::success(preview),
            Tone::Error => {
                let workspace = cx.entity().downgrade();
                Notification::error(preview)
                    .title(tr!("error.operation_title"))
                    .action(move |_, _, _| {
                        let workspace = workspace.clone();
                        Button::new("notice-details")
                            .ghost()
                            .label(tr!("action.details"))
                            .on_click(move |_, _, cx| {
                                let _ = workspace.update(cx, |view, cx| {
                                    view.open_auxiliary(Page::Activity, cx);
                                });
                            })
                    })
            }
        }
        .id::<Workspace>()
        .placement(Anchor::BottomRight);
        let fallback = window.window_handle();
        let active = cx.active_window().filter(|handle| {
            *handle == fallback || self.auxiliary_windows.iter().any(|(_, own)| own == handle)
        });
        cx.defer(move |cx| {
            let _ = active.unwrap_or(fallback).update(cx, |_, window, cx| {
                window.push_notification(note, cx);
            });
        });
    }
    fn open_auxiliary(&mut self, page: Page, cx: &mut Context<Self>) {
        let workspace = cx.entity();
        cx.defer(move |cx| {
            let existing = workspace
                .read(cx)
                .auxiliary_windows
                .iter()
                .find(|(kind, _)| *kind == page)
                .map(|(_, handle)| *handle);
            if let Some(handle) = existing
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                return;
            }
            let mut options = crate::chrome::options(cx);
            if let Some(titlebar) = &mut options.titlebar {
                titlebar.title = Some(page.title().into());
            }
            let shared = workspace.clone();
            let result = gpui_kit::open_window(options, cx, move |_, cx| {
                cx.new(|cx| {
                    let subscription = cx.observe(&shared, |_, _, cx| cx.notify());
                    AuxiliaryWindow {
                        workspace: shared,
                        page,
                        _subscription: subscription,
                    }
                })
            });
            workspace.update(cx, |view, cx| {
                view.auxiliary_windows.retain(|(kind, _)| *kind != page);
                match result {
                    Ok((handle, _)) => view.auxiliary_windows.push((page, handle)),
                    Err(error) => {
                        view.message(Tone::Error, tr!("error.window_open", error = error))
                    }
                }
                cx.notify();
            });
        });
    }
}

struct AuxiliaryWindow {
    workspace: Entity<Workspace>,
    page: Page,
    _subscription: Subscription,
}
impl Render for AuxiliaryWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page;
        self.workspace.update(cx, |view, cx| {
            view.present_notice(window, cx);
            let body = match page {
                Page::Settings => view.settings(cx).into_any_element(),
                Page::Activity => view.activity_view(cx).into_any_element(),
                Page::Downloads => div().into_any_element(),
            };
            WindowBorder::new()
                .child(
                    div()
                        .size_full()
                        .relative()
                        .flex()
                        .flex_col()
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .font_family(cx.theme().font_family.clone())
                        .child(
                            view.titlebar(cx).child(
                                div().flex_1().flex().items_center().px_3().child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(page.title()),
                                ),
                            ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .when(page != Page::Settings, |content| content.p_6())
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(body),
                        ),
                )
                .into_any_element()
        })
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.present_notice(window, cx);
        let body = self.downloads(cx);
        let header = self.flow_header(cx);
        WindowBorder::new().child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .font_family(cx.theme().font_family.clone())
                .child(
                    self.titlebar(cx).child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(header)
                            .child(div().flex_1())
                            .child(
                                div()
                                    .occlude()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .child(animated_button(
                                        Button::new("open-activity")
                                            .ghost()
                                            .small()
                                            .icon(IconName::SquareTerminal)
                                            .tooltip(tr!("page.activity"))
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                view.open_auxiliary(Page::Activity, cx)
                                            })),
                                        ("motion", "open-activity"),
                                        false,
                                    )),
                            )
                            .child(
                                div()
                                    .occlude()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .child(animated_button(
                                        Button::new("open-settings")
                                            .ghost()
                                            .small()
                                            .icon(IconName::Settings)
                                            .tooltip(tr!("page.settings"))
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                view.open_auxiliary(Page::Settings, cx)
                                            })),
                                        ("motion", "open-settings"),
                                        false,
                                    )),
                            ),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .p_6()
                        .when(self.step == DownloadStep::Progress, |content| {
                            content.px_4().pt_1().pb_4()
                        })
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(body),
                ),
        )
    }
}
