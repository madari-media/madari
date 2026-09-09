use super::*;
use madari_native::trakt::{Client, Credentials, DeviceCode, Poll};

impl Ui {
    pub(in crate::app) fn trakt_preferences(self: &Rc<Self>) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title("Trakt")
            .description(
                "Import your lists and history. Playback automatically reports watching, pause and watched progress to Trakt.",
            )
            .build();
        let row = adw::ActionRow::builder()
            .title("Trakt account")
            .subtitle("Checking connection…")
            .use_markup(false)
            .build();
        let connect = self.button("Connect", |ui| ui.trakt_connect());
        let status = row.downgrade();
        let sync = self.button("Sync now", move |ui| ui.trakt_sync(status.clone()));
        let status = row.downgrade();
        let disconnect=self.button("Disconnect",move|ui| {
            let status=status.clone();
            ui.dialog("Disconnect Trakt?","Remove this profile’s connection and cached Trakt lists. Local playback progress and saved titles will stay.",&form(),"Disconnect",move|ui| {
                let profiles=ui.profiles.clone();let session=ui.session.borrow().as_ref().unwrap().clone();
                let status=status.clone();
                ui.run(async move{profiles.disconnect_trakt(session).await},move|ui,revoked| {
                    if let Some(row)=status.upgrade(){row.set_subtitle("Not connected");}
                    ui.toast.add_toast(adw::Toast::new(if revoked {"Trakt disconnected"}else{"Disconnected locally. Remove Madari from Trakt’s connected apps to revoke access when online."}));
                    ui.refresh();
                });
            });
        });
        // Keep actions in their own row so narrow settings never overflow.
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        for button in [&connect, &sync, &disconnect] {
            button.set_sensitive(false);
            actions.append(button);
        }
        group.add(&row);
        group.add(&actions);
        let profiles = self.profiles.clone();
        let session = self.session.borrow().as_ref().unwrap().clone();
        let task = self.runtime.spawn(async move {
            Ok::<_, madari_model::Error>((
                profiles.trakt_connected(session.clone()).await?,
                profiles.trakt_data(session).await?,
            ))
        });
        let row = row.downgrade();
        glib::spawn_future_local(async move {
            let result = task.await;
            let Some(row) = row.upgrade() else {
                return;
            };
            match result {
                Ok(Ok((connected, data))) => {
                    connect.set_visible(!connected);
                    connect.set_sensitive(true);
                    sync.set_visible(connected);
                    sync.set_sensitive(true);
                    disconnect.set_visible(connected);
                    disconnect.set_sensitive(true);
                    row.set_subtitle(
                        &data
                            .map(|d| {
                                format!("Connected as {} · {} lists", d.username, d.lists.len())
                            })
                            .unwrap_or_else(|| {
                                if connected {
                                    "Connected · ready to sync".into()
                                } else {
                                    "Not connected".into()
                                }
                            }),
                    );
                }
                _ => {
                    row.set_subtitle("Could not read Trakt connection");
                    connect.set_sensitive(true);
                }
            }
        });
        group
    }
    fn trakt_connect(self: &Rc<Self>) {
        if let Some(credentials) = Credentials::environment() {
            self.trakt_device_login(credentials);
            return;
        }
        let fields = form();
        let id = entry("Application client ID", false);
        let secret = entry("Application client secret", true);
        secret.set_max_length(0);
        secret.set_input_purpose(gtk::InputPurpose::Password);
        let redirect = entry("Redirect URI from your Trakt app", false);
        redirect.set_text("urn:ietf:wg:oauth:2.0:oob");
        fields.append(&gtk::LinkButton::with_label(
            "https://trakt.tv/oauth/applications",
            "Create a Trakt application ↗",
        ));
        fields.append(&id);
        fields.append(&secret);
        fields.append(&redirect);
        self.dialog("Set up Trakt","Enter Madari’s Trakt application credentials once. Next, you’ll approve a device code in your browser.",&fields,"Continue",move|ui| {
            let credentials=Credentials{client_id:id.text().trim().into(),client_secret:secret.text().trim().into(),redirect_uri:redirect.text().trim().into()};
            secret.set_text("");
            ui.trakt_device_login(credentials);
        });
    }
    fn trakt_device_login(self: &Rc<Self>, credentials: Credentials) {
        self.run(
            async move {
                let client = Client::new(credentials)?;
                let code = client.device_code().await?;
                Ok((client, code))
            },
            |ui, (client, code)| ui.trakt_code_dialog(client, code),
        );
    }
    fn trakt_code_dialog(self: &Rc<Self>, client: Client, code: DeviceCode) {
        let fields = form();
        let text = gtk::Label::builder()
            .label(&code.user_code)
            .selectable(true)
            .halign(gtk::Align::Center)
            .build();
        text.add_css_class("title-1");
        fields.append(&text);
        let copy = gtk::Button::with_label("Copy code");
        let value = code.user_code.clone();
        copy.connect_clicked(move |button| button.clipboard().set_text(&value));
        fields.append(&copy);
        fields.append(&gtk::LinkButton::with_label(
            &code.verification_url,
            "Open Trakt to approve ↗",
        ));
        let waiting = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        waiting.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.start();
        waiting.append(&spinner);
        waiting.append(&label("Waiting for approval…", "dim-label"));
        fields.append(&waiting);
        let dialog = adw::AlertDialog::builder()
            .heading("Connect to Trakt")
            .body(format!(
                "Enter this code on Trakt. It expires in {} minutes.",
                code.expires_in.div_ceil(60)
            ))
            .extra_child(&fields)
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.set_close_response("cancel");
        let session = self.session.borrow().as_ref().unwrap().clone();
        let retry_credentials = client.credentials();
        let task = self.runtime.spawn(async move {
            let deadline = tokio::time::Instant::from_std(code.issued_at)
                + std::time::Duration::from_secs(code.expires_in);
            let mut interval = code.interval;
            let poll = async {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                    match client.poll(&code).await? {
                        Poll::Pending => {}
                        Poll::SlowDown(seconds) => {
                            interval = interval.saturating_add(5).max(seconds);
                        }
                        Poll::Authorized(tokens) => return Ok((client, tokens)),
                    }
                }
            };
            tokio::time::timeout_at(deadline, poll)
                .await
                .unwrap_or_else(|_| {
                    Err(madari_model::Error::new(
                        madari_model::ErrorCode::Timeout,
                        "The Trakt code expired. Connect again for a new code.",
                    ))
                })
        });
        let abort = task.abort_handle();
        let cancelled = Rc::new(Cell::new(false));
        let cancel = cancelled.clone();
        let retry_ui = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            cancel.set(true);
            abort.abort();
            if response == "retry"
                && let Some(ui) = retry_ui.upgrade()
            {
                ui.trakt_device_login(retry_credentials.clone());
            }
        });
        let weak = Rc::downgrade(self);
        let dialog_weak = dialog.downgrade();
        glib::spawn_future_local(async move {
            let result = task.await;
            if cancelled.get() {
                return;
            }
            let (Some(ui), Some(dialog)) = (weak.upgrade(), dialog_weak.upgrade()) else {
                return;
            };
            match result {
                Ok(Ok((client, tokens))) => {
                    dialog.force_close();
                    let profiles = ui.profiles.clone();
                    ui.run(
                        async move { profiles.connect_trakt(session, client, tokens).await },
                        |ui, ()| {
                            ui.toast.add_toast(adw::Toast::new(
                                "Connected to Trakt. Importing your lists…",
                            ));
                            ui.trakt_sync(glib::WeakRef::new());
                        },
                    );
                }
                Ok(Err(error)) => {
                    spinner.stop();
                    waiting.set_visible(false);
                    dialog.set_body(&error.message);
                    dialog.set_heading(Some("Couldn’t connect to Trakt"));
                    dialog.set_response_label("cancel", "Close");
                    text.set_sensitive(false);
                    copy.set_sensitive(false);
                    dialog.add_response("retry", "Get new code");
                    dialog.set_response_appearance("retry", adw::ResponseAppearance::Suggested);
                }
                Err(_) => {}
            }
        });
        let parent: gtk::Widget = self
            .preferences
            .borrow()
            .as_ref()
            .map(|p| p.clone().upcast())
            .unwrap_or_else(|| self.window.clone().upcast());
        dialog.present(Some(&parent));
    }
    fn trakt_sync(self: &Rc<Self>, status: glib::WeakRef<adw::ActionRow>) {
        if let Some(row) = status.upgrade() {
            row.set_subtitle("Syncing watchlist, history and lists…");
        }
        let profiles = self.profiles.clone();
        let session = self.session.borrow().as_ref().unwrap().clone();
        // Return errors as data so the status row is restored after a failed sync.
        self.run(
            async move { Ok(profiles.sync_trakt(session, false).await) },
            move |ui, result| match result {
                Ok(Some(data)) => {
                    if let Some(row) = status.upgrade() {
                        row.set_subtitle(&format!("Connected as {} · Up to date", data.username));
                    }
                    ui.toast.add_toast(adw::Toast::new(&format!(
                        "Imported {} watchlist items, {} watched items and {} lists",
                        data.watchlist.len(),
                        data.history.len(),
                        data.lists.len()
                    )));
                    if data.skipped > 0 {
                        ui.toast.add_toast(adw::Toast::new(&format!(
                            "{} unsupported Trakt entries were skipped",
                            data.skipped
                        )));
                    }
                    ui.refresh();
                }
                Ok(None) => {
                    if let Some(row) = status.upgrade() {
                        row.set_subtitle("Not connected");
                    }
                }
                Err(e) => {
                    if let Some(row) = status.upgrade() {
                        row.set_subtitle("Sync failed · previous import kept");
                    }
                    ui.toast.add_toast(adw::Toast::new(&e.message));
                }
            },
        );
    }
    pub(in crate::app) fn trakt_background_sync(self: &Rc<Self>) {
        let profiles = self.profiles.clone();
        let session = self.session.borrow().as_ref().unwrap().clone();
        let id = session.profile.id.clone();
        let task = self
            .runtime
            .spawn(async move { profiles.sync_trakt(session, true).await });
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = task.await;
            let Some(ui) = weak.upgrade() else {
                return;
            };
            if ui
                .session
                .borrow()
                .as_ref()
                .is_none_or(|s| s.profile.id != id)
            {
                return;
            }
            if let Ok(Err(e)) = result {
                ui.toast
                    .add_toast(adw::Toast::new(&format!("Trakt: {}", e.message)));
            }
        });
    }
}
