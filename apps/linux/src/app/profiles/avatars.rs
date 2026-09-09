use super::*;
use madari_native::profiles::avatars;

impl Ui {
    /// Cached remote artwork layered over initials, which remain on network/decode failure.
    pub(in crate::app) fn profile_artwork(
        self: &Rc<Self>, name: &str, avatar: Option<&str>, size: i32,
    ) -> gtk::Overlay {
        let host = gtk::Overlay::new();
        host.set_size_request(size, size);
        host.set_halign(gtk::Align::Center);
        host.set_valign(gtk::Align::Center);
        host.set_overflow(gtk::Overflow::Hidden);
        host.add_css_class("profile-image");
        let fallback = adw::Avatar::builder().size(size).text(name).show_initials(true).build();
        host.set_child(Some(&fallback));
        if let Some(url) = avatar.and_then(avatars::url) {
            let picture = gtk::Picture::new();
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.set_size_request(size, size);
            host.add_overlay(&picture);
            self.artwork(&picture, &host, Some(&url), "profile-avatar");
        }
        host
    }

    /// The chooser edits only this form's draft. Apply/Create persists it with name and PIN.
    pub(in crate::app) fn avatar_field(
        self: &Rc<Self>, name: &str, initial: Option<&str>,
    ) -> (gtk::Box, Rc<RefCell<Option<String>>>) {
        let value = Rc::new(RefCell::new(initial.map(str::to_owned)));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let preview = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        preview.append(&self.profile_artwork(name, initial, 64));
        row.append(&preview);
        let menu = gtk::MenuButton::builder().label("Choose profile image").valign(gtk::Align::Center).build();
        let popover = gtk::Popover::new();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.append(&label("Choose profile image", "heading"));
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(4).max_children_per_line(4)
            .column_spacing(8).row_spacing(8).homogeneous(true).build();
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_width(420).min_content_height(300)
            .max_content_height(360).child(&grid).build();
        content.append(&scroll);
        popover.set_child(Some(&content));
        menu.set_popover(Some(&popover));
        row.append(&menu);
        let mut group: Option<gtk::ToggleButton> = None;
        for id in std::iter::once(None).chain(avatars::FILES.iter().map(|&id| Some(id))) {
            let title = id.and_then(|id| id.strip_suffix(".webp")).unwrap_or("Use initials");
            let tile = gtk::Box::new(gtk::Orientation::Vertical, 6);
            tile.append(&self.profile_artwork(if id.is_some() { title } else { name }, id, 64));
            let caption = label(title, "caption");
            caption.set_max_width_chars(12);
            caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
            tile.append(&caption);
            let button = gtk::ToggleButton::builder().child(&tile).tooltip_text(title).build();
            if let Some(first) = &group { button.set_group(Some(first)); }
            else { group = Some(button.clone()); }
            button.set_active(id == initial);
            let weak = Rc::downgrade(self);
            let weak_popover = popover.downgrade();
            let preview = preview.clone();
            let value = value.clone();
            let name = name.to_owned();
            let id = id.map(str::to_owned);
            button.connect_clicked(move |button| {
                if !button.is_active() { button.set_active(true); }
                *value.borrow_mut() = id.clone();
                if let Some(ui) = weak.upgrade() {
                    while let Some(child) = preview.first_child() { preview.remove(&child); }
                    preview.append(&ui.profile_artwork(&name, id.as_deref(), 64));
                }
                if let Some(popover) = weak_popover.upgrade() { popover.popdown(); }
            });
            grid.insert(&button, -1);
        }
        (row, value)
    }
}
