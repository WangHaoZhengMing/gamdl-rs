use super::*;

impl Workspace {
    pub fn credentials(&self) -> Result<app::Credentials, String> {
        match self.auth_method {
            AuthMethod::CookieFile => Ok(app::Credentials::CookieFile),
            AuthMethod::Browser => self
                .browser_session
                .as_ref()
                .ok_or_else(|| tr!("login.required"))?
                .credentials(),
        }
    }

    pub fn set_auth_method(&mut self, method: AuthMethod, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy || method == self.auth_method {
            return;
        }
        self.auth_method = method;
        self.account = None;
        self.selection = Selection::default();
        if self.step == DownloadStep::Selection {
            self.step = DownloadStep::Input;
        }
        self.autosave(cx);
        if method == AuthMethod::Browser && self.browser_session.is_none() {
            self.restore_login(cx);
        }
        cx.notify();
    }

    pub(super) fn restore_login(&mut self, cx: &mut Context<Self>) {
        self.tool_busy = true;
        self.active_tool = Some(Tool::Restore);
        let job = self.runtime.spawn_blocking(login::load);
        self.login_task = Some(cx.spawn(async move |view, cx| {
            let result = job.await;
            let _ = view.update(cx, |view, cx| {
                view.tool_busy = false;
                view.active_tool = None;
                match result {
                    Ok(Ok(session)) => view.browser_session = session,
                    Ok(Err(error)) => view.message(Tone::Error, error),
                    Err(_) => view.message(Tone::Error, tr!("login.restore_failed")),
                }
                cx.notify();
            });
        }));
    }

    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy || self.choosing.is_some() {
            return;
        }
        self.tool_busy = true;
        self.active_tool = Some(Tool::Login);
        let cancel = app::CancellationToken::new();
        self.login_cancel = Some(cancel.clone());
        let runtime = self.runtime.clone();
        let job = runtime.spawn(login::sign_in(cancel));
        self.login_task = Some(cx.spawn(async move |view, cx| {
            let result = match job.await {
                Ok(result) => result,
                Err(_) => Err(tr!("login.interrupted")),
            };
            let session = match result {
                Ok(Some(session)) => session,
                result => {
                    let _ = view.update(cx, |view, cx| {
                        view.tool_busy = false;
                        view.active_tool = None;
                        view.login_cancel = None;
                        match result {
                            Ok(_) => view.message(Tone::Info, tr!("login.cancelled")),
                            Err(error) => view.message(Tone::Error, error),
                        }
                        cx.notify();
                    });
                    return;
                }
            };
            if view
                .update(cx, |view, cx| {
                    view.login_cancel = None;
                    view.active_tool = Some(Tool::VerifyLogin);
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            let checked = session.clone();
            let result = runtime
                .spawn(async move {
                    let credentials = checked.credentials()?;
                    app::check_auth_with_credentials(&app::Config::default(), &credentials)
                        .await
                        .map_err(|error| {
                            tr!(
                                "login.validation_failed",
                                error = crate::i18n::app_error(&error)
                            )
                        })
                })
                .await;
            let account = match result {
                Ok(Ok(account)) => account,
                error => {
                    let _ = view.update(cx, |view, cx| {
                        view.tool_busy = false;
                        view.active_tool = None;
                        let message = match error {
                            Ok(Err(error)) => error,
                            _ => tr!("login.validation_interrupted"),
                        };
                        view.message(Tone::Error, message);
                        cx.notify();
                    });
                    return;
                }
            };
            let saved = session.clone();
            let persistence = runtime.spawn_blocking(move || login::save(&saved)).await;
            let _ = view.update(cx, |view, cx| {
                view.tool_busy = false;
                view.active_tool = None;
                view.auth_method = AuthMethod::Browser;
                view.browser_session = Some(session);
                view.selection = Selection::default();
                if view.step == DownloadStep::Selection {
                    view.step = DownloadStep::Input;
                }
                view.message(
                    if account.active_subscription {
                        Tone::Success
                    } else {
                        Tone::Info
                    },
                    if account.active_subscription {
                        tr!("login.connected", p0 = account.storefront)
                    } else {
                        tr!("login.no_subscription")
                    },
                );
                view.account = Some(account);
                match persistence {
                    Ok(Ok(())) => (),
                    Ok(Err(error)) => view.message(Tone::Error, error),
                    Err(_) => view.message(Tone::Error, tr!("login.persistence_failed")),
                }
                view.autosave(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.tool_busy {
            return;
        }
        self.tool_busy = true;
        self.active_tool = Some(Tool::Logout);
        let job = self.runtime.spawn_blocking(login::forget);
        self.login_task = Some(cx.spawn(async move |view, cx| {
            let result = job.await;
            let _ = view.update(cx, |view, cx| {
                view.tool_busy = false;
                view.active_tool = None;
                match result {
                    Ok(Ok(())) => {
                        view.browser_session = None;
                        view.account = None;
                        view.selection = Selection::default();
                        if view.step == DownloadStep::Selection {
                            view.step = DownloadStep::Input;
                        }
                        view.message(Tone::Info, tr!("login.signed_out"));
                    }
                    Ok(Err(error)) => view.message(Tone::Error, error),
                    Err(_) => view.message(Tone::Error, tr!("login.sign_out_failed")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
