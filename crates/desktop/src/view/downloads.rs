use super::*;

impl Workspace {
    fn queue_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let busy = self.busy();
        let retry = self.queue.count(Filter::Failed) + self.queue.count(Filter::Cancelled);
        let mut filters = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .when(busy, |filters| {
                filters.child(Spinner::new().small().color(accent()))
            });
        for filter in Filter::ALL {
            filters = filters.child(animated_button(
                Button::new(filter.id())
                    .small()
                    .ghost()
                    .label(format!("{} {}", filter.label(), self.queue.count(filter)))
                    .when(filter == self.filter, |button| {
                        button.bg(cx.theme().muted).text_color(accent())
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.filter = filter;
                        cx.notify();
                    })),
                ("motion", filter.id()),
                false,
            ));
        }
        let indices = self
            .queue
            .filtered(self.filter, self.search.read(cx).value().as_ref());
        let mut panel = div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_4()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(filters)
                    .child(
                        div().flex_1().min_w(px(140.)).flex().justify_end().child(
                            div()
                                .w_full()
                                .max_w(px(220.))
                                .child(Input::new(&self.search).small().cleanable(true)),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .gap_1()
                            .child(animated_button(
                                Button::new("retry")
                                    .ghost()
                                    .small()
                                    .icon(IconName::RefreshCw)
                                    .tooltip(tr!("download.retry"))
                                    .accessibility_label(tr!("download.retry"))
                                    .disabled(busy || self.tool_busy || retry == 0)
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.start(true, window, cx)
                                    })),
                                ("motion", "retry"),
                                busy || self.tool_busy || retry == 0,
                            ))
                            .child(animated_button(
                                Button::new("clear")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Plus)
                                    .tooltip(tr!("download.new"))
                                    .accessibility_label(tr!("download.new"))
                                    .disabled(busy || self.tool_busy)
                                    .on_click(cx.listener(|view, _, _, cx| view.new_download(cx))),
                                ("motion", "clear"),
                                busy || self.tool_busy,
                            ))
                            .child(animated_button(
                                Button::new("cancel")
                                    .danger()
                                    .small()
                                    .icon(IconName::Close)
                                    .loading(self.cancelling)
                                    .tooltip(if self.cancelling {
                                        tr!("action.cancelling")
                                    } else {
                                        tr!("download.cancel")
                                    })
                                    .accessibility_label(tr!("download.cancel"))
                                    .disabled(!busy || self.cancelling)
                                    .on_click(cx.listener(|view, _, _, cx| view.cancel(cx))),
                                ("motion", "cancel"),
                                !busy || self.cancelling,
                            )),
                    ),
            )
            .child(
                div().px_4().pb_3().child(
                    Progress::new("overall")
                        .small()
                        .value(self.queue.percent())
                        .loading(busy && self.queue.rows.is_empty())
                        .accessibility_label(tr!("download.overall_progress"))
                        .color(accent()),
                ),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_y_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().muted)
                    .flex()
                    .gap_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().w_8().child("#"))
                    .child(div().flex_1().child(tr!("column.track_artist")))
                    .child(div().w(px(130.)).child(tr!("column.status")))
                    .child(div().w(px(130.)).child(tr!("column.progress")))
                    .child(div().w_12().child(tr!("column.actions"))),
            );
        if indices.is_empty() {
            panel = panel.child(
                div()
                    .flex_1()
                    .min_h(px(130.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .items_center()
                    .justify_center()
                    .child(if busy && self.queue.rows.is_empty() {
                        Spinner::new().large().color(accent()).into_any_element()
                    } else {
                        Icon::new(if self.queue.rows.is_empty() {
                            IconName::ArrowDown
                        } else {
                            IconName::Search
                        })
                        .large()
                        .text_color(cx.theme().muted_foreground)
                        .into_any_element()
                    })
                    .child(div().text_sm().child(if self.queue.rows.is_empty() {
                        if busy {
                            tr!("download.preparing")
                        } else {
                            tr!("download.empty")
                        }
                    } else {
                        tr!("search.no_results")
                    }))
                    .child(muted(
                        if self.queue.rows.is_empty() {
                            if busy {
                                tr!("download.preparing_hint")
                            } else {
                                tr!("download.empty_hint")
                            }
                        } else {
                            tr!("search.try_another")
                        },
                        cx,
                    )),
            );
        } else {
            panel = panel.child(
                uniform_list(
                    "tracks",
                    indices.len(),
                    cx.processor(move |view, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| view.track_row(indices[index], cx).into_any_element())
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0(),
            );
        }
        panel.child(
            div()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .flex()
                .justify_between()
                .child(muted(
                    tr!(
                        "download.visible_count",
                        p0 = self
                            .queue
                            .filtered(self.filter, self.search.read(cx).value().as_ref())
                            .len(),
                        p1 = self.jobs
                    ),
                    cx,
                ))
                .child(muted(
                    if self.queue.rows.is_empty() {
                        tr!("download.save_hint")
                    } else {
                        tr!(
                            "download.counts",
                            p0 = self.queue.count(Filter::Done),
                            p1 = self.queue.count(Filter::Failed),
                            p2 = self.queue.count(Filter::Cancelled),
                            p3 = self.queue.count(Filter::Active)
                        )
                    },
                    cx,
                )),
        )
    }
    fn track_row(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let row = &self.queue.rows[index];
        let path = row.path().map(std::path::Path::to_owned);
        let error = row.error().map(str::to_owned);
        let active = !model::terminal(&row.state) && !matches!(row.state, app::State::Pending);
        let color = if matches!(row.state, app::State::Failed { .. }) {
            tone_color(Tone::Error)
        } else if matches!(
            row.state,
            app::State::Complete { .. } | app::State::Skipped { .. }
        ) {
            tone_color(Tone::Success)
        } else if active {
            accent()
        } else {
            cx.theme().muted_foreground
        };
        let mut action = div().w_12();
        if let Some(path) = path {
            action = action.child(animated_button(
                Button::new(("reveal", index))
                    .ghost()
                    .small()
                    .icon(IconName::FolderOpen)
                    .tooltip(tr!("action.reveal"))
                    .on_click(move |_, _, cx| cx.reveal_path(&path)),
                ("motion", ("reveal", index)),
                false,
            ));
        }
        if let Some(error) = error {
            action = action.child(animated_button(
                Button::new(("error", index))
                    .ghost()
                    .small()
                    .icon(IconName::Copy)
                    .tooltip(tr!("action.copy_error"))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(error.clone()));
                        view.message(Tone::Info, tr!("error.copied"));
                        cx.notify();
                    })),
                ("motion", ("error", index)),
                false,
            ));
        }
        div()
            .w_full()
            .h(px(76.))
            .px_4()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .hover(|style| style.bg(cx.theme().muted))
            .child(
                div()
                    .w_8()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{:02}", index + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(row.item.title.clone()),
                    )
                    .child(
                        muted(
                            format!(
                                "{}{}",
                                row.item.artist,
                                row.assets
                                    .as_ref()
                                    .map(|assets| format!(" · {}", assets.label()))
                                    .unwrap_or_default()
                            ),
                            cx,
                        )
                        .truncate(),
                    ),
            )
            .child(
                div()
                    .w(px(130.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(active, |status| {
                        status.child(Spinner::new().small().color(color))
                    })
                    .child(badge(model::state_label(&row.state), color, cx)),
            )
            .child(
                div()
                    .w(px(130.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        Progress::new(("progress", index))
                            .small()
                            .value(row.percent())
                            .loading(
                                active
                                    && (!matches!(row.state, app::State::Downloading)
                                        || row.total == 0),
                            )
                            .accessibility_label(tr!(
                                "download.track_progress",
                                p0 = row.item.title
                            ))
                            .color(color),
                    )
                    .child(muted(
                        if row.total == 0 {
                            "—".into()
                        } else {
                            format!(
                                "{}/{} · {:.1} MiB",
                                row.completed,
                                row.total,
                                row.bytes as f64 / 1_048_576.
                            )
                        },
                        cx,
                    )),
            )
            .child(action)
    }
    pub(super) fn progress_panel(&mut self, cx: &mut Context<Self>) -> Div {
        self.queue_panel(cx)
    }
}
