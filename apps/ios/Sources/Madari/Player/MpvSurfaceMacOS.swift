#if os(macOS)

import AppKit
import Darwin
import Foundation
import Mpv
import OpenGL.GL3
import SwiftUI

/// Hosts libmpv's video output on macOS.
///
/// The contract is the same as the iOS surface: the vendored build uses `vo=libmpv`, so
/// the app owns presentation, and mpv renders through the render API into a context we
/// provide. What differs is the context — iOS hands us a `CAEAGLLayer` and OpenGL ES,
/// macOS an `NSOpenGLContext` drawing into the window's own framebuffer.
///
/// The two surfaces are separate files rather than one full of conditionals: the layers,
/// the context APIs and the surface-size bookkeeping have almost nothing in common, and
/// the iOS one is verified on device.
struct MpvSurface: NSViewRepresentable {
    let engine: MpvEngine

    func makeNSView(context: Context) -> MpvSurfaceView {
        let view = MpvSurfaceView(engine: engine)
        engine.surface = view
        // The surface and the engine start in whichever order SwiftUI chooses, so the
        // engine is told either way and completes the handshake with whichever half was
        // missing.
        engine.surfaceAttached()
        return view
    }

    func updateNSView(_ nsView: MpvSurfaceView, context: Context) {}

    static func dismantleNSView(_ nsView: MpvSurfaceView, coordinator: ()) {
        nsView.dispose()
    }
}

final class MpvSurfaceView: NSOpenGLView {
    private let engine: MpvEngine
    private var renderContext: OpaquePointer?
    private var rendersSeen = 0

    /// mpv resolves every GL entry point through this. The symbols live in the OpenGL
    /// framework, which this view links, so RTLD_DEFAULT finds them; opening it as a
    /// fallback keeps that from depending on load order.
    private static let openGL = dlopen(
        "/System/Library/Frameworks/OpenGL.framework/OpenGL",
        RTLD_NOW
    )

    /// A 3.2 core profile is what mpv's OpenGL renderer expects on macOS.
    static let pixelFormat: NSOpenGLPixelFormat? = {
        let attributes: [NSOpenGLPixelFormatAttribute] = [
            NSOpenGLPixelFormatAttribute(NSOpenGLPFAOpenGLProfile),
            NSOpenGLPixelFormatAttribute(NSOpenGLProfileVersion3_2Core),
            NSOpenGLPixelFormatAttribute(NSOpenGLPFAColorSize), 24,
            NSOpenGLPixelFormatAttribute(NSOpenGLPFAAlphaSize), 8,
            NSOpenGLPixelFormatAttribute(NSOpenGLPFADoubleBuffer),
            NSOpenGLPixelFormatAttribute(NSOpenGLPFAAccelerated),
            0,
        ]
        return NSOpenGLPixelFormat(attributes: attributes)
    }()

    /// mpv jumps straight through whatever this returns, so an unresolved entry point is
    /// a crash rather than an error. The names are recorded to make that visible.
    nonisolated(unsafe) private static var unresolvedCount = 0
    private static let unresolvedLock = NSLock()

    /// mpv calls this from its video output thread, so it must not be actor-isolated.
    ///
    /// This is the same trap that crashed the iOS build: a closure written inside a view
    /// method inherits MainActor isolation, Swift injects a runtime assertion into
    /// MainActor closures, and mpv calls this on `vo_thread` — which trapped with
    /// EXC_BREAKPOINT the moment the first frame arrived. A `@convention(c)` static has no
    /// isolation and cannot capture, so the view arrives through the context pointer.
    private static let updateCallback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { context in
        guard let context else { return }
        let view = Unmanaged<MpvSurfaceView>.fromOpaque(context).takeUnretainedValue()
        DispatchQueue.main.async {
            MainActor.assumeIsolated { view.renderFrame() }
        }
    }

    private static let getProcAddress: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> UnsafeMutableRawPointer? = { _, name in
        guard let name else { return nil }
        if let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), name) { return symbol }
        if let openGL, let symbol = dlsym(openGL, name) { return symbol }
        let missing = String(cString: name)
        unresolvedLock.lock()
        if unresolvedCount < 20 {
            unresolvedCount += 1
            DebugLog.write("mpv: GL entry point unresolved: \(missing)")
        }
        unresolvedLock.unlock()
        return nil
    }

    init(engine: MpvEngine) {
        self.engine = engine
        super.init(frame: .zero)
        pixelFormat = Self.pixelFormat
        // Retina: without this the surface is presented at 1x and the video looks soft.
        wantsBestResolutionOpenGLSurface = true
        if let format = Self.pixelFormat {
            openGLContext = NSOpenGLContext(format: format, share: nil)
        }
        if openGLContext == nil {
            DebugLog.write("mpv: no OpenGL context could be created")
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override var isOpaque: Bool { true }

    /// AppKit calls this once the context is current for the first time, which is the
    /// earliest point a render context can be created.
    override func prepareOpenGL() {
        super.prepareOpenGL()
        prepareRenderContext()
    }

    /// The drawable only exists once the view is in a window, so the handshake the engine
    /// started at init may have found no context to work with.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window != nil {
            prepareRenderContext()
        }
    }

    /// Releases the render context. SwiftUI calls this through `dismantleNSView` when the
    /// surface goes away; `MpvEngine.stop()` also tears the render context down first,
    /// because mpv requires that ordering. Safe to repeat.
    func dispose() {
        teardownRenderContext()
    }

    // MARK: - Setup

    /// Creates mpv's render context. mpv needs it before the first frame is decoded, so
    /// the engine calls this once it has a handle and the view is in place. Safe to call
    /// more than once, and a no-op until both halves exist.
    func prepareRenderContext() {
        guard renderContext == nil, let handle = engine.handle, let openGLContext else { return }
        openGLContext.makeCurrentContext()
        // The API type has to outlive the call. Swift's implicit String-to-pointer
        // conversion only guarantees its buffer for the duration of the expression that
        // produced it, and mpv reads this parameter inside `mpv_render_context_create`, so
        // a dangling pointer makes it fail with "operation not implemented" because it
        // cannot recognise the API it was asked for. Hence the explicit copy.
        guard let apiType = strdup(MPV_RENDER_API_TYPE_OPENGL) else { return }
        defer { free(apiType) }
        var initParams = mpv_opengl_init_params(
            get_proc_address: Self.getProcAddress,
            get_proc_address_ctx: nil
        )
        var context: OpaquePointer?
        let result = withUnsafeMutablePointer(to: &initParams) { initParams -> Int32 in
            var params = [
                mpv_render_param(
                    type: MPV_RENDER_PARAM_API_TYPE,
                    data: UnsafeMutableRawPointer(apiType)
                ),
                mpv_render_param(
                    type: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                    data: UnsafeMutableRawPointer(initParams)
                ),
                mpv_render_param(type: MPV_RENDER_PARAM_INVALID, data: nil),
            ]
            return mpv_render_context_create(&context, handle, &params)
        }
        guard result >= 0, let context else {
            engine.reportRenderFailure(String(cString: mpv_error_string(result)))
            return
        }
        renderContext = context
        // Video output exists now, which is what the engine was waiting for to load.
        engine.videoOutputReady()
        // mpv tells us when a new frame is ready; that is the only time we need to draw.
        mpv_render_context_set_update_callback(
            context,
            Self.updateCallback,
            Unmanaged.passUnretained(self).toOpaque()
        )
    }

    /// mpv requires its render context to go before the mpv handle is destroyed, so
    /// `MpvEngine.stop()` calls this first. Safe to call more than once.
    func teardownRenderContext() {
        guard let renderContext else { return }
        mpv_render_context_set_update_callback(renderContext, nil, nil)
        mpv_render_context_free(renderContext)
        self.renderContext = nil
    }

    // MARK: - Drawing

    /// The drawable is resized with the window, so the viewport mpv is told about is
    /// recomputed on every frame rather than cached.
    override func reshape() {
        super.reshape()
        openGLContext?.update()
        setNeedsDisplay(bounds)
    }

    override func draw(_ dirtyRect: NSRect) {
        render()
    }

    /// Consumes one frame from mpv. Called from the render update callback and from the
    /// view's own display pass.
    func renderFrame() {
        // Nothing can be presented before the view is in a window: there is no drawable,
        // and mpv would be asked to render into a framebuffer that does not exist yet.
        guard window != nil else { return }
        render()
    }

    private func render() {
        guard let renderContext, let openGLContext, window != nil else { return }
        let backing = convertToBacking(bounds)
        let width = GLint(backing.width)
        let height = GLint(backing.height)
        guard width > 0, height > 0 else { return }
        openGLContext.makeCurrentContext()
        glViewport(0, 0, width, height)
        // Framebuffer 0 is the window's own drawable, which is already sized to the view,
        // so unlike the iOS surface there is no renderbuffer to allocate here.
        var fbo = mpv_opengl_fbo(
            fbo: 0,
            w: width,
            h: height,
            internal_format: GLint(GL_RGBA8)
        )
        // 0 rather than 1: the window's default framebuffer has OpenGL's own bottom-left
        // origin, whereas the iOS surface presents through a CAEAGLLayer, whose origin is
        // top-left. This is the one value that would need flipping if the picture came out
        // inverted.
        var flip: GLint = 0
        // The values these params point at must outlive the render call: mpv dereferences
        // them inside mpv_render_context_render, so `data: &fbo` would hand over a
        // temporary that may already be gone by then. Same mistake as the dangling
        // API-type string that made mpv refuse the render API outright.
        var params: [mpv_render_param] = []
        withUnsafeMutablePointer(to: &fbo) { fboPointer in
            withUnsafeMutablePointer(to: &flip) { flipPointer in
                params = [
                    mpv_render_param(
                        type: MPV_RENDER_PARAM_OPENGL_FBO,
                        data: UnsafeMutableRawPointer(fboPointer)
                    ),
                    mpv_render_param(
                        type: MPV_RENDER_PARAM_FLIP_Y,
                        data: UnsafeMutableRawPointer(flipPointer)
                    ),
                    mpv_render_param(type: MPV_RENDER_PARAM_INVALID, data: nil),
                ]
                if rendersSeen < 6 {
                    DebugLog.write("mpv: render #\(rendersSeen + 1) calling, \(width)x\(height)")
                }
                mpv_render_context_render(renderContext, &params)
                if rendersSeen < 6 {
                    rendersSeen += 1
                    DebugLog.write("mpv: render #\(rendersSeen) returned")
                }
            }
        }
        // Double buffered, so this is what actually shows the frame.
        openGLContext.flushBuffer()
    }
}

#endif
