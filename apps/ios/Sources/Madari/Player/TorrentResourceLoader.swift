import AVFoundation
import Foundation
import UniformTypeIdentifiers

/// Streams an internal `madari-internal://` source into AVFoundation.
///
/// AVPlayer cannot open a torrent, so the asset is created with the internal URI and
/// this delegate answers the byte-range requests AVFoundation makes. That is the iOS
/// counterpart of Media3's data source on Android: bytes stay in the process, and the
/// only thing crossing the boundary is a range read.
///
/// The Rust reader is a real seekable stream, so each request seeks before reading
/// instead of assuming sequential access.
final class TorrentResourceLoader: NSObject, AVAssetResourceLoaderDelegate, @unchecked Sendable {
    /// AVFoundation hands the request to a delegate queue; boxing it lets one task
    /// own it for the duration of the read without tripping strict concurrency.
    private final class Box: @unchecked Sendable {
        let request: AVAssetResourceLoadingRequest
        init(_ request: AVAssetResourceLoadingRequest) { self.request = request }
    }

    private let uri: String
    private let contentType: String
    private let core = NativeCore.shared

    private var handle: Int64?
    private var length: Int64 = 0
    private var opening: Task<(handle: Int64, length: Int64), Error>?

    /// AVFoundation asks for these before it will play a custom-scheme asset.
    private static let chunkSize: Int64 = 256 * 1024
    /// Matches the TV client: two minutes without data means the peers are gone.
    private static let pendingTimeout: TimeInterval = 120
    private static let pendingRetry: Duration = .milliseconds(200)

    init(uri: String, contentType: String) {
        self.uri = uri
        self.contentType = contentType
        super.init()
    }

    deinit {
        if let handle { core.closeMedia(handle: handle) }
    }

    func resourceLoader(
        _ resourceLoader: AVAssetResourceLoader,
        shouldWaitForLoadingOfRequestedResource loadingRequest: AVAssetResourceLoadingRequest
    ) -> Bool {
        let box = Box(loadingRequest)
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.fulfill(box.request)
            } catch {
                box.request.finishLoading(with: error)
            }
        }
        return true
    }

    func resourceLoader(
        _ resourceLoader: AVAssetResourceLoader,
        didCancel loadingRequest: AVAssetResourceLoadingRequest
    ) {
        // The read loop checks `isCancelled`; nothing else to release per request.
    }

    // MARK: - Request handling

    private func fulfill(_ request: AVAssetResourceLoadingRequest) async throws {
        // A content-information request asks how big the resource is and what it is.
        if let information = request.contentInformationRequest {
            let total = try await openOnce()
            information.contentType = contentType
            information.contentLength = total
            information.isByteRangeAccessSupported = true
            request.finishLoading()
            return
        }
        guard let dataRequest = request.dataRequest else {
            request.finishLoading()
            return
        }
        let handle = try await openOnce()
        let total = length

        var offset = dataRequest.currentOffset != 0 ? dataRequest.currentOffset : dataRequest.requestedOffset
        // A request for "the rest of the file" is answered in bounded reads so a
        // long stream does not try to buffer the whole title at once.
        let requestedEnd = dataRequest.requestedLength > 0
            ? offset + Int64(dataRequest.requestedLength)
            : total
        let deadline = Date().addingTimeInterval(Self.pendingTimeout)

        while offset < min(requestedEnd, total) {
            if request.isCancelled { return }
            let want = min(Self.chunkSize, min(requestedEnd, total) - offset)
            switch try await core.readMedia(handle: handle, position: offset, length: want) {
            case .data(let bytes):
                if bytes.isEmpty { break }
                dataRequest.respond(with: bytes)
                offset += Int64(bytes.count)
            case .eof:
                offset = total
            case .pending:
                // The torrent has not produced this piece yet. Retrying is correct;
                // treating it as end of stream would truncate playback.
                if Date() >= deadline {
                    throw PlayerError.torrentStalled
                }
                try? await Task.sleep(for: Self.pendingRetry)
            }
        }
        request.finishLoading()
    }

    /// Opens the reader once. Concurrent requests share the same handle, because the
    /// core's reader registry is keyed by handle and seeks per read.
    private func openOnce() async throws -> Int64 {
        if let handle { return handle }
        // A concurrent request joins the same open and reuses its handle.
        if let opening { return try await opening.value.handle }
        // The open task carries both values, so the length is read once rather than
        // fetched again after the handle is known.
        let task = Task<(handle: Int64, length: Int64), Error> { [core, uri] in
            let handle = try await core.openMedia(uri: uri, position: 0)
            let length = try await core.mediaLength(handle: handle)
            return (handle, length)
        }
        opening = task
        do {
            let opened = try await task.value
            self.handle = opened.handle
            length = opened.length
            return opened.handle
        } catch {
            opening = nil
            throw error
        }
    }
}

enum PlayerError: Error, LocalizedError {
    case torrentStalled
    case unsupportedFormat(String)

    var errorDescription: String? {
        switch self {
        case .torrentStalled:
            "No torrent data arrived for two minutes. Peers may be unavailable; try another source."
        case .unsupportedFormat(let detail):
            detail
        }
    }
}

/// Content types for the container the source delivers.
enum MediaFormat {
    /// Extensions AVFoundation cannot open at all. libmpv decodes every one of them, so
    /// these select the libmpv backend instead of being a dead end.
    static let mpvOnlyExtensions: Set<String> = [
        "mkv", "webm", "avi", "wmv", "flv", "ts", "m2ts", "mpg", "mpeg", "ogv",
    ]

    /// Whether this source has to be played by libmpv rather than AVPlayer.
    static func requiresMpv(_ playback: Playback) -> Bool {
        mpvOnlyExtensions.contains(extensionOf(playback))
    }

    /// The MIME type AVFoundation needs for this file, and whether it is a container
    /// AVPlayer can decode at all.
    ///
    /// AVPlayer decodes MP4/MOV/M4V/HLS and audio natively; Matroska, WebM and AVI,
    /// which is what most torrent releases use, go to the libmpv backend. The warning
    /// survives only as a safety net for a container that reaches AVPlayer anyway.
    static func inspect(_ playback: Playback) -> (mime: String, warning: String?) {
        let ext = extensionOf(playback)
        let mime = UTType(filenameExtension: ext)?.preferredMIMEType ?? "application/octet-stream"
        if mpvOnlyExtensions.contains(ext) {
            return (mime, "This source is a .\(ext) file, which the iOS player cannot decode. Choose a different source, or a title offered as MP4.")
        }
        return (mime, nil)
    }

    private static func extensionOf(_ playback: Playback) -> String {
        (fileName(playback) as NSString).pathExtension.lowercased()
    }

    /// The torrent's file name when known, otherwise the URL's last path component.
    private static func fileName(_ playback: Playback) -> String {
        if let name = playback.delivery["file"]?["name"]?.stringValue.nilIfEmpty { return name }
        if let url = URL(string: playback.delivery.text("url")), !url.lastPathComponent.isEmpty {
            return url.lastPathComponent
        }
        return playback.source.displayName
    }
}
