use super::*;
use gpui_kit::component::checkbox::Checkbox;

impl Workspace {
    pub(super) fn flow_header(&self, cx: &mut Context<Self>) -> Div {
        if self.step != DownloadStep::Selection {
            return div();
        }
        div()
            .flex()
            .items_center()
            .gap_3()
            .when(self.step == DownloadStep::Selection, |header| {
                header.child(
                    div()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(animated_button(
                            Button::new("back-to-input")
                                .ghost()
                                .small()
                                .label("←")
                                .tooltip(tr!("action.back"))
                                .disabled(self.busy() || self.tool_busy)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    if view.busy() || view.tool_busy {
                                        return;
                                    }
                                    view.step = DownloadStep::Input;
                                    view.notice = None;
                                    cx.notify();
                                })),
                            ("motion", "back-to-input"),
                            false,
                        )),
                )
            })
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr!("flow.selection")),
            )
    }

    fn input_step(&self, cx: &mut Context<Self>) -> Div {
        let count = model::urls(self.urls.read(cx).value().as_ref()).len();
        let resolving = self.active_tool == Some(Tool::Resolve);
        let logging_in = matches!(self.active_tool, Some(Tool::Login | Tool::VerifyLogin));
        let needs_login = self.auth_method == crate::preferences::AuthMethod::Browser
            && self
                .browser_session
                .as_ref()
                .is_none_or(|session| session.credentials().is_err());
        let cancelling = self
            .resolve_cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled());
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .px_6()
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(tr!("input.links")),
                            )
                            .child(animated_button(
                                Button::new("paste-links")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Copy)
                                    .label(tr!("input.paste"))
                                    .disabled(self.tool_busy)
                                    .on_click(
                                        cx.listener(|view, _, window, cx| view.paste(window, cx)),
                                    ),
                                ("motion", "paste-links"),
                                self.tool_busy,
                            )),
                    )
                    .child(
                        Textarea::new(&self.urls)
                            .h(px(192.))
                            .disabled(self.tool_busy),
                    )
                    .child(muted(tr!("input.hint"), cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pt_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(resolving, |row| {
                                        row.child(Spinner::new().small().color(accent()))
                                    })
                                    .child(muted(
                                        if resolving {
                                            if cancelling {
                                                tr!("resolve.stopping")
                                            } else {
                                                tr!("resolve.waiting")
                                            }
                                        } else {
                                            tr!("input.source_count", p0 = count)
                                        },
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .when(self.login_cancel.is_some(), |content| {
                                        content.child(
                                            Button::new("cancel-login-input")
                                                .ghost()
                                                .label(tr!("login.cancel"))
                                                .disabled(
                                                    self.login_cancel.as_ref().is_some_and(
                                                        |cancel| cancel.is_cancelled(),
                                                    ),
                                                )
                                                .on_click(cx.listener(|view, _, _, cx| {
                                                    if let Some(cancel) = &view.login_cancel {
                                                        cancel.cancel();
                                                    }
                                                    cx.notify();
                                                })),
                                        )
                                    })
                                    .when(self.resolve_cancel.is_some(), |content| {
                                        content.child(animated_button(
                                            Button::new("cancel-parse")
                                                .ghost()
                                                .label(if cancelling {
                                                    tr!("action.cancelling")
                                                } else {
                                                    tr!("resolve.cancel")
                                                })
                                                .loading(cancelling)
                                                .disabled(cancelling)
                                                .on_click(cx.listener(|view, _, _, cx| {
                                                    if let Some(cancel) = &view.resolve_cancel {
                                                        cancel.cancel();
                                                    }
                                                    cx.notify();
                                                })),
                                            ("motion", "cancel-parse"),
                                            false,
                                        ))
                                    })
                                    .child(animated_button(
                                        Button::new("parse-links")
                                            .primary()
                                            .loading(resolving || logging_in)
                                            .label(if resolving {
                                                tr!("resolve.loading")
                                            } else if logging_in {
                                                tr!("login.loading")
                                            } else if needs_login {
                                                tr!("login.sign_in")
                                            } else {
                                                tr!("resolve.action")
                                            })
                                            .disabled(
                                                self.tool_busy || (!needs_login && count == 0),
                                            )
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                if view.auth_method
                                                    == crate::preferences::AuthMethod::Browser
                                                    && view.browser_session.as_ref().is_none_or(
                                                        |session| session.credentials().is_err(),
                                                    )
                                                {
                                                    view.sign_in(cx);
                                                } else {
                                                    view.run(Tool::Resolve, cx);
                                                }
                                            })),
                                        ("motion", "parse-links"),
                                        self.tool_busy || count == 0,
                                    )),
                            ),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .pt_3()
                    .child(muted(tr!("flow.next_hint"), cx)),
            )
    }

    fn selection_row(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let track = &self.selection.tracks[index];
        let checked = self.selection.selected.contains(&track.id);
        div()
            .id(("selection-row", index))
            .w_full()
            .h(px(64.))
            .px_3()
            .flex()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(checked, |row| row.bg(cx.theme().muted))
            .hover(|row| row.bg(cx.theme().muted))
            .cursor_pointer()
            .on_click(cx.listener(move |view, _, _, cx| {
                let selected = view
                    .selection
                    .tracks
                    .get(index)
                    .is_some_and(|track| view.selection.selected.contains(&track.id));
                view.selection.toggle(index, !selected);
                cx.notify();
            }))
            .child(
                div().w(px(18.)).flex_shrink_0().child(
                    Checkbox::new(("select-track", index))
                        .checked(checked)
                        .on_click(cx.listener(move |view, checked, _, cx| {
                            cx.stop_propagation();
                            view.selection.toggle(index, *checked);
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .w_8()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{:02}", index + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(track.title.clone()),
                    )
                    .child(muted(track.artist.clone(), cx).truncate()),
            )
            .child(
                muted(track.album.clone(), cx)
                    .w(px(230.))
                    .flex_shrink_0()
                    .truncate(),
            )
            .child(muted("AAC · 256 kbps", cx).w(px(115.)).flex_shrink_0())
    }

    fn selection_step(&mut self, cx: &mut Context<Self>) -> Div {
        let count = self.selection.selected.len();
        let total = self.selection.tracks.len();
        let indices = self
            .selection
            .filtered(self.selection_search.read(cx).value().as_ref());
        let mut content = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .py_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                Checkbox::new("select-all-tracks")
                                    .label(tr!("selection.all"))
                                    .checked(total > 0 && count == total)
                                    .on_click(cx.listener(|view, checked, _, cx| {
                                        view.selection.select_all(*checked);
                                        cx.notify();
                                    })),
                            )
                            .child(muted(tr!("selection.count", p0 = count, p1 = total), cx))
                            .child(animated_button(
                                Button::new("deselect-tracks")
                                    .ghost()
                                    .small()
                                    .label(tr!("selection.clear"))
                                    .disabled(count == 0)
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.selection.select_all(false);
                                        cx.notify();
                                    })),
                                ("motion", "deselect-tracks"),
                                count == 0,
                            )),
                    )
                    .child(
                        div()
                            .w(px(240.))
                            .child(Input::new(&self.selection_search).small().cleanable(true)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_3()
                    .py_2()
                    .border_y_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().w(px(18.)))
                    .child(div().w_8().flex_shrink_0().child("#"))
                    .child(div().flex_1().child(tr!("column.track_artist")))
                    .child(div().w(px(230.)).flex_shrink_0().child(tr!("column.album")))
                    .child(
                        div()
                            .w(px(115.))
                            .flex_shrink_0()
                            .child(tr!("column.format")),
                    ),
            );
        if indices.is_empty() {
            content = content.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(muted(tr!("selection.no_results"), cx)),
            );
        } else {
            content = content.child(
                uniform_list(
                    "parsed-tracks",
                    indices.len(),
                    cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| view.selection_row(indices[index], cx).into_any_element())
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0(),
            );
        }
        content.child(
            div()
                .pt_4()
                .border_t_1()
                .border_color(cx.theme().border)
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div().flex().items_center().gap_2().child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .min_w_0()
                            .child(muted("AAC · 256 kbps", cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        muted(
                                            tr!(
                                                "selection.output",
                                                p0 = self.output.read(cx).value()
                                            ),
                                            cx,
                                        )
                                        .max_w(px(360.))
                                        .truncate(),
                                    )
                                    .child(animated_button(
                                        Button::new("selection-output")
                                            .ghost()
                                            .small()
                                            .label(tr!("action.change"))
                                            .loading(self.choosing == Some(Field::Output))
                                            .disabled(self.choosing.is_some())
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                view.choose(Field::Output, cx)
                                            })),
                                        ("motion", "selection-output"),
                                        false,
                                    )),
                            ),
                    ),
                )
                .child(animated_button(
                    Button::new("download-selection")
                        .primary()
                        .label(tr!("selection.download", p0 = count))
                        .disabled(count == 0)
                        .on_click(cx.listener(|view, _, window, cx| view.start(false, window, cx))),
                    ("motion", "download-selection"),
                    count == 0,
                )),
        )
    }

    pub(super) fn downloads(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match self.step {
            DownloadStep::Input => self.input_step(cx),
            DownloadStep::Selection => self.selection_step(cx),
            DownloadStep::Progress => self.progress_panel(cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_5()
            .child(body)
    }
}
