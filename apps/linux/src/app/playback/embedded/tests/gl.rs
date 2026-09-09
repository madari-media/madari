//! Opt-in offscreen EGL test: no application windows, accessibility bus or GUI driver.
use super::*;
use std::ptr;
type Display = *mut c_void;
struct Offscreen {
    lib: libloading::Library,
    display: Display,
    surface: Display,
    context: Display,
}
impl Offscreen {
    fn new() -> Self {
        // SAFETY: signatures are EGL 1.5 ABI; handles remain owned until Drop.
        unsafe {
            let lib = libloading::Library::new("libEGL.so.1").unwrap();
            let get = lib
                .get::<unsafe extern "C" fn(u32, Display, *const isize) -> Display>(
                    b"eglGetPlatformDisplay\0",
                )
                .unwrap();
            let display = get(0x31DD, ptr::null_mut(), ptr::null());
            assert!(!display.is_null());
            let init = lib
                .get::<unsafe extern "C" fn(Display, *mut i32, *mut i32) -> u32>(b"eglInitialize\0")
                .unwrap();
            assert_ne!(init(display, ptr::null_mut(), ptr::null_mut()), 0);
            let bind = lib
                .get::<unsafe extern "C" fn(u32) -> u32>(b"eglBindAPI\0")
                .unwrap();
            assert_ne!(bind(0x30A2), 0);
            let choose=lib.get::<unsafe extern "C" fn(Display,*const i32,*mut Display,i32,*mut i32)->u32>(b"eglChooseConfig\0").unwrap();
            let mut config = ptr::null_mut();
            let mut count = 0;
            assert_ne!(
                choose(
                    display,
                    [
                        0x3033, 1, 0x3040, 8, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038
                    ]
                    .as_ptr(),
                    &mut config,
                    1,
                    &mut count
                ),
                0
            );
            assert_eq!(count, 1);
            let surface = lib
                .get::<unsafe extern "C" fn(Display, Display, *const i32) -> Display>(
                    b"eglCreatePbufferSurface\0",
                )
                .unwrap()(
                display, config, [0x3057, 160, 0x3056, 90, 0x3038].as_ptr()
            );
            assert!(!surface.is_null());
            let context = lib
                .get::<unsafe extern "C" fn(Display, Display, Display, *const i32) -> Display>(
                    b"eglCreateContext\0",
                )
                .unwrap()(
                display,
                config,
                ptr::null_mut(),
                [0x3098, 3, 0x30FB, 3, 0x30FD, 1, 0x3038].as_ptr(),
            );
            assert!(!context.is_null());
            let make = lib
                .get::<unsafe extern "C" fn(Display, Display, Display, Display) -> u32>(
                    b"eglMakeCurrent\0",
                )
                .unwrap();
            assert_ne!(make(display, surface, surface, context), 0);
            Self {
                lib,
                display,
                surface,
                context,
            }
        }
    }
}
impl Drop for Offscreen {
    fn drop(&mut self) {
        unsafe {
            self.lib
                .get::<unsafe extern "C" fn(Display, Display, Display, Display) -> u32>(
                    b"eglMakeCurrent\0",
                )
                .unwrap()(
                self.display,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            self.lib
                .get::<unsafe extern "C" fn(Display, Display) -> u32>(b"eglDestroyContext\0")
                .unwrap()(self.display, self.context);
            self.lib
                .get::<unsafe extern "C" fn(Display, Display) -> u32>(b"eglDestroySurface\0")
                .unwrap()(self.display, self.surface);
            self.lib
                .get::<unsafe extern "C" fn(Display) -> u32>(b"eglTerminate\0")
                .unwrap()(self.display);
        }
    }
}
#[test]
#[ignore = "Needs offscreen EGL; run with LIBGL_ALWAYS_SOFTWARE=1 LP_NUM_THREADS=2"]
fn video_survives_render_context_recreation() {
    let (_dir, video) = crate::app::playback::engine::video_fixture();
    let _egl = Offscreen::new();
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
    let mut renderer = Some(Renderer::new(mpv.clone(), dirty.clone()).unwrap());
    let (engine, events) = engine::start(mpv.clone());
    engine
        .0
        .send(Command::Start(Box::new(Target {
            title: "Test metadata title".into(),
            context: Default::default(),
            url: video.to_string_lossy().into(),
            headers: Default::default(),
            resume_ms: 0,
            offset_ms: 0,
            subtitles: vec![],
        })))
        .unwrap();
    for pass in 0..3 {
        if pass > 0 {
            renderer.take();
            renderer = Some(Renderer::new(mpv.clone(), dirty.clone()).unwrap());
            engine.0.send(Command::Reattach).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut colored = false;
        while Instant::now() < deadline {
            for n in events.try_iter() {
                if let Notice::Error(e) | Notice::Warning(e) = n {
                    panic!("{e}");
                }
            }
            if dirty.swap(false, Ordering::AcqRel) {
                let r = renderer.as_ref().unwrap();
                r.context
                    .as_ref()
                    .unwrap()
                    .render::<GlFunctions>(0, 160, 90, true)
                    .unwrap();
                let mut pixels = vec![0u8; 160 * 90 * 4];
                // SAFETY: current context owns a 160×90 framebuffer, buffer fits RGBA8 pixels.
                unsafe {
                    let read: unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void) =
                        std::mem::transmute(address(&r.gl, "glReadPixels"));
                    read(0, 0, 160, 90, 0x1908, 0x1401, pixels.as_mut_ptr().cast());
                }
                if pixels
                    .chunks_exact(4)
                    .any(|p| p[0] > 40 || p[1] > 40 || p[2] > 40)
                {
                    colored = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            colored,
            "Video became black after render-context transition {pass}"
        );
    }
    renderer.take();
    engine.0.send(Command::Shutdown).unwrap();
}
