import Flutter
import Network
import UIKit
import dnssd

/// Browses `_lanpilot._udp` with NWBrowser, resolves each service with dns_sd
/// and streams it to Dart with its TXT record and one resolved IPv4 address.
/// Dart validates every result with core; nothing here is trusted.
public class LanpilotDiscoveryPlugin: NSObject, FlutterPlugin, FlutterStreamHandler {
  private static let maxAttempts = 3
  private static let resolveTimeout: TimeInterval = 5
  private static let retryDelay: TimeInterval = 2

  private let queue = DispatchQueue(label: "dev.lanpilot.discovery")

  // Owned by `queue`.
  private var wantsBrowsing = false
  private var browser: NWBrowser?
  private var results: [String: NWBrowser.Result] = [:]
  private var resolvers: [String: Resolver] = [:]
  private var attempts: [String: Int] = [:]
  private var retries: [String: DispatchWorkItem] = [:]
  private var emitted: Set<String> = []

  // Owned by the main thread.
  private var sink: FlutterEventSink?

  public static func register(with registrar: FlutterPluginRegistrar) {
    let channel = FlutterEventChannel(
      name: "lanpilot/discovery", binaryMessenger: registrar.messenger())
    channel.setStreamHandler(LanpilotDiscoveryPlugin())
  }

  public func onListen(
    withArguments arguments: Any?, eventSink events: @escaping FlutterEventSink
  ) -> FlutterError? {
    sink = events
    queue.async {
      self.wantsBrowsing = true
      self.startBrowsing()
    }
    return nil
  }

  public func onCancel(withArguments arguments: Any?) -> FlutterError? {
    sink = nil
    queue.async {
      self.wantsBrowsing = false
      self.stopBrowsing()
    }
    return nil
  }

  private func startBrowsing() {
    stopBrowsing()
    let browser = NWBrowser(
      for: .bonjourWithTXTRecord(type: "_lanpilot._udp", domain: "local."),
      using: NWParameters())
    browser.browseResultsChangedHandler = { [weak self] _, changes in
      guard let self else { return }
      for change in changes {
        switch change {
        case .added(let result): self.resolve(result)
        case .changed(old: _, new: let result, flags: _): self.resolve(result)
        case .removed(let result): self.lost(result)
        case .identical: break
        @unknown default: break
        }
      }
    }
    browser.stateUpdateHandler = { [weak self] state in
      guard let self else { return }
      if case .failed(let error) = state {
        self.emit(["event": "error", "message": "\(error)"])
        self.queue.asyncAfter(deadline: .now() + Self.retryDelay) {
          if self.wantsBrowsing { self.startBrowsing() }
        }
      }
    }
    browser.start(queue: queue)
    self.browser = browser
  }

  /// Stops everything and tells Dart that every announced service is gone.
  private func stopBrowsing() {
    browser?.cancel()
    browser = nil
    resolvers.values.forEach { $0.cancel() }
    resolvers.removeAll()
    retries.values.forEach { $0.cancel() }
    retries.removeAll()
    results.removeAll()
    attempts.removeAll()
    for fullname in emitted {
      emit(["event": "lost", "fullname": fullname])
    }
    emitted.removeAll()
  }

  private static func fullname(_ endpoint: NWEndpoint) -> String? {
    guard case let .service(name, type, domain, _) = endpoint else { return nil }
    let cleanType = type.hasSuffix(".") ? String(type.dropLast()) : type
    let cleanDomain = domain.hasSuffix(".") ? domain : domain + "."
    return "\(name).\(cleanType).\(cleanDomain)"
  }

  /// A new or changed browse result: resolve it with a fresh attempt budget.
  private func resolve(_ result: NWBrowser.Result) {
    guard let fullname = Self.fullname(result.endpoint) else { return }
    results[fullname] = result
    attempts[fullname] = 0
    retries.removeValue(forKey: fullname)?.cancel()
    attempt(fullname)
  }

  /// Resolves the service with dns_sd, as `dns-sd -L` and `dns-sd -G` do:
  /// DNSServiceResolve for host and port, then DNSServiceGetAddrInfo for an
  /// IPv4 address. Both run on `queue`.
  private func attempt(_ fullname: String) {
    guard let result = results[fullname],
      case let .service(name, type, domain, _) = result.endpoint
    else { return }
    resolvers.removeValue(forKey: fullname)?.cancel()
    let resolver = Resolver(fullname: fullname, owner: self)
    resolvers[fullname] = resolver
    var ref: DNSServiceRef?
    let status = DNSServiceResolve(
      &ref, 0, 0, name, type, domain,
      { _, _, _, error, _, host, port, _, _, context in
        guard let context else { return }
        let resolver = Unmanaged<Resolver>.fromOpaque(context).takeUnretainedValue()
        resolver.owner?.serviceResolved(
          resolver, error, host.map { String(cString: $0) } ?? "", UInt16(bigEndian: port))
      },
      Unmanaged.passUnretained(resolver).toOpaque())
    guard status == kDNSServiceErr_NoError, let ref else {
      failed(fullname, resolver)
      return
    }
    resolver.service = ref
    guard DNSServiceSetDispatchQueue(ref, queue) == kDNSServiceErr_NoError else {
      failed(fullname, resolver)
      return
    }
    queue.asyncAfter(deadline: .now() + Self.resolveTimeout) { [weak self, weak resolver] in
      guard let self, let resolver, self.resolvers[fullname] === resolver else { return }
      self.failed(fullname, resolver)
    }
  }

  /// The SRV record arrived: look up the host's IPv4 address.
  private func serviceResolved(
    _ resolver: Resolver, _ error: DNSServiceErrorType, _ host: String, _ port: UInt16
  ) {
    let fullname = resolver.fullname
    guard resolvers[fullname] === resolver else { return }
    guard error == kDNSServiceErr_NoError else {
      failed(fullname, resolver)
      return
    }
    // DNSServiceResolve reports the record once per interface; one lookup is enough.
    guard resolver.address == nil else { return }
    resolver.port = port
    // Interface 0 on purpose: the SRV answer can come from an interface that
    // has no A record for the host (an agent on this Mac answers SRV on lo0
    // but A only on en0), and a lookup scoped to it never completes.
    var ref: DNSServiceRef?
    let status = DNSServiceGetAddrInfo(
      &ref, 0, 0, DNSServiceProtocol(kDNSServiceProtocol_IPv4), host,
      { _, flags, _, error, _, address, _, context in
        guard let context else { return }
        let resolver = Unmanaged<Resolver>.fromOpaque(context).takeUnretainedValue()
        let added = flags & DNSServiceFlags(kDNSServiceFlagsAdd) != 0
        resolver.owner?.addressResolved(
          resolver, error, added ? address.flatMap(LanpilotDiscoveryPlugin.ipv4) : nil)
      },
      Unmanaged.passUnretained(resolver).toOpaque())
    guard status == kDNSServiceErr_NoError, let ref else {
      failed(fullname, resolver)
      return
    }
    resolver.address = ref
    guard DNSServiceSetDispatchQueue(ref, queue) == kDNSServiceErr_NoError else {
      failed(fullname, resolver)
      return
    }
  }

  /// An address arrived: announce the service with it.
  private func addressResolved(
    _ resolver: Resolver, _ error: DNSServiceErrorType, _ address: String?
  ) {
    let fullname = resolver.fullname
    guard resolvers[fullname] === resolver else { return }
    guard error == kDNSServiceErr_NoError else {
      failed(fullname, resolver)
      return
    }
    guard let address, let result = results[fullname] else { return }
    var txt: [String: String] = [:]
    if case let .bonjour(record) = result.metadata { txt = record.dictionary }
    resolvers.removeValue(forKey: fullname)
    resolver.cancel()
    attempts[fullname] = 0
    emitted.insert(fullname)
    emit([
      "event": "found", "fullname": fullname, "txt": txt,
      "addrs": [address], "port": Int(resolver.port),
    ])
  }

  private static func ipv4(_ address: UnsafePointer<sockaddr>) -> String? {
    guard address.pointee.sa_family == sa_family_t(AF_INET) else { return nil }
    var addr = address.withMemoryRebound(to: sockaddr_in.self, capacity: 1) {
      $0.pointee.sin_addr
    }
    var buffer = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
    guard inet_ntop(AF_INET, &addr, &buffer, socklen_t(buffer.count)) != nil else { return nil }
    return String(cString: buffer)
  }

  /// Gives up on this attempt and retries after a delay, a bounded number of times.
  private func failed(_ fullname: String, _ resolver: Resolver) {
    guard resolvers[fullname] === resolver else { return }
    resolvers.removeValue(forKey: fullname)
    resolver.cancel()
    let count = (attempts[fullname] ?? 0) + 1
    attempts[fullname] = count
    guard count < Self.maxAttempts else { return }
    let work = DispatchWorkItem { [weak self] in
      guard let self else { return }
      self.retries.removeValue(forKey: fullname)
      self.attempt(fullname)
    }
    retries[fullname] = work
    queue.asyncAfter(deadline: .now() + Self.retryDelay, execute: work)
  }

  private func lost(_ result: NWBrowser.Result) {
    guard let fullname = Self.fullname(result.endpoint) else { return }
    results.removeValue(forKey: fullname)
    attempts.removeValue(forKey: fullname)
    retries.removeValue(forKey: fullname)?.cancel()
    resolvers.removeValue(forKey: fullname)?.cancel()
    emitted.remove(fullname)
    emit(["event": "lost", "fullname": fullname])
  }

  private func emit(_ event: [String: Any]) {
    DispatchQueue.main.async { [weak self] in self?.sink?(event) }
  }
}

/// One resolution attempt: the DNSServiceRefs it owns and the port it found.
/// Used and cancelled only on the plugin's queue, so no callback arrives after
/// `cancel()`.
private final class Resolver {
  let fullname: String
  weak var owner: LanpilotDiscoveryPlugin?
  var service: DNSServiceRef?
  var address: DNSServiceRef?
  var port: UInt16 = 0

  init(fullname: String, owner: LanpilotDiscoveryPlugin) {
    self.fullname = fullname
    self.owner = owner
  }

  func cancel() {
    if let address { DNSServiceRefDeallocate(address) }
    if let service { DNSServiceRefDeallocate(service) }
    address = nil
    service = nil
  }
}
