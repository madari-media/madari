import Foundation

#if DEBUG

/// A small append-only trace written inside the app container.
///
/// There is no debugger console on the Linux host, and the device's unified log does
/// not surface this app's `NSLog` output, so a real device session is inspected by
/// pulling this file:
///
/// ```bash
/// pymobiledevice3 apps pull <bundle-id> \
///   "Library/Application Support/madari/debug.log" /tmp/debug.log
/// ```
///
/// Release builds compile every call away.
enum DebugLog {
    private static let queue = DispatchQueue(label: "media.madari.debug-log")

    private static let url: URL? = {
        guard let base = try? FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        ) else { return nil }
        let directory = base.appendingPathComponent("madari", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appendingPathComponent("debug.log", isDirectory: false)
    }()

    private static let formatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss.SSS"
        formatter.locale = Locale(identifier: "en_US_POSIX")
        return formatter
    }()

    /// Appends one line. Writes are serialized and never throw at the call site:
    /// a diagnostic must not be able to break the screen it is describing.
    static func write(_ message: String) {
        guard let url else { return }
        let line = "\(formatter.string(from: Date())) \(message)\n"
        queue.async {
            let data = Data(line.utf8)
            if let handle = try? FileHandle(forWritingTo: url) {
                defer { try? handle.close() }
                _ = try? handle.seekToEnd()
                try? handle.write(contentsOf: data)
            } else {
                try? data.write(to: url, options: .atomic)
            }
        }
    }

    /// Starts a fresh trace. Called once at launch so a session is not mixed with the
    /// previous one.
    static func reset() {
        guard let url else { return }
        queue.async {
            try? FileManager.default.removeItem(at: url)
        }
    }
}

#else

enum DebugLog {
    static func write(_ message: String) {}
    static func reset() {}
}

#endif
