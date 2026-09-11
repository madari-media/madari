#if os(iOS)

import Darwin
import Foundation
import Mpv
import OpenGLES
import QuartzCore
import SwiftUI

/// Hosts libmpv's video output.
///
/// The vendored build uses `vo=libmpv`, so the app owns presentation: mpv renders into
/// an OpenGL ES framebuffer we supply, and we present it through a `CAEAGLLayer`. The
/// VideoToolbox decoder hands its frames over as GLES textures because that build
/// enables mpv's `ios-gl` interop, so no frames are copied through the CPU.
///
/// OpenGL ES is deprecated on iOS, but it is still the API this libmpv is built
/// against, and it remains available; the alternative would mean a Metal build of mpv
/// with libplacebo, which upstream does not support.
struct MpvSurface: UIViewRepresentable {
    let engine: MpvEngine

    func makeUIView(context: Context) -> MpvSurfaceView {
        let view = MpvSurfaceView(engine: engine)
        engine.surface = view
        // The surface and the engine start in whichever order SwiftUI chooses, so the
        // engine is told either way and completes the handshake with whichever half was
        // missing.
        engine.surfaceAttached()
        return view
    }

    func updateUIView(_ uiView: MpvSurfaceView, context: Context) {}

    static func dismantleUIView(_ uiView: MpvSurfaceView, coordinator: ()) {
        uiView.dispose()
    }
}

final class MpvSurfaceView: UIView {
    override class var layerClass: AnyClass { CAEAGLLayer.self }

    private let engine: MpvEngine
    private var glContext: EAGLContext?
    private var renderContext: OpaquePointer?
    private var framebuffer: GLuint = 0
    private var colorRenderbuffer: GLuint = 0
    private var pixelWidth: GLint = 0
    private var pixelHeight: GLint = 0
    private var rendersSeen = 0

    /// mpv resolves every GL entry point through this. The symbols live in the OpenGL ES
    /// framework, which is loaded because this view links it, so RTLD_DEFAULT finds them;
    /// opening it as a fallback keeps that from depending on load order.
    private static let openGLES: UnsafeMutableRawPointer? = dlopen(
        "/System/Library/Frameworks/OpenGLES.framework/OpenGLES",
        RTLD_NOW
    )

    /// mpv jumps straight through whatever this returns, so an unresolved entry point is
    /// a crash rather than an error. The names are recorded to make that visible.
    nonisolated(unsafe) private static var unresolvedCount = 0
    private static let unresolvedLock = NSLock()

    /// mpv calls this from its video output thread, so it must not be actor-isolated.
    ///
    /// Declared inside a `UIView` method, a closure inherits MainActor isolation, and Swift
    /// injects a runtime assertion into MainActor closures. mpv calls this on `vo_thread`,
    /// which is not the main queue, so the assertion trapped with EXC_BREAKPOINT and took
    /// the process down the moment the first frame arrived — from the crash reports, in
    /// `draw_frame` on `vo_thread`. A `@convention(c)` static has no isolation and cannot
    /// capture, so the view arrives through the context pointer and the hop to the main
    /// queue is explicit.
    private static let updateCallback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { context in
        guard let context else { return }
        let view = Unmanaged<MpvSurfaceView>.fromOpaque(context).takeUnretainedValue()
        DispatchQueue.main.async {
            // Already on the main queue, which is what the view requires. The frame is
            // consumed here rather than by scheduling a display pass: mpv reports
            // "mpv_render_context_render() not being called or stuck" while it waits, and
            // `setNeedsDisplay`/`draw(_:)` was not producing those calls.
            MainActor.assumeIsolated { view.renderFrame() }
        }
    }

    private static let getProcAddress: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> UnsafeMutableRawPointer? = { _, name in
        guard let name else { return nil }
        if let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), name) { return symbol }
        if let openGLES, let symbol = dlsym(openGLES, name) { return symbol }
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
        backgroundColor = .black
        isOpaque = true
        if let glLayer = layer as? CAEAGLLayer {
            glLayer.isOpaque = true
            glLayer.drawableProperties = [
                kEAGLDrawablePropertyRetainedBacking: false,
                kEAGLDrawablePropertyColorFormat: kEAGLColorFormatRGBA8,
            ]
        }
        glContext = EAGLContext(api: .openGLES3)
        EAGLContext.setCurrent(glContext)
        prepareFramebuffer()
        prepareRenderContext()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Releases the render context and the GL objects. SwiftUI calls this through
    /// `dismantleUIView` when the surface goes away; `MpvEngine.stop()` also tears the
    /// render context down first, because mpv requires that ordering. Safe to repeat.
    func dispose() {
        teardownRenderContext()
        guard let glContext else { return }
        EAGLContext.setCurrent(glContext)
        if colorRenderbuffer != 0 {
            glDeleteRenderbuffers(1, &colorRenderbuffer)
            colorRenderbuffer = 0
        }
        if framebuffer != 0 {
            glDeleteFramebuffers(1, &framebuffer)
            framebuffer = 0
        }
    }

    // MARK: - Setup

    /// Binds a colour renderbuffer that is backed by the layer's drawable, so mpv
    /// renders straight into what gets composited.
    private func prepareFramebuffer() {
        guard let glContext else { return }
        EAGLContext.setCurrent(glContext)
        if framebuffer == 0 { glGenFramebuffers(1, &framebuffer) }
        if colorRenderbuffer == 0 { glGenRenderbuffers(1, &colorRenderbuffer) }
        glBindFramebuffer(GLenum(GL_FRAMEBUFFER), framebuffer)
        glBindRenderbuffer(GLenum(GL_RENDERBUFFER), colorRenderbuffer)
        glContext.renderbufferStorage(Int(GL_RENDERBUFFER), from: layer as? CAEAGLLayer)
        glFramebufferRenderbuffer(
            GLenum(GL_FRAMEBUFFER),
            GLenum(GL_COLOR_ATTACHMENT0),
            GLenum(GL_RENDERBUFFER),
            colorRenderbuffer
        )
        glGetRenderbufferParameteriv(GLenum(GL_RENDERBUFFER), GLenum(GL_RENDERBUFFER_WIDTH), &pixelWidth)
        glGetRenderbufferParameteriv(GLenum(GL_RENDERBUFFER), GLenum(GL_RENDERBUFFER_HEIGHT), &pixelHeight)
        glBindRenderbuffer(GLenum(GL_RENDERBUFFER), 0)
        glBindFramebuffer(GLenum(GL_FRAMEBUFFER), 0)
    }

    /// Creates mpv's render context. mpv needs it before the first frame is decoded, so
    /// the engine calls this once it has a handle and the view is in place. Safe to call
    /// more than once, and a no-op until both halves exist.
    func prepareRenderContext() {
        guard renderContext == nil, let handle = engine.handle, let glContext else { return }
        EAGLContext.setCurrent(glContext)
        // The API type has to outlive the call. Swift's implicit String-to-pointer
        // conversion only guarantees its buffer for the duration of the expression that
        // produced it, and mpv reads this parameter inside
        // `mpv_render_context_create`, so a dangling pointer makes it fail with
        // "operation not implemented" because it cannot recognise the API it was asked
        // for. Hence the explicit copy.
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

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0 else { return }
        let width = GLint(bounds.width * contentScaleFactor)
        let height = GLint(bounds.height * contentScaleFactor)
        // The drawable is reallocated when the layer resizes, so the renderbuffer has to
        // be restated; mpv is told the framebuffer size on every frame it renders.
        if width != pixelWidth || height != pixelHeight {
            prepareFramebuffer()
            setNeedsDisplay()
        }
    }

    override func draw(_ rect: CGRect) {
        render()
    }

    /// Consumes one frame from mpv. Called from the render update callback and from the
    /// layer's own display pass.
    func renderFrame() {
        // `layoutSubviews` may not have run before the first frame arrives, in which case
        // there is no drawable to render into and mpv would wait forever.
        if pixelWidth <= 0 || pixelHeight <= 0 {
            prepareFramebuffer()
            if pixelWidth <= 0 || pixelHeight <= 0 {
                DebugLog.write("mpv: no drawable yet (\(bounds.width)x\(bounds.height) points)")
                return
            }
        }
        render()
    }

    private func render() {
        guard let renderContext, let glContext, pixelWidth > 0, pixelHeight > 0 else { return }
        EAGLContext.setCurrent(glContext)
        glBindFramebuffer(GLenum(GL_FRAMEBUFFER), framebuffer)
        glViewport(0, 0, pixelWidth, pixelHeight)
        var fbo = mpv_opengl_fbo(
            fbo: GLint(framebuffer),
            w: pixelWidth,
            h: pixelHeight,
            internal_format: GLint(GL_RGBA8)
        )
        // The layer's origin is top-left while OpenGL's is bottom-left.
        var flip: GLint = 1
        // The values these params point at must outlive the render call: mpv dereferences
        // them inside mpv_render_context_render, so `data: &fbo` would hand over a
        // temporary that may already be gone by then. That is the same mistake as the
        // dangling API-type string that made mpv refuse the render API outright.
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
                    DebugLog.write("mpv: render #\(rendersSeen + 1) calling, fbo=\(framebuffer) \(pixelWidth)x\(pixelHeight)")
                }
                mpv_render_context_render(renderContext, &params)
                if rendersSeen < 6 {
                    rendersSeen += 1
                    DebugLog.write("mpv: render #\(rendersSeen) returned")
                }
            }
        }
        glBindRenderbuffer(GLenum(GL_RENDERBUFFER), colorRenderbuffer)
        _ = glContext.presentRenderbuffer(Int(GL_RENDERBUFFER))
    }
}

#endif
