use super::*;

impl Ui {
    pub(in crate::app) fn dialog(
        self: &Rc<Self>,
        title: &str,
        body: &str,
        child: &gtk::Box,
        confirm: &str,
        action: impl Fn(Rc<Self>) + 'static,
    ) {
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .max_content_height((self.window.height() - 220).clamp(80, 400))
            .propagate_natural_height(true)
            .child(child)
            .build();
        let dialog = adw::AlertDialog::builder()
            .heading(title)
            .body(body)
            .extra_child(&scroll)
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("confirm", confirm)]);
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("confirm"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            if response == "confirm"
                && let Some(ui) = weak.upgrade()
            {
                action(ui);
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
    pub(in crate::app) fn pin(
        self: &Rc<Self>,
        title: &str,
        body: &str,
        action: impl Fn(Rc<Self>, String) + 'static,
    ) {
        let box_ = form();
        let pin = entry("PIN (4–8 digits)", true);
        box_.append(&pin);
        self.dialog(title, body, &box_, "Unlock", move |ui| {
            action(ui, pin.text().to_string())
        });
    }
}
