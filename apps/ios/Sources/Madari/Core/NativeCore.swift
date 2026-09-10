import Foundation
import MadariCore

/// A failure reported by the Rust core, with the core's own message.
///
/// The generated bindings expose `MadariError` as a public enum, but its
/// `localizedDescription` is a reflection of the case rather than the message the
/// user should read, so this unwraps the associated value instead.
struct CoreError: Error, LocalizedError {
    let message: String

    var errorDescription: String? { message }
}

/// Bridges Swift to the shared Rust core.
///
/// The core owns one Tokio runtime, one profile session, the SQLite store and the
/// torrent engine. Ordinary operations run concurrently because the core
/// deliberately never holds the session lock across network work; torrent reads
/// get their own queue so a slow catalog request cannot stall the player.
final class NativeCore: @unchecked Sendable {
    static let shared = NativeCore()

    private let operations = DispatchQueue(
        label: "media.madari.core",
        qos: .userInitiated,
        attributes: .concurrent
    )
    private let media = DispatchQueue(
        label: "media.madari.media",
        qos: .userInitiated,
        attributes: .concurrent
    )

    private init() {}

    /// Opens the profile database and torrent engine.
    ///
    /// Called once during launch; the core keeps the first store it opens.
    func initialize(storagePath: String) async throws {
        try await run(on: operations) {
            do {
                // Qualified: the generated bindings are free functions with the same
                // names as the methods here.
                try MadariCore.initialize(path: storagePath)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    /// Runs one core operation and returns its decoded JSON result.
    func call(_ operation: String, _ arguments: JSONValue = .object([:])) async throws -> JSONValue {
        let payload = arguments.serialized()
        let result: String = try await run(on: operations) {
            do {
                return try MadariCore.dispatch(operation: operation, arguments: payload)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
        return try JSONValue.parse(result)
    }

    // MARK: - Internal media streaming

    /// Opens a `madari-internal://` source as a seekable reader.
    func openMedia(uri: String, position: Int64) async throws -> Int64 {
        try await run(on: media) {
            do {
                return try MadariCore.openMedia(uri: uri, position: position)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    func mediaLength(handle: Int64) async throws -> Int64 {
        try await run(on: media) {
            do {
                return try MadariCore.mediaLength(handle: handle)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    /// Reads a byte range. `.pending` means the piece is not ready and the caller
    /// should ask again rather than treating it as the end of the stream.
    func readMedia(handle: Int64, position: Int64, length: Int64) async throws -> MediaRead {
        try await run(on: media) {
            do {
                return try MadariCore.readMedia(handle: handle, position: position, length: length)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    func closeMedia(handle: Int64) {
        MadariCore.closeMedia(handle: handle)
    }

    // MARK: - Synchronous media access

    // libmpv reads from its own threads and expects its stream callbacks to block, so
    // those go through these instead of the async wrappers above. They share the media
    // queue, so torrent reads stay ordered against the player's own.

    func openMediaSync(uri: String, position: Int64) throws -> Int64 {
        try media.sync {
            do {
                return try MadariCore.openMedia(uri: uri, position: position)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    func mediaLengthSync(handle: Int64) throws -> Int64 {
        try media.sync {
            do {
                return try MadariCore.mediaLength(handle: handle)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    func readMediaSync(handle: Int64, position: Int64, length: Int64) throws -> MediaRead {
        try media.sync {
            do {
                return try MadariCore.readMedia(handle: handle, position: position, length: length)
            } catch let error as MadariError {
                throw CoreError(message: Self.message(from: error))
            }
        }
    }

    // MARK: - Plumbing

    /// Runs blocking native work off the cooperative thread pool.
    private func run<T: Sendable>(
        on queue: DispatchQueue,
        _ work: @escaping @Sendable () throws -> T
    ) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            queue.async {
                do {
                    continuation.resume(returning: try work())
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private static func message(from error: MadariError) -> String {
        switch error {
        case .Failed(let message): return message
        }
    }
}
