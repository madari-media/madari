use super::*;

impl Ui {
    pub(in crate::app) fn settings(self: &Rc<Self>) {
        let s = self.session.borrow().as_ref().unwrap().clone();
        if s.profile.kids || s.profile.pin_protected {
            self.pin(
                "Unlock settings",
                if s.profile.kids {
                    "Enter the guardian’s PIN to change this kids profile."
                } else {
                    "Enter this profile’s PIN to change its settings."
                },
                |ui, pin| ui.unlock_settings(pin),
            );
        } else {
            self.unlock_settings(String::new());
        }
    }
    pub(in crate::app) fn unlock_settings(self: &Rc<Self>, pin: String) {
        let profiles = self.profiles.clone();
        let s = self.session.borrow().as_ref().unwrap().clone();
        self.run(
            async move { profiles.authorize_settings(s, pin).await },
            |ui, ()| {
                ui.editing.set(true);
                ui.refresh();
            },
        );
    }
}
