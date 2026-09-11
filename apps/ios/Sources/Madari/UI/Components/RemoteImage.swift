import SwiftUI

/// A cached, de-duplicated, retrying image loader shared by every remote image.
///
/// `AsyncImage` starts one unbounded request per view and never retries, so a grid of
/// 72 avatars or a wall of posters opens dozens of simultaneous connections and every
/// request iOS drops stays a permanent placeholder.
///
/// Concurrency is bounded by `URLSession`'s own per-host connection limit rather than
/// by an app-level semaphore. A semaphore here would have to be cancellation-aware:
/// a lazy grid cancels the tasks of tiles that scroll away, and a cancellation while
/// queued would leak its permit until every later request stalled behind permits that
/// were never returned.
@MainActor
enum ImageCache {
    private static let cache: NSCache<NSString, PlatformImage> = {
        let cache = NSCache<NSString, PlatformImage>()
        cache.countLimit = 400
        return cache
    }()

    /// Requests already in flight, keyed by URL, so a URL is fetched once no matter
    /// how many views ask for it. These are deliberately unstructured: scrolling a
    /// tile away must not throw away a download the cache is about to keep.
    private static var inFlight: [String: Task<PlatformImage?, Never>] = [:]

    private static let session: URLSession = {
        let configuration = URLSessionConfiguration.default
        configuration.urlCache = URLCache(
            memoryCapacity: 32 * 1024 * 1024,
            diskCapacity: 256 * 1024 * 1024,
            diskPath: "madari-artwork"
        )
        configuration.requestCachePolicy = .returnCacheDataElseLoad
        // The bound on how many downloads run at once.
        configuration.httpMaximumConnectionsPerHost = 4
        // Fail rather than wait forever, so a tile can be retried when it reappears
        // instead of staying blank for the life of the screen.
        configuration.waitsForConnectivity = false
        configuration.timeoutIntervalForRequest = 20
        return URLSession(configuration: configuration)
    }()

    static func cached(_ url: String) -> PlatformImage? {
        cache.object(forKey: url as NSString)
    }

    /// Returns the image for `url`, or nil when it cannot be fetched or decoded.
    static func load(_ url: String) async -> PlatformImage? {
        guard !url.isEmpty, let parsed = URL(string: url) else { return nil }
        if let hit = cached(url) { return hit }
        if let existing = inFlight[url] { return await existing.value }

        let task = Task<PlatformImage?, Never> { @MainActor in
            defer { inFlight[url] = nil }
            // One retry covers a dropped connection or a throttled request.
            for attempt in 0..<2 {
                switch await fetch(parsed, key: url) {
                case .image(let image):
                    return image
                case .undecodable:
                    // Retrying a decode failure cannot help. Addons sometimes publish
                    // SVG, which UIKit cannot decode.
                    return nil
                case .retryable:
                    if attempt == 0 {
                        try? await Task.sleep(for: .milliseconds(400))
                    }
                }
            }
            return nil
        }
        inFlight[url] = task
        return await task.value
    }

    private enum FetchOutcome {
        case image(PlatformImage)
        case retryable
        case undecodable
    }

    private static func fetch(_ url: URL, key: String) async -> FetchOutcome {
        do {
            let (data, response) = try await session.data(from: url)
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                // Worth knowing about: an addon or a host rejecting artwork is the
                // usual reason a row fills with placeholders.
                DebugLog.write("artwork status \(http.statusCode) \(url.absoluteString)")
                return .retryable
            }
            guard let image = PlatformImage(data: data) else {
                DebugLog.write("artwork undecodable (\(data.count) bytes) \(url.absoluteString)")
                return .undecodable
            }
            cache.setObject(image, forKey: key as NSString)
            return .image(image)
        } catch {
            DebugLog.write("artwork failed \(url.absoluteString) error=\(String(describing: error))")
            return .retryable
        }
    }
}

/// Remote artwork with the shared panel placeholder, so a slow or missing image keeps
/// the layout stable instead of collapsing the row.
///
/// Loading, loaded and failed are three distinct states. They used to share one dark
/// panel, which made a stalled request indistinguishable from artwork that will never
/// arrive — the most confusing way for a row of posters to look.
///
/// The size is defined by a `Color.clear`, never by the image. An aspect-*filled*
/// image reports a size larger than the frame it was given — a 1920×1080 wallpaper
/// filled into a portrait screen reports several thousand points of width — and any
/// container that sizes to its children then grows with it. In a `ZStack`, which
/// centres its children, that silently slides every sibling off screen. Here the
/// overlay is sized to the clear colour and clipped, so it can never affect layout.
struct RemoteImage: View {
    let url: String
    var contentMode: ContentMode = .fill
    /// Spinner size relative to the default. Small tiles need a smaller one.
    var progressScale: CGFloat = 1

    private enum Phase {
        case loading
        case loaded(PlatformImage)
        case failed
    }

    @State private var phase: Phase = .loading

    var body: some View {
        Color.clear
            .overlay {
                switch phase {
                case .loaded(let image):
                    Image(platformImage: image)
                        .resizable()
                        .aspectRatio(contentMode: contentMode)
                case .loading:
                    ProgressView()
                        .progressViewStyle(.circular)
                        .tint(MadariColors.muted)
                        .scaleEffect(progressScale)
                        .accessibilityLabel("Loading image")
                case .failed:
                    // Distinct from loading, so a broken addon URL is visible at a glance.
                    GlyphIcon(glyph: .close, size: 12 * progressScale)
                        .foregroundStyle(MadariColors.muted)
                        .accessibilityLabel("Image unavailable")
                }
            }
            .background(MadariColors.panel)
            .clipped()
            .task(id: url) {
                // A cached image is applied before the first suspension, so scrolling
                // back through a row never flashes a placeholder.
                if let cached = ImageCache.cached(url) {
                    phase = .loaded(cached)
                    return
                }
                if let loaded = await ImageCache.load(url) {
                    // The tile may have been recycled onto another URL while this awaited.
                    if !Task.isCancelled { phase = .loaded(loaded) }
                } else {
                    if !Task.isCancelled { phase = .failed }
                }
            }
    }
}
