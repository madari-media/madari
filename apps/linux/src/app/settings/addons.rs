use super::*;

impl Ui {
    pub(in crate::app) fn install(self: &Rc<Self>, id: Option<String>) {
        if let Some(id) = id {
            let profiles = self.profiles.clone();
            let s = self.session.borrow().as_ref().unwrap().clone();
            let lookup = id.clone();
            self.run(async move {profiles.linked_profiles(s,lookup).await},move |ui,names|ui.addon_dialog(Some(id),format!("Configuration changes apply to: {}. Paste the complete configured manifest URL.",names.join(", "))));
        } else {
            self.addon_dialog(None,"Paste an addon’s manifest URL. Configure it on its provider’s website first if required.".into());
        }
    }
    pub(in crate::app) fn addon_dialog(self: &Rc<Self>, id: Option<String>, body: String) {
        let fields = form();
        let url = entry("https://…/manifest.json", false);
        let local = gtk::CheckButton::with_label("Allow this addon to access the local network");
        fields.append(&url);
        fields.append(&local);
        self.dialog(
            if id.is_some() {
                "Configure shared addon"
            } else {
                "Install addon"
            },
            &body,
            &fields,
            "Save",
            move |ui| {
                let core = ui.core.borrow().as_ref().unwrap().clone();
                let id = id.clone();
                let url = url.text().to_string();
                let local = local.is_active();
                ui.run(
                    async move {
                        match id {
                            Some(id) => core.configure_addon(&id, &url, local).await,
                            None => {
                                core.install(uuid::Uuid::new_v4().to_string(), &url, local)
                                    .await
                            }
                        }
                    },
                    |ui, _| ui.refresh(),
                );
            },
        );
    }
    pub(in crate::app) fn share(self: &Rc<Self>, id: String) {
        let profiles = self.profiles.clone();
        self.run(async move {profiles.list().await},move |ui,profiles|{
            let current=ui.session.borrow().as_ref().unwrap().profile.id.clone();let targets:Vec<Profile>=profiles.into_iter().filter(|p|p.id!=current).collect();
            if targets.is_empty(){ui.toast.add_toast(adw::Toast::new("Create another profile before sharing an addon."));return;}
            let names:Vec<_>=targets.iter().map(|p|p.name.as_str()).collect();let select=gtk::DropDown::from_strings(&names);let pin=entry("Recipient or guardian PIN, if required",true);let fields=form();fields.append(&select);fields.append(&pin);
            ui.dialog("Share addon","This links one installation. Future configuration changes affect both profiles. Your own kids profiles use your current settings authorization.",&fields,"Share",move |ui|{
                let target=targets[select.selected() as usize].id.clone();let pin=pin.text().to_string();let profiles=ui.profiles.clone();let s=ui.session.borrow().as_ref().unwrap().clone();let id=id.clone();
                ui.run(async move {profiles.share(s,id,target,pin).await},|ui,()|{ui.toast.add_toast(adw::Toast::new("Addon shared."));ui.refresh();});
            });
        });
    }
}
