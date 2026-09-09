use super::*;

impl Ui {
    pub(in crate::app) fn profile_picker(self: &Rc<Self>, profiles: Vec<Profile>) {
        self.clear();
        self.navigation.set_visible(false);
        self.content.set_valign(gtk::Align::Center);
        let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
        panel.add_css_class("profile-picker");
        let logo = brand_logo(48);
        logo.set_halign(gtk::Align::Center);
        panel.append(&logo);
        let title = label("Who’s watching?", "profile-heading");
        title.set_halign(gtk::Align::Fill);
        title.set_xalign(0.5);
        title.set_justify(gtk::Justification::Center);
        panel.append(&title);
        let subtitle = label(
            if profiles.is_empty() {
                "Create a profile to make Madari yours."
            } else {
                "Your shows. Your place to pick up."
            },
            "profile-subtitle",
        );
        subtitle.set_halign(gtk::Align::Fill);
        subtitle.set_xalign(0.5);
        subtitle.set_justify(gtk::Justification::Center);
        panel.append(&subtitle);
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(12)
            .row_spacing(12)
            .min_children_per_line(1)
            .max_children_per_line(4)
            .homogeneous(true)
            .halign(gtk::Align::Center)
            .build();
        grid.add_css_class("profile-grid");
        grid.set_margin_top(24);
        for (index, profile) in profiles.iter().enumerate() {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
            let avatar = self.profile_artwork(&profile.name, profile.avatar.as_deref(), 72);
            card.append(&avatar);
            let name = label(&profile.name, "profile-name");
            name.set_lines(2);
            name.set_max_width_chars(10);
            name.set_width_chars(1);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_xalign(0.5);
            name.set_justify(gtk::Justification::Center);
            card.append(&name);
            let detail = gtk::Box::new(gtk::Orientation::Horizontal, 5);
            detail.set_halign(gtk::Align::Center);
            if profile.pin_protected || profile.kids {
                let icon = gtk::Image::from_icon_name(if profile.kids {
                    "face-smile-symbolic"
                } else {
                    "system-lock-screen-symbolic"
                });
                icon.set_pixel_size(12);
                detail.append(&icon);
            }
            detail.add_css_class("profile-subtitle");
            detail.append(&label(
                if profile.kids {
                    "Kids"
                } else if profile.pin_protected {
                    "PIN protected"
                } else {
                    "Personal"
                },
                "profile-type",
            ));
            card.append(&detail);
            let selected = profile.clone();
            let button = self.button(&profile.name, move |ui| {
                if selected.pin_protected {
                    let id = selected.id.clone();
                    ui.pin(
                        "Open profile",
                        &format!("Enter the PIN for {}.", selected.name),
                        move |ui, pin| ui.open(id.clone(), pin),
                    );
                } else {
                    ui.open(selected.id.clone(), String::new());
                }
            });
            button.remove_css_class("compact-action");
            button.add_css_class("profile-tile");
            button.set_halign(gtk::Align::Fill);
            button.set_valign(gtk::Align::Fill);
            button.set_tooltip_text(Some(&profile.name));
            button.set_child(Some(&card));
            grid.insert(
                &crate::app::interface::motion::slide_in(&button, index as u64 * 45),
                -1,
            );
        }
        if !profiles.is_empty() {
            panel.append(&grid);
        }
        let add = self.button("Add profile", move |ui| ui.picker_add(profiles.clone()));
        add.remove_css_class("compact-action");
        add.add_css_class("profile-create");
        add.set_halign(gtk::Align::Center);
        add.set_margin_top(20);
        let add_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        add_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
        add_content.append(&label("Add profile", "heading"));
        add.set_child(Some(&add_content));
        panel.append(&add);
        let clamp = adw::Clamp::builder()
            .maximum_size(760)
            .tightening_threshold(600)
            .child(&panel)
            .build();
        self.content.append(&clamp);
    }
}
