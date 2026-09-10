use super::*;

impl Ui {
    pub(in crate::app) fn picker(self: &Rc<Self>) {
        let profiles = self.profiles.clone();
        self.run(
            async move { Ok((profiles.list().await?, profiles.active_kids().await?)) },
            |ui, (profiles, kids)| {
                if let Some(kid) = kids {
                    ui.open(kid.id, String::new());
                    return;
                }
                ui.profile_picker(profiles);
            },
        );
    }
    pub(in crate::app) fn open(self: &Rc<Self>, id: String, pin: String) {
        self.artwork_policies.borrow_mut().clear();
        self.playback_titles.borrow_mut().clear();
        self.playback_metadata.borrow_mut().clear();
        if let Some(p) = self.preferences.borrow_mut().take() {
            p.destroy();
        }
        self.preferences_pages.borrow_mut().clear();
        let profiles = self.profiles.clone();
        self.run(
            async move {
                let session = profiles.unlock(id, pin).await?;
                // Seed a profile that has no addons at all. The core skips any profile
                // that already has one, so a default the user removes stays removed,
                // and a failure here never blocks entry.
                let core = profiles.core(session.clone());
                if let Ok(snapshot) = core.snapshot().await
                    && snapshot.addons.is_empty()
                {
                    let _ = core.install_default_addons().await;
                }
                Ok(session)
            },
            |ui, session| {
                *ui.core.borrow_mut() = Some(ui.profiles.core(session.clone()));
                *ui.session.borrow_mut() = Some(session);
                ui.editing.set(false);
                ui.refresh();
                ui.trakt_background_sync();
            },
        );
    }
    pub(in crate::app) fn switch_profile(self: &Rc<Self>) {
        if self.playback_active.get() {
            *self.after_playback.borrow_mut() = Some(Box::new(|ui| ui.switch_profile()));
            if let Some(stop) = self.player_stop.borrow_mut().take() {
                let _ = stop.send(());
            }
            return;
        }

        if self.session.borrow().as_ref().unwrap().profile.kids {
            self.pin(
                "Leave kids mode",
                "Enter the guardian’s PIN to choose another profile.",
                |ui, pin| ui.leave(pin),
            );
        } else {
            self.leave(String::new());
        }
    }
    pub(in crate::app) fn leave(self: &Rc<Self>, pin: String) {
        let profiles = self.profiles.clone();
        let s = self.session.borrow().as_ref().unwrap().clone();
        self.run(async move { profiles.leave(s, pin).await }, |ui, ()| {
            ui.session.borrow_mut().take();
            ui.core.borrow_mut().take();
            ui.editing.set(false);
            ui.picker();
        });
    }
    pub(in crate::app) fn create_profile(self: &Rc<Self>) {
        self.create_dialog(self.session.borrow().clone());
    }
    pub(in crate::app) fn create_dialog(self: &Rc<Self>, actor: Option<ProfileSession>) {
        let fields = form();
        let name = entry("Profile name", false);
        let (image_field, avatar) = self.avatar_field("", None);
        fields.append(&image_field);
        let pin = entry("Optional PIN (4–8 digits)", true);
        let kids = gtk::CheckButton::with_label("Kids profile — uses your PIN for adult controls");
        fields.append(&name);
        fields.append(&pin);
        if actor.is_some() {
            fields.append(&kids);
        }
        let pin_copy = pin.clone();
        kids.connect_toggled(move |kids| pin_copy.set_sensitive(!kids.is_active()));
        self.dialog("Create profile", "Each profile has its own addons and viewing state. Creating a kids profile requires a PIN on your regular profile.",&fields,"Create",move |ui|{
            let profiles=ui.profiles.clone(); let s=actor.clone(); let name=name.text().to_string(); let pin=pin.text().to_string();let kids=kids.is_active();
            let entry_pin=pin.clone();
            let avatar=avatar.borrow().clone();
            ui.run(async move {profiles.create_with_avatar(s,name,kids,pin,avatar).await},move |ui,p|ui.open(p.id,entry_pin));
        });
    }
}
