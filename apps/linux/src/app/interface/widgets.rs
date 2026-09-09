use super::*;

pub(in crate::app) fn brand_logo(size: i32) -> gtk::Image {
    let bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icon/icon_mini.png"
    )));
    let texture = gtk::gdk::Texture::from_bytes(&bytes).expect("decode bundled Madari logo");
    let logo = gtk::Image::from_paintable(Some(&texture));
    logo.set_pixel_size(size);
    logo.set_halign(gtk::Align::Start);
    logo.set_valign(gtk::Align::Center);
    logo.add_css_class("brand-logo");
    logo.update_property(&[gtk::accessible::Property::Label("Madari")]);
    logo
}

pub(in crate::app) fn label(text: &str, style: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(style);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_xalign(0.0);
    label
}
pub(in crate::app) fn entry(placeholder: &str, secret: bool) -> gtk::Entry {
    let entry = gtk::Entry::builder()
        .placeholder_text(placeholder)
        .visibility(!secret)
        .build();
    entry.update_property(&[gtk::accessible::Property::Label(placeholder)]);
    if secret {
        entry.set_max_length(8);
        entry.set_input_purpose(gtk::InputPurpose::Digits);
    }
    entry
}
pub(in crate::app) fn form() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, 12)
}

impl Ui {
    pub(in crate::app) fn button(
        self: &Rc<Self>,
        title: &str,
        action: impl Fn(Rc<Self>) + 'static,
    ) -> gtk::Button {
        let button = gtk::Button::with_label(title);
        button.add_css_class("compact-action");
        button.set_can_shrink(true);
        button.set_halign(gtk::Align::Start);
        button.set_valign(gtk::Align::Center);
        button.update_property(&[gtk::accessible::Property::Label(title)]);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade()
                && !ui.busy.get()
            {
                action(ui);
            }
        });
        button
    }
}
