//! Layout-preserving skeletons share a low-frequency timer with weak references.
//! Only visible placeholders redraw; the timer stops when the final one is destroyed.
use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

type Placeholder = (gtk::glib::WeakRef<gtk::DrawingArea>, Rc<Cell<f64>>);
thread_local! {
    static PLACEHOLDERS: std::cell::RefCell<Vec<Placeholder>> = const { std::cell::RefCell::new(Vec::new()) };
    static ANIMATING: Cell<bool> = const { Cell::new(false) };
}
fn animate(area: &gtk::DrawingArea, phase: Rc<Cell<f64>>) {
    PLACEHOLDERS.with(|items| items.borrow_mut().push((area.downgrade(), phase)));
    if ANIMATING.with(|running| running.replace(true)) {
        return;
    }
    let started = std::time::Instant::now();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(83), move || {
        let enabled = gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations());
        let value = if enabled {
            (started.elapsed().as_secs_f64() / 2.4).fract()
        } else {
            0.0
        };
        let empty = PLACEHOLDERS.with(|items| {
            let mut items = items.borrow_mut();
            items.retain(|(weak, phase)| {
                let Some(area) = weak.upgrade() else {
                    return false;
                };
                if in_view(&area) && phase.replace(value) != value {
                    area.queue_draw();
                }
                true
            });
            items.is_empty()
        });
        if empty {
            ANIMATING.with(|running| running.set(false));
            gtk::glib::ControlFlow::Break
        } else {
            gtk::glib::ControlFlow::Continue
        }
    });
}

pub fn shimmer(width: i32, height: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(width, height);
    area.set_hexpand(width < 0);
    area.set_vexpand(height < 0);
    area.set_can_target(false);
    area.update_property(&[gtk::accessible::Property::Label("Loading")]);
    let phase = Rc::new(Cell::new(0.0));
    let drawing = phase.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (f64::from(w), f64::from(h));
        let radius = 10.0_f64.min(w / 2.0).min(h / 2.0);
        cr.new_sub_path();
        cr.arc(
            w - radius,
            radius,
            radius,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        );
        cr.arc(
            w - radius,
            h - radius,
            radius,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        cr.arc(
            radius,
            h - radius,
            radius,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        );
        cr.arc(
            radius,
            radius,
            radius,
            std::f64::consts::PI,
            3.0 * std::f64::consts::FRAC_PI_2,
        );
        cr.close_path();
        cr.clip();
        let pulse = (1.0 - (drawing.get() * std::f64::consts::TAU).cos()) * 0.5;
        let shade = 0.105 + pulse * 0.045;
        cr.set_source_rgb(shade, shade, shade + 0.025);
        let _ = cr.paint();
    });
    animate(&area, phase);
    area
}
pub fn poster_skeleton() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.append(&shimmer(170, 255));
    card.append(&shimmer(140, 14));
    card.append(&shimmer(74, 10));
    card
}
pub fn poster_row(row: &gtk::Box) {
    for _ in 0..7 {
        row.append(&poster_skeleton());
    }
}
pub fn hero_skeleton() -> gtk::Overlay {
    let hero = gtk::Overlay::new();
    hero.set_size_request(-1, 460);
    hero.set_child(Some(&shimmer(-1, 460)));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 16);
    text.set_margin_start(16);
    text.set_margin_end(16);
    text.set_margin_bottom(42);
    text.set_halign(gtk::Align::Fill);
    text.set_valign(gtk::Align::End);
    text.append(&shimmer(120, 13));
    text.append(&shimmer(-1, 48));
    text.append(&shimmer(-1, 16));
    text.append(&shimmer(-1, 16));
    text.append(&shimmer(140, 44));
    hero.add_overlay(&text);
    hero
}

/// Intersect with every ancestor, including scrolling viewports.
pub fn in_view(widget: &impl IsA<gtk::Widget>) -> bool {
    let widget = widget.as_ref();
    let mut parent = widget.parent();
    while let Some(ancestor) = parent {
        if let Some(bounds) = widget.compute_bounds(&ancestor)
            && (bounds.x() >= ancestor.width() as f32
                || bounds.y() >= ancestor.height() as f32
                || bounds.x() + bounds.width() <= 0.0
                || bounds.y() + bounds.height() <= 0.0)
        {
            return false;
        }
        parent = ancestor.parent();
    }
    widget.is_mapped()
}
