//! Shared browsing chrome, outside the page scroller and navigation transitions.
use super::*;

impl Ui {
    pub(in crate::app) fn build_navigation(self: &Rc<Self>) {
        while let Some(child) = self.navigation.first_child() {
            self.navigation.remove(&child);
        }
        self.navigation.set_visible(true);
        self.navigation.append(&brand_logo(40));
        let mut destinations = Vec::new();
        for title in [
            "Home", "Movies", "Series", "Search", "My list", "Calendar", "Torrents",
        ] {
            let button = self.button(title, move |ui| {
                if title == "Torrents" {
                    ui.torrent_manager();
                    return;
                }
                if title == "Calendar" {
                    ui.calendar_page();
                    return;
                }
                let Some(core) = ui.core.borrow().as_ref().cloned() else {
                    return;
                };
                ui.run(
                    async move { core.snapshot().await },
                    move |ui, snapshot| match title {
                        "Search" => ui.search_page(snapshot),
                        "My list" => ui.saved_page(snapshot),
                        "Movies" => ui.dashboard_filter(snapshot, Some("movie")),
                        "Series" => ui.dashboard_filter(snapshot, Some("series")),
                        _ => ui.dashboard(snapshot),
                    },
                );
            });
            button.add_css_class("flat");
            button.set_can_shrink(false);
            responsive::visibility(&button, &self.window, false);
            destinations.push(button.clone());
            self.navigation.append(&button);
        }
        let browse = responsive::overflow("Browse", &destinations);
        browse.set_icon_name("");
        browse.set_label("Browse");
        responsive::visibility(&browse, &self.window, true);
        self.navigation.append(&browse);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        self.navigation.append(&spacer);
        let settings = self.button("Settings", |ui| ui.settings());
        let profiles = self.button("Switch profile", |ui| ui.switch_profile());
        let overflow = responsive::overflow(
            "Settings and profiles",
            &[settings.clone(), profiles.clone()],
        );
        for action in [&settings, &profiles] {
            responsive::visibility(action, &self.window, false);
            self.navigation.append(action);
        }
        responsive::visibility(&overflow, &self.window, true);
        self.navigation.append(&overflow);
    }

    pub(in crate::app) fn select_section(&self, title: &str) {
        let mut child = self.navigation.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                if button.label().as_deref() == Some(title) {
                    button.add_css_class("current-section");
                } else {
                    button.remove_css_class("current-section");
                }
            }
        }
    }

    pub(in crate::app) fn section_heading(&self, title: &str, subtitle: Option<&str>) -> gtk::Box {
        self.select_section(title);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let text = form();
        text.set_hexpand(true);
        text.append(&label(title, "hero-title"));
        if let Some(subtitle) = subtitle {
            text.append(&label(subtitle, "hero-meta"));
        }
        heading.append(&text);
        self.content.append(&heading);
        heading
    }
}
