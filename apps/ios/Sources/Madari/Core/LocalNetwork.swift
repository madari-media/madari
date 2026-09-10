import Foundation

#if canImport(Darwin)
import Darwin
#endif

/// Local network helpers for the web settings address.
enum LocalNetwork {
    /// The device's own IPv4 address on the active interface.
    ///
    /// The core binds `0.0.0.0` and reports that it is running, but the address a
    /// browser should open is the phone's own, so it is read here rather than hard
    /// coded into the core's response.
    static func ipv4Address() -> String? {
        var head: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&head) == 0, let first = head else { return nil }
        defer { freeifaddrs(head) }

        var result: String?
        var cursor: UnsafeMutablePointer<ifaddrs>? = first
        while let entry = cursor {
            let flags = Int32(entry.pointee.ifa_flags)
            let isUp = (flags & IFF_UP) == IFF_UP
            let isLoopback = (flags & IFF_LOOPBACK) == IFF_LOOPBACK
            if isUp, !isLoopback, entry.pointee.ifa_addr.pointee.sa_family == UInt8(AF_INET) {
                var address = entry.pointee.ifa_addr.pointee
                var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
                let code = getnameinfo(
                    &address, socklen_t(entry.pointee.ifa_addr.pointee.sa_len),
                    &host, socklen_t(host.count),
                    nil, 0, NI_NUMERICHOST
                )
                if code == 0 {
                    // `String(cString:)` is deprecated; decode the buffer up to its
                    // terminator instead.
                    let value = host.withUnsafeBufferPointer { buffer in
                        String(decoding: buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
                    }
                    // Prefer the usual private ranges over anything unexpected, but
                    // accept whatever the active interface has.
                    if result == nil || value.hasPrefix("192.168.") || value.hasPrefix("10.") {
                        result = value
                    }
                }
            }
            cursor = entry.pointee.ifa_next
        }
        return result
    }
}
