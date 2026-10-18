use super::*;

impl Workspace {
    pub(super) fn activity_view(&self, cx: &mut Context<Self>) -> Div {
        let mut content = card(cx)
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .p_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(heading(
                        tr!("activity.session"),
                        tr!("activity.description"),
                        cx,
                    ))
                    .child(animated_button(
                        Button::new("clear-activity")
                            .ghost()
                            .small()
                            .label(tr!("activity.clear"))
                            .disabled(self.activity.is_empty())
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.activity.clear();
                                cx.notify();
                            })),
                        ("motion", "clear-activity"),
                        false,
                    )),
            );
        let mut entries = div().id("activities").flex_1().overflow_y_scroll();
        for (index, activity) in self.activity.iter().enumerate().rev() {
            entries = entries.child(
                div()
                    .p_4()
                    .flex()
                    .gap_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .w(px(70.))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(activity.time.clone()),
                    )
                    .child(
                        div()
                            .size_2()
                            .mt_1()
                            .rounded_full()
                            .bg(tone_color(activity.tone)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .child(activity.text.clone()),
                    )
                    .child(
                        Button::new(("copy-activity", index))
                            .ghost()
                            .small()
                            .icon(IconName::Copy)
                            .tooltip(tr!("activity.copy"))
                            .on_click({
                                let text = activity.text.clone();
                                move |_, window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                                    window.push_notification(
                                        Notification::info(tr!("activity.copied"))
                                            .id::<Workspace>()
                                            .placement(Anchor::BottomRight),
                                        cx,
                                    );
                                }
                            }),
                    ),
            );
        }
        if self.activity.is_empty() {
            entries = entries.child(div().p_8().child(muted(tr!("activity.empty"), cx)));
        }
        content = content.child(entries);
        content
    }
}
