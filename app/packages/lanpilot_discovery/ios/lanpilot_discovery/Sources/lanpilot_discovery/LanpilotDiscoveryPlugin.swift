import Flutter
import Network
import UIKit

/// Browses `_lanpilot._udp` with NWBrowser and streams each service to Dart
/// with its TXT record and one resolved IPv4 address. Dart validates every
/// result with core; nothing here is trusted.
public class LanpilotDiscoveryPlugin: NSObject, FlutterPlugin, FlutterStreamHandler {
  private static let maxAttempts = 3
  private static let resolveTimeout: TimeInterval = 5
  private static let retryDelay: TimeInterval = 2

  private let queue = DispatchQueue(label: "dev.lanpilot.discovery")

  // Owned by `queue`.
  private var wantsBrowsing = false
  private var browser: NWBrowser?
  private var results: [String: NWBrowser.Result] = [:]
  private var resolvers: [String: NWConnection] = [:]
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

  /// A UDP NWConnection to the service resolves it; nothing is sent.
  private func attempt(_ fullname: String) {
    guard let result = results[fullname] else { return }
    var txt: [String: String] = [:]
    if case let .bonjour(record) = result.metadata { txt = record.dictionary }
    resolvers.removeValue(forKey: fullname)?.cancel()
    let params = NWParameters.udp
    if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
      ip.version = .v4
    }
    let connection = NWConnection(to: result.endpoint, using: params)
    resolvers[fullname] = connection
    connection.stateUpdateHandler = { [weak self, weak connection] state in
      guard let self, let connection, self.resolvers[fullname] === connection else { return }
      switch state {
      case .ready:
        if case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint,
          case let .ipv4(address) = host
        {
          let text = "\(address)".split(separator: "%").first.map(String.init) ?? "\(address)"
          self.resolvers.removeValue(forKey: fullname)
          self.attempts[fullname] = 0
          self.emitted.insert(fullname)
          self.emit([
            "event": "found", "fullname": fullname, "txt": txt,
            "addrs": [text], "port": Int(port.rawValue),
          ])
          connection.cancel()
        } else {
          self.failed(fullname, connection)
        }
      case .failed, .waiting:
        self.failed(fullname, connection)
      default:
        break
      }
    }
    connection.start(queue: queue)
    queue.asyncAfter(deadline: .now() + Self.resolveTimeout) { [weak self, weak connection] in
      guard let self, let connection, self.resolvers[fullname] === connection else { return }
      self.failed(fullname, connection)
    }
  }

  /// Gives up on this connection and retries after a delay, a bounded number of times.
  private func failed(_ fullname: String, _ connection: NWConnection) {
    guard resolvers[fullname] === connection else { return }
    resolvers.removeValue(forKey: fullname)
    connection.cancel()
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
