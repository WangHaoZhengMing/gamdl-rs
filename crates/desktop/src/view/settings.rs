use super::*;
use crate::preferences::AuthMethod;
use crate::workspace::SaveStatus;
use gpui_kit::component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};

impl Workspace {
    fn login_setting(&self, cx: &mut Context<Self>) -> SettingItem {
        let sign_in = std::rc::Rc::new(cx.listener(|view, _, _, cx| view.sign_in(cx)));
        let sign_out = std::rc::Rc::new(cx.listener(|view, _, _, cx| view.sign_out(cx)));
        let cancel_login = std::rc::Rc::new(cx.listener(|view, _, _, cx| {
            if let Some(cancel) = &view.login_cancel {
                cancel.cancel();
            }
            cx.notify();
        }));
        let logging_in = matches!(self.active_tool, Some(Tool::Login | Tool::VerifyLogin));
        let signing_out = self.active_tool == Some(Tool::Logout);
        let has_session = self.browser_session.is_some();
        let cancellable = self.login_cancel.is_some();
        let cancelling = self
            .login_cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled());
        let description = match self.active_tool {
            Some(Tool::Restore) => tr!("login.restoring"),
            Some(Tool::Login) => tr!("login.instructions"),
            Some(Tool::VerifyLogin) => tr!("login.verifying"),
            _ => match &self.account {
                Some(account) => tr!(
                    "account.status",
                    p0 = account.storefront,
                    p1 = if account.active_subscription {
                        tr!("account.subscription_active")
                    } else {
                        tr!("account.subscription_inactive")
                    }
                ),
                None if has_session => tr!("login.restored"),
                None => tr!("login.description"),
            },
        };
        SettingItem::new(
            tr!("login.title"),
            SettingField::render(move |options, _, _| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("apple-sign-in")
                            .outline()
                            .label(if logging_in {
                                tr!("login.loading")
                            } else if has_session {
                                tr!("login.sign_in_again")
                            } else {
                                tr!("login.sign_in")
                            })
                            .loading(logging_in)
                            .disabled(options.is_disabled())
                            .on_click({
                                let callback = sign_in.clone();
                                move |event, window, cx| callback(event, window, cx)
                            }),
                    )
                    .when(cancellable, |row| {
                        row.child(
                            Button::new("cancel-sign-in")
                                .ghost()
                                .label(if cancelling {
                                    tr!("action.cancelling")
                                } else {
                                    tr!("login.cancel")
                                })
                                .disabled(cancelling)
                                .on_click({
                                    let callback = cancel_login.clone();
                                    move |event, window, cx| callback(event, window, cx)
                                }),
                        )
                    })
                    .when(!cancellable, |row| {
                        row.child(
                            Button::new("apple-sign-out")
                                .ghost()
                                .label(tr!("login.sign_out"))
                                .loading(signing_out)
                                .disabled(options.is_disabled())
                                .on_click({
                                    let callback = sign_out.clone();
                                    move |event, window, cx| callback(event, window, cx)
                                }),
                        )
                    })
            }),
        )
        .description(description)
        .disabled(self.busy() || self.tool_busy)
    }

    fn path_setting(
        &self,
        id: &'static str,
        title: String,
        hint: String,
        input: &Entity<gpui_kit::component::input::InputState>,
        field: Field,
        cx: &mut Context<Self>,
    ) -> SettingItem {
        let input = input.clone();
        let choosing = self.choosing == Some(field);
        let picker_busy = self.choosing.is_some();
        let choose = std::rc::Rc::new(cx.listener(move |view, _, _, cx| view.choose(field, cx)));
        SettingItem::new(
            title,
            SettingField::render(move |options, _, _| {
                div()
                    .w(px(330.))
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&input).disabled(options.is_disabled())),
                    )
                    .child(animated_button(
                        Button::new(id)
                            .outline()
                            .label(tr!("action.browse"))
                            .loading(choosing)
                            .disabled(options.is_disabled() || picker_busy)
                            .on_click({
                                let callback = choose.clone();
                                move |event, window, cx| callback(event, window, cx)
                            }),
                        ("motion", id),
                        options.is_disabled(),
                    ))
            }),
        )
        .description(hint)
        .disabled(self.busy() || self.tool_busy)
    }

    fn switch_setting(
        &self,
        title: String,
        hint: String,
        get: fn(&Self) -> bool,
        set: fn(&mut Self, bool),
        cx: &mut Context<Self>,
    ) -> SettingItem {
        let read = cx.entity();
        let write = read.clone();
        SettingItem::new(
            title,
            SettingField::switch(
                move |cx| get(read.read(cx)),
                move |value, cx| {
                    write.update(cx, |view, cx| {
                        if !view.busy() && !view.tool_busy {
                            set(view, value);
                            view.autosave(cx);
                            cx.notify();
                        }
                    })
                },
            ),
        )
        .description(hint)
        .disabled(self.busy() || self.tool_busy)
    }

    fn tool_setting(
        &self,
        title: String,
        description: String,
        id: &'static str,
        label: String,
        tool: Tool,
        cx: &mut Context<Self>,
    ) -> SettingItem {
        let click = std::rc::Rc::new(cx.listener(move |view, _, _, cx| view.run(tool, cx)));
        let running = self.active_tool == Some(tool);
        SettingItem::new(
            title,
            SettingField::render(move |options, _, _| {
                animated_button(
                    Button::new(id)
                        .outline()
                        .label(if running {
                            tr!("tools.checking")
                        } else {
                            label.clone()
                        })
                        .loading(running)
                        .disabled(options.is_disabled())
                        .on_click({
                            let callback = click.clone();
                            move |event, window, cx| callback(event, window, cx)
                        }),
                    ("motion", id),
                    options.is_disabled(),
                )
            }),
        )
        .description(description)
        .disabled(self.busy() || self.tool_busy)
    }

    pub(super) fn settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy() || self.tool_busy;
        let read_jobs = cx.entity();
        let write_jobs = read_jobs.clone();
        let jobs = SettingItem::new(
            tr!("settings.jobs"),
            SettingField::number_input(
                NumberFieldOptions {
                    min: 1.,
                    max: 16.,
                    step: 1.,
                },
                move |cx| read_jobs.read(cx).jobs as f64,
                move |value, cx| {
                    write_jobs.update(cx, |view, cx| {
                        if !view.busy() && !view.tool_busy {
                            view.jobs = value.round().clamp(1., 16.) as usize;
                            view.autosave(cx);
                            cx.notify();
                        }
                    })
                },
            ),
        )
        .description(tr!("settings.jobs_hint"))
        .disabled(busy);
        let language = self.language.clone();
        let language = SettingItem::new(
            tr!("settings.metadata_language"),
            SettingField::render(move |options, _, _| {
                div()
                    .w(px(200.))
                    .child(Input::new(&language).disabled(options.is_disabled()))
            }),
        )
        .description(tr!("settings.metadata_language_hint"))
        .disabled(busy);
        let read_format = cx.entity();
        let write_format = read_format.clone();
        let format = SettingItem::new(
            tr!("settings.lyrics_format"),
            SettingField::dropdown(
                vec![
                    ("lrc".into(), "LRC / Enhanced LRC".into()),
                    ("ttml".into(), tr!("settings.original_ttml").into()),
                ],
                move |cx| match read_format.read(cx).lyrics_format {
                    app::LyricsFormat::Lrc => "lrc".into(),
                    app::LyricsFormat::Ttml => "ttml".into(),
                },
                move |value, cx| {
                    write_format.update(cx, |view, cx| {
                        if !view.busy() && !view.tool_busy {
                            view.lyrics_format = if value == "ttml" {
                                app::LyricsFormat::Ttml
                            } else {
                                app::LyricsFormat::Lrc
                            };
                            view.autosave(cx);
                            cx.notify();
                        }
                    })
                },
            ),
        )
        .description(tr!("settings.lyrics_format_hint"))
        .disabled(busy || !self.save_lyrics);
        let theme_click =
            std::rc::Rc::new(cx.listener(|view, _, window, cx| view.toggle_theme(window, cx)));
        let dark = self.dark;
        let theme = SettingItem::new(
            tr!("settings.theme"),
            SettingField::render(move |_, _, _| {
                animated_button(
                    Button::new("theme-setting")
                        .outline()
                        .label(if dark {
                            tr!("settings.light_theme")
                        } else {
                            tr!("settings.dark_theme")
                        })
                        .on_click({
                            let callback = theme_click.clone();
                            move |event, window, cx| callback(event, window, cx)
                        }),
                    ("motion", "theme-setting"),
                    false,
                )
            }),
        )
        .description(tr!("settings.theme_hint"));
        let read_locale = cx.entity();
        let write_locale = read_locale.clone();
        let ui_language = SettingItem::new(
            tr!("settings.ui_language"),
            SettingField::dropdown(
                crate::i18n::UiLanguage::ALL
                    .into_iter()
                    .map(|language| (language.code().into(), language.native_name().into()))
                    .collect(),
                move |cx| read_locale.read(cx).ui_language.code().into(),
                move |value, cx| {
                    write_locale.update(cx, |view, cx| {
                        view.set_ui_language(crate::i18n::UiLanguage::from_code(&value), cx);
                    })
                },
            ),
        )
        .description(tr!("settings.ui_language_hint"));
        let account = match &self.account {
            Some(account) if account.active_subscription => {
                tr!("account.active_region", p0 = account.storefront)
            }
            Some(_) => tr!("account.subscription_inactive"),
            None => tr!("account.unchecked"),
        };
        let read_auth = cx.entity();
        let write_auth = read_auth.clone();
        let auth_method = SettingItem::new(
            tr!("settings.auth_method"),
            SettingField::dropdown(
                vec![
                    ("browser".into(), tr!("settings.browser_auth").into()),
                    ("cookie_file".into(), tr!("settings.cookie_auth").into()),
                ],
                move |cx| match read_auth.read(cx).auth_method {
                    AuthMethod::Browser => "browser".into(),
                    AuthMethod::CookieFile => "cookie_file".into(),
                },
                move |value, cx| {
                    write_auth.update(cx, |view, cx| {
                        view.set_auth_method(
                            if value == "cookie_file" {
                                AuthMethod::CookieFile
                            } else {
                                AuthMethod::Browser
                            },
                            cx,
                        )
                    })
                },
            ),
        )
        .description(tr!("settings.auth_hint"))
        .disabled(busy);
        let mut apple_group = SettingGroup::new().title("Apple Music").item(auth_method);
        if self.auth_method == AuthMethod::Browser {
            apple_group = apple_group.item(self.login_setting(cx));
        } else {
            apple_group = apple_group.item(self.path_setting(
                "pick-cookies",
                tr!("settings.cookies"),
                tr!("settings.cookies_hint"),
                &self.cookies,
                Field::Cookies,
                cx,
            ));
        }
        apple_group = apple_group.item(self.tool_setting(
            tr!("settings.subscription"),
            account,
            "check-account",
            tr!("settings.check_account"),
            Tool::Auth,
            cx,
        ));
        let pages =
            vec![
                SettingPage::new(tr!("settings.account_output"))
                    .group(apple_group)
                    .group(
                        SettingGroup::new()
                            .title(tr!("settings.saving_files"))
                            .item(self.path_setting(
                                "pick-output",
                                tr!("settings.output"),
                                tr!("settings.output_hint"),
                                &self.output,
                                Field::Output,
                                cx,
                            )),
                    ),
                SettingPage::new(tr!("settings.download_preferences"))
                    .group(
                        SettingGroup::new()
                            .title(tr!("settings.download_tasks"))
                            .items([jobs, language]),
                    )
                    .group(
                        SettingGroup::new()
                            .title(tr!("settings.extras"))
                            .description(tr!("settings.extras_hint"))
                            .item(self.switch_setting(
                                tr!("settings.save_cover"),
                                tr!("settings.save_cover_hint"),
                                |view| view.save_cover,
                                |view, value| view.save_cover = value,
                                cx,
                            ))
                            .item(self.switch_setting(
                                tr!("settings.save_lyrics"),
                                tr!("settings.save_lyrics_hint"),
                                |view| view.save_lyrics,
                                |view, value| view.save_lyrics = value,
                                cx,
                            ))
                            .item(format),
                    ),
                SettingPage::new(tr!("settings.media_tools"))
                    .group(
                        SettingGroup::new()
                            .title("FFmpeg")
                            .item(self.path_setting(
                                "pick-ffmpeg",
                                tr!("settings.ffmpeg_path"),
                                tr!("settings.ffmpeg_path_hint"),
                                &self.ffmpeg,
                                Field::Ffmpeg,
                                cx,
                            ))
                            .item(
                                self.tool_setting(
                                    tr!("settings.tool_status"),
                                    self.tool_version
                                        .clone()
                                        .unwrap_or_else(|| tr!("settings.tool_unchecked")),
                                    "doctor",
                                    tr!("settings.check_ffmpeg"),
                                    Tool::Doctor,
                                    cx,
                                ),
                            ),
                    )
                    .group(SettingGroup::new().title(tr!("settings.device")).item(
                        self.path_setting(
                            "pick-device",
                            tr!("settings.device_override"),
                            tr!("settings.device_hint"),
                            &self.device,
                            Field::Device,
                            cx,
                        ),
                    )),
                SettingPage::new(tr!("settings.appearance")).group(
                    SettingGroup::new()
                        .title(tr!("settings.interface"))
                        .items([ui_language, theme]),
                ),
            ];
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div().flex_1().min_h_0().child(
                    // The Kit caches translated search/dropdown state. Rebuild
                    // that presentation on locale changes, staying on Appearance.
                    Settings::new(SharedString::from(format!(
                        "preferences-{}",
                        self.ui_language.code()
                    )))
                    .when(self.settings_locale_changed, |settings| {
                        settings.default_selected_index(gpui_kit::component::setting::SelectIndex {
                            page_ix: 3,
                            group_ix: None,
                        })
                    })
                    .sidebar_width(px(200.))
                    .pages(pages),
                ),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(matches!(self.save_status, SaveStatus::Saving), |status| {
                        status.child(Spinner::new().small().color(accent()))
                    })
                    .child(muted(
                        match self.save_status {
                            SaveStatus::Unchanged => tr!("settings.autosave_hint"),
                            SaveStatus::Saving => tr!("settings.saving"),
                            SaveStatus::Saved => tr!("settings.saved"),
                            SaveStatus::Failed => tr!("settings.save_failed"),
                        },
                        cx,
                    )),
            )
    }
}
