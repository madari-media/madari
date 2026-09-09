//! Drag and resize chrome for the persistent in-app video surface.
use super::*;
#[derive(Default)]
pub(in crate::app) struct Layout {
    rect: Cell<(i32, i32, i32, i32)>,
    origin: Cell<(i32, i32, i32, i32)>,
    pointer: Cell<(f64, f64)>,
    generation: Cell<u64>,
    handles: RefCell<Vec<gtk::Widget>>,
    paintable: RefCell<Option<gtk::WidgetPaintable>>,
}
fn bounded(rect: (i32, i32, i32, i32), width: i32, height: i32) -> (i32, i32, i32, i32) {
    let max_width = (width - 24).min((height - 24) * 16 / 9).clamp(1, 800);
    let w = rect.2.clamp(240.min(max_width), max_width);
    let h = (w * 9 / 16).max(1);
    (
        rect.0
            .clamp(12.min(width - w), (width - w - 12).max(12.min(width - w))),
        rect.1.clamp(
            12.min(height - h),
            (height - h - 12).max(12.min(height - h)),
        ),
        w,
        h,
    )
}
fn interactive_at(root: &gtk::Overlay, x: f64, y: f64) -> bool {
    let mut picked = root.pick(x, y, gtk::PickFlags::DEFAULT);
    while let Some(widget) = picked {
        if widget.is::<gtk::Button>()
            || widget.is::<gtk::MenuButton>()
            || widget.is::<gtk::Range>()
            || widget.is::<gtk::Popover>()
            || widget.is::<gtk::DrawingArea>()
        {
            return true;
        }
        if widget == *root.upcast_ref::<gtk::Widget>() {
            break;
        }
        picked = widget.parent();
    }
    false
}
impl Player {
    pub(in crate::app) fn mini_handles(self: &Rc<Self>) {
        // WidgetPaintable receives its cached render node during GTK frame updates.
        // Creating it only at toggle time can yield an empty image.
        *self.mini_layout.paintable.borrow_mut() =
            Some(gtk::WidgetPaintable::new(Some(&self.controls.root)));
        let resize = gtk::DrawingArea::new();
        resize.set_size_request(24, 24);
        resize.set_halign(gtk::Align::End);
        resize.set_valign(gtk::Align::End);
        resize.set_cursor_from_name(Some("se-resize"));
        resize.set_tooltip_text(Some("Drag to resize mini player"));
        resize.set_draw_func(|_, cr, _, _| {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
            cr.set_line_width(2.0);
            for offset in [5.0, 10.0, 15.0] {
                cr.move_to(21.0 - offset, 21.0);
                cr.line_to(21.0, 21.0 - offset);
            }
            let _ = cr.stroke();
        });
        for (widget, resizing) in [
            (self.controls.root.clone().upcast::<gtk::Widget>(), false),
            (resize.upcast(), true),
        ] {
            if resizing {
                widget.set_visible(false);
                self.controls.root.add_overlay(&widget);
                self.mini_layout.handles.borrow_mut().push(widget.clone());
            }
            let drag = gtk::GestureDrag::new();
            drag.set_button(1);
            if !resizing {
                drag.set_propagation_phase(gtk::PropagationPhase::Capture);
            }
            let weak = Rc::downgrade(self);
            drag.connect_drag_begin(move |gesture, x, y| {
                if let Some(p) = weak.upgrade() {
                    if p.mini.borrow().is_none()
                        || p.mini_animating.get()
                        || (!resizing && interactive_at(&p.controls.root, x, y))
                    {
                        gesture.set_state(gtk::EventSequenceState::Denied);
                        return;
                    }
                    p.mini_layout
                        .generation
                        .set(p.mini_layout.generation.get().wrapping_add(1));
                    p.mini_animating.set(false);
                    p.mini_layout.origin.set(p.mini_layout.rect.get());
                    p.mini_layout.pointer.set(
                        gesture
                            .current_event()
                            .and_then(|e| e.position())
                            .unwrap_or((0.0, 0.0)),
                    );
                    if resizing {
                        gesture.set_state(gtk::EventSequenceState::Claimed);
                    }
                }
            });
            let weak = Rc::downgrade(self);
            drag.connect_drag_update(move |gesture, dx, dy| {
                if let Some(p) = weak.upgrade() {
                    let (dx, dy) = gesture
                        .current_event()
                        .and_then(|e| e.position())
                        .map(|(x, y)| {
                            let (start_x, start_y) = p.mini_layout.pointer.get();
                            (x - start_x, y - start_y)
                        })
                        .unwrap_or((dx, dy));
                    if !resizing && dx.hypot(dy) < 6.0 {
                        return;
                    }
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    let (x, y, w, h) = p.mini_layout.origin.get();
                    let rect = if resizing {
                        let delta = if dx.abs() > (dy * 16.0 / 9.0).abs() {
                            dx
                        } else {
                            dy * 16.0 / 9.0
                        };
                        (x, y, w + delta as i32, h)
                    } else {
                        (x + dx as i32, y + dy as i32, w, h)
                    };
                    p.place_mini(bounded(rect, p.host.width(), p.host.height()));
                }
            });
            widget.add_controller(drag);
        }
        let weak = Rc::downgrade(self);
        // Keep the player inside the viewport after the application window resizes.
        self.host.add_tick_callback(move |host, _| {
            let Some(p) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if p.finished.get() {
                return glib::ControlFlow::Break;
            }
            if p.mini.borrow().is_some() && !p.mini_animating.get() {
                let rect = p.mini_layout.rect.get();
                let fit = bounded(rect, host.width(), host.height());
                if rect != fit {
                    p.place_mini(fit);
                }
            }
            glib::ControlFlow::Continue
        });
    }
    fn place_mini(&self, rect: (i32, i32, i32, i32)) {
        self.mini_layout.rect.set(rect);
        self.controls.root.set_halign(gtk::Align::Start);
        self.controls.root.set_valign(gtk::Align::Start);
        self.controls.root.set_margin_end(0);
        self.controls.root.set_margin_bottom(0);
        self.controls.root.set_margin_start(rect.0.max(0));
        self.controls.root.set_margin_top(rect.1.max(0));
        self.controls
            .root
            .set_size_request(rect.2.max(1), rect.3.max(1));
    }
    pub(in crate::app) fn animate_mini(self: &Rc<Self>, small: bool) {
        let width = self.host.width().max(1);
        let height = self.host.height().max(1);
        let from = if small && !self.mini_animating.get() {
            (0, 0, width, height)
        } else {
            self.mini_layout.rect.get()
        };
        let target = if small {
            let last = self.mini_layout.origin.get();
            bounded(
                if last.2 > 0 {
                    last
                } else {
                    (width - 420, height - 245, 400, 225)
                },
                width,
                height,
            )
        } else {
            (0, 0, width, height)
        };
        if !small {
            self.mini_layout.origin.set(from);
        }
        let generation = self.mini_layout.generation.get().wrapping_add(1);
        self.mini_layout.generation.set(generation);
        self.mini_animating.set(true);
        let animate = crate::app::interface::motion::enabled();
        // Snapshot once: animating the live GLArea reallocates its framebuffer on
        // every frame. A fixed render node scales cheaply while mpv keeps playing.
        let snapshot = gtk::Snapshot::new();
        let paintable = self
            .mini_layout
            .paintable
            .borrow()
            .as_ref()
            .unwrap()
            .clone();
        gtk::gdk::prelude::PaintableExt::snapshot(
            &paintable,
            &snapshot,
            self.controls.root.width().max(1) as f64,
            self.controls.root.height().max(1) as f64,
        );
        let still = if animate {
            snapshot.to_paintable(None).map(|image| {
                let picture = gtk::Picture::for_paintable(&image);
                picture.set_can_shrink(true);
                picture.set_content_fit(gtk::ContentFit::Fill);
                picture.set_can_target(false);
                picture.set_halign(gtk::Align::Start);
                picture.set_valign(gtk::Align::Start);
                picture.set_overflow(gtk::Overflow::Hidden);
                picture.set_margin_start(from.0.max(0));
                picture.set_margin_top(from.1.max(0));
                picture.set_size_request(from.2.max(1), from.3.max(1));
                self.host.add_overlay(&picture);
                self.controls.root.set_opacity(0.0);
                picture
            })
        } else {
            None
        };
        // Capture before changing any chrome: visibility changes invalidate allocation.
        for handle in self.mini_layout.handles.borrow().iter() {
            handle.set_visible(small);
        }
        if small {
            self.controls.set_compact(true);
            self.controls.root.add_css_class("in-app-mini");
        } else {
            self.controls.root.remove_css_class("in-app-mini");
        }
        // Allocate the live surface only once, underneath the transition image.
        let live_fallback = animate && still.is_none();
        self.place_mini(if live_fallback { from } else { target });
        if !small && !live_fallback {
            self.controls.set_compact(false);
            self.controls.root.set_margin_start(0);
            self.controls.root.set_margin_top(0);
            self.controls.root.set_size_request(-1, -1);
            self.controls.root.set_halign(gtk::Align::Fill);
            self.controls.root.set_valign(gtk::Align::Fill);
        }
        let started = Cell::new(None);
        let weak = Rc::downgrade(self);
        self.host.add_tick_callback(move |host, clock| {
            let Some(p) = weak.upgrade() else {
                if let Some(picture) = &still {
                    host.remove_overlay(picture);
                }
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            let start = started.get().unwrap_or(now);
            started.set(Some(start));
            let cancelled = p.finished.get() || p.mini_layout.generation.get() != generation;
            let t = if animate && !cancelled {
                ((now - start) as f64 / 280_000.0).clamp(0.0, 1.0)
            } else {
                1.0
            };
            // Smootherstep has zero velocity and acceleration at both ends.
            let ease = crate::app::interface::motion::ease(t);
            let blend = |a: i32, b: i32| (a as f64 + (b - a) as f64 * ease).round() as i32;
            if let Some(picture) = &still {
                picture.set_margin_start(blend(from.0, target.0).max(0));
                picture.set_margin_top(blend(from.1, target.1).max(0));
                picture.set_size_request(
                    blend(from.2, target.2).max(1),
                    blend(from.3, target.3).max(1),
                );
            }
            if live_fallback {
                p.place_mini((
                    blend(from.0, target.0),
                    blend(from.1, target.1),
                    blend(from.2, target.2),
                    blend(from.3, target.3),
                ));
            }
            if t < 1.0 {
                return glib::ControlFlow::Continue;
            }
            if let Some(picture) = &still {
                host.remove_overlay(picture);
            }
            p.controls.root.set_opacity(1.0);
            p.mini_animating.set(false);
            if !small && !cancelled {
                if live_fallback {
                    p.controls.set_compact(false);
                    p.controls.root.set_margin_start(0);
                    p.controls.root.set_margin_top(0);
                    p.controls.root.set_size_request(-1, -1);
                    p.controls.root.set_halign(gtk::Align::Fill);
                    p.controls.root.set_valign(gtk::Align::Fill);
                }
                p.original.set_visible(false);
            }
            glib::ControlFlow::Break
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an isolated GTK display"]
    fn transition_capture_is_ready_after_a_rendered_frame() {
        gtk::init().unwrap();
        let root = gtk::Overlay::new();
        root.set_child(Some(&gtk::Label::new(Some("Visible video placeholder"))));
        let retained = gtk::WidgetPaintable::new(Some(&root));
        let window = gtk::Window::builder()
            .default_width(640)
            .default_height(360)
            .child(&root)
            .build();
        window.present();
        let context = glib::MainContext::default();
        let capture = |paintable: &gtk::WidgetPaintable| {
            let snapshot = gtk::Snapshot::new();
            gtk::gdk::prelude::PaintableExt::snapshot(paintable, &snapshot, 640.0, 360.0);
            snapshot.to_node()
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while capture(&retained).is_none() && Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            capture(&retained).is_some(),
            "A retained capture must contain the last rendered frame"
        );
        let fresh = gtk::WidgetPaintable::new(Some(&root));
        eprintln!("new capture empty: {}", capture(&fresh).is_none());
        window.close();
    }
    #[test]
    fn resizing_stays_inside_viewport_and_keeps_video_ratio() {
        let (x, y, w, h) = bounded((2000, -500, 1200, 1), 1100, 740);
        assert!(x + w <= 1100 && y >= 0 && y + h <= 740);
        assert!((w as f64 / h as f64 - 16.0 / 9.0).abs() < 0.02);
        let (x, y, w, h) = bounded((-100, -100, 800, 1), 300, 200);
        assert!(x >= 0 && y >= 0 && x + w <= 300 && y + h <= 200);
    }
}
