//! Real GTK/libmpv regression: move a running GLArea between two native windows.
use super::*;

fn wait(stage: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for _ in 0..8 {
            if !glib::MainContext::default().pending() {
                break;
            }
            glib::MainContext::default().iteration(false);
        }
        if ready() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "PiP stage did not finish: {stage}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "Requires GTK/OpenGL; run under Xvfb with software GL"]
fn pip_moves_keep_video_output_and_playback_state() {
    adw::init().unwrap();
    let (_dir, video) = crate::app::playback::engine::video_fixture();
    let main = gtk::Window::builder()
        .default_width(320)
        .default_height(180)
        .build();
    let pip = gtk::Window::builder()
        .default_width(280)
        .default_height(160)
        .build();
    let area = gtk::GLArea::builder()
        .auto_render(false)
        .hexpand(true)
        .vexpand(true)
        .build();
    area.set_required_version(3, 3);
    area.set_allowed_apis(gtk::gdk::GLAPI::GL);
    let pinned = preserve_gl_context(&area);
    let mpv = Arc::new(
        Mpv::with_initializer(|i| {
            i.set_option("vo", "libmpv")?;
            i.set_option("ao", "null")?;
            i.set_option("terminal", "no")?;
            Ok(())
        })
        .unwrap(),
    );
    let dirty = Arc::new(AtomicBool::new(true));
    let renderer = Rc::new(RefCell::new(None::<Renderer>));
    let constructions = Rc::new(Cell::new(0));
    let rendered = Rc::new(Cell::new(0));
    let r = renderer.clone();
    let m = mpv.clone();
    let d = dirty.clone();
    let count = constructions.clone();
    area.connect_realize(move |area| {
        area.make_current();
        assert!(area.error().is_none(), "{:?}", area.error());
        if r.borrow().is_none() {
            *r.borrow_mut() = Some(Renderer::new(m.clone(), d.clone()).unwrap());
            count.set(count.get() + 1);
        }
        d.store(true, Ordering::Release);
    });
    let r = renderer.clone();
    let frames = rendered.clone();
    area.connect_render(move |area, _| {
        if let Some(renderer) = r.borrow().as_ref() {
            renderer.draw(area).unwrap();
            area.attach_buffers();
            let width = area.width() * area.scale_factor();
            let height = area.height() * area.scale_factor();
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            // SAFETY: GTK's current framebuffer is bound and the RGBA8 buffer fits.
            unsafe {
                let read: unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void) =
                    std::mem::transmute(address(&renderer.gl, "glReadPixels"));
                read(
                    0,
                    0,
                    width,
                    height,
                    0x1908,
                    0x1401,
                    pixels.as_mut_ptr().cast(),
                );
            }
            if pixels
                .chunks_exact(4)
                .any(|p| p[0] > 40 || p[1] > 40 || p[2] > 40)
            {
                frames.set(frames.get() + 1);
            }
        }
        glib::Propagation::Stop
    });
    let d = dirty.clone();
    area.add_tick_callback(move |area, _| {
        if d.swap(false, Ordering::AcqRel) {
            area.queue_render();
        }
        glib::ControlFlow::Continue
    });
    main.set_child(Some(&area));
    main.present();
    wait("renderer creation", || renderer.borrow().is_some());
    let original_context = area.context().unwrap();
    let (engine, notices) = engine::start(mpv.clone());
    engine
        .0
        .send(Command::Start(Box::new(Target {
            title: "PiP continuity".into(),
            context: Default::default(),
            url: video.to_string_lossy().into(),
            headers: Default::default(),
            resume_ms: 2_000,
            offset_ms: 0,
            subtitles: vec![],
        })))
        .unwrap();
    wait("initial video frame", || {
        rendered.get() > 0 && mpv.get_property::<f64>("time-pos").unwrap_or(0.0) >= 2.0
    });
    engine.set("speed", "1.25");
    engine.set("vid", "1");
    engine.set("pause", "yes");
    wait("initial pause and speed", || {
        mpv.get_property::<bool>("pause").unwrap_or(false)
            && mpv.get_property::<f64>("speed").unwrap_or(0.0) == 1.25
    });
    let mut loads = 0;
    for pass in 0..4 {
        let paused = pass % 2 == 0;
        engine.set("pause", if paused { "yes" } else { "no" });
        wait("requested playback state", || {
            mpv.get_property::<bool>("pause").ok() == Some(paused)
        });
        let position = mpv.get_property::<f64>("time-pos").unwrap();
        let frames = rendered.get();
        if pass % 2 == 0 {
            main.set_child(gtk::Widget::NONE);
            pip.set_child(Some(&area));
            pip.present();
        } else {
            pip.set_child(gtk::Widget::NONE);
            main.set_child(Some(&area));
            main.present();
        }
        wait("video frame in destination window", || {
            area.is_mapped() && rendered.get() > frames
        });
        assert_eq!(area.context().as_ref(), Some(&original_context));
        assert_eq!(constructions.get(), 1, "renderer was recreated");
        assert_eq!(mpv.get_property::<bool>("pause").unwrap(), paused);
        assert_eq!(mpv.get_property::<String>("vid").unwrap(), "1");
        assert_eq!(mpv.get_property::<f64>("speed").unwrap(), 1.25);
        let after_move = mpv.get_property::<f64>("time-pos").unwrap();
        if paused {
            assert!((after_move - position).abs() < 0.1);
        } else {
            assert!(after_move >= position - 0.1 && after_move < position + 2.0);
        }
        engine.set("pause", "no");
        wait("playback advancing after move", || {
            mpv.get_property::<f64>("time-pos").unwrap_or(0.0) > position + 0.2
        });
        engine.set("pause", "yes");
        wait("pause after move", || {
            mpv.get_property::<bool>("pause").unwrap_or(false)
        });
        for notice in notices.try_iter() {
            match notice {
                Notice::Loaded => loads += 1,
                Notice::Error(message) | Notice::Warning(message) => panic!("{message}"),
                _ => (),
            }
        }
    }
    assert_eq!(loads, 1, "the media file was loaded again during PiP");
    pinned.borrow().as_ref().unwrap().make_current();
    renderer.borrow_mut().take();
    engine.0.send(Command::Shutdown).unwrap();
    pip.destroy();
    main.destroy();
}
