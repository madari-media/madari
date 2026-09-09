use adw::prelude::*;

pub fn row(spacing: i32) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
    row.add_css_class("responsive-row");
    row.connect_map(|row| {
        let narrow = row.root().is_some_and(|root| root.has_css_class("narrow"));
        row.set_orientation(if narrow {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
    });
    row
}

fn update(widget: &gtk::Widget, narrow: bool) {
    if widget.has_css_class("responsive-row")
        && let Some(row) = widget.downcast_ref::<gtk::Box>()
    {
        row.set_orientation(if narrow {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
    }
    if widget.has_css_class("wide-only") {
        widget.set_visible(!narrow);
    }
    if widget.has_css_class("narrow-only") {
        widget.set_visible(narrow);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        update(&current, narrow);
    }
}

pub fn visibility(
    widget: &impl IsA<gtk::Widget>,
    window: &adw::ApplicationWindow,
    narrow_only: bool,
) {
    widget.add_css_class(if narrow_only {
        "narrow-only"
    } else {
        "wide-only"
    });
    widget.set_visible(window.has_css_class("narrow") == narrow_only);
}

pub fn install(window: &adw::ApplicationWindow, content: &gtk::Box, navigation: &gtk::Box) {
    window.set_size_request(360, 280);
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        600.0,
        adw::LengthUnit::Sp,
    ));
    for widget in [content, navigation] {
        for property in ["margin-start", "margin-end"] {
            breakpoint.add_setter(widget, property, Some(&12i32.to_value()));
        }
    }
    for property in ["margin-top", "margin-bottom"] {
        breakpoint.add_setter(content, property, Some(&16i32.to_value()));
    }
    let weak = window.downgrade();
    breakpoint.connect_apply(move |_| {
        if let Some(window) = weak.upgrade() {
            window.add_css_class("narrow");
            update(window.upcast_ref(), true);
        }
    });
    let weak = window.downgrade();
    breakpoint.connect_unapply(move |_| {
        if let Some(window) = weak.upgrade() {
            window.remove_css_class("narrow");
            update(window.upcast_ref(), false);
        }
    });
    window.add_breakpoint(breakpoint);
}

pub fn overflow(title: &str, actions: &[gtk::Button]) -> gtk::MenuButton {
    let menu = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text(title)
        .build();
    menu.update_property(&[gtk::accessible::Property::Label(title)]);
    let popover = gtk::Popover::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for action in actions {
        let button = gtk::Button::with_label(action.label().as_deref().unwrap_or("Action"));
        button.add_css_class("flat");
        let weak = action.downgrade();
        let pop = popover.downgrade();
        button.connect_clicked(move |_| {
            if let Some(pop) = pop.upgrade() {
                pop.popdown();
            }
            if let Some(action) = weak.upgrade() {
                action.emit_clicked();
            }
        });
        content.append(&button);
    }
    popover.set_child(Some(&content));
    menu.set_popover(Some(&popover));
    menu
}
