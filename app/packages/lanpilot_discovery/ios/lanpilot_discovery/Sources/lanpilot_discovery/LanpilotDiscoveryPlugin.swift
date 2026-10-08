import Flutter
import Network
import UIKit

/// Browses `_lanpilot._udp` with NWBrowser and streams each service to Dart
/// with its TXT record and one resolved IPv4 address. Dart validates every
/// result with core; nothing here is trusted.
public class LanpilotDiscoveryPlugin: NSObject, FlutterPlugin, FlutterStreamHandler {
  private let queue = DispatchQueue(label: "dev.lanpilot.discovery")
  private var browser: NWBrowser?
  private var resolvers: [String: NWConnection] = [:]
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
    queue.async { self.startBrowsing() }
    return nil
  }

  public func onCancel(withArguments arguments: Any?) -> FlutterError? {
    sink = nil
    queue.async { self.stopBrowsing() }
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
        self.queue.asyncAfter(deadline: .now() + 2) {
          if self.sink != nil { self.startBrowsing() }
        }
      }
    }
    browser.start(queue: queue)
    self.browser = browser
  }

  private func stopBrowsing() {
    browser?.cancel()
    browser = nil
    resolvers.values.forEach { $0.cancel() }
    resolvers.removeAll()
  }

  private static func fullname(_ endpoint: NWEndpoint) -> String? {
    guard case let .service(name, type, domain, _) = endpoint else { return nil }
    let cleanType = type.hasSuffix(".") ? String(type.dropLast()) : type
    let cleanDomain = domain.hasSuffix(".") ? domain : domain + "."
    return "\(name).\(cleanType).\(cleanDomain)"
  }

  /// A UDP NWConnection to the service resolves it; nothing is sent.
  private func resolve(_ result: NWBrowser.Result) {
    guard let fullname = Self.fullname(result.endpoint) else { return }
    var txt: [String: String] = [:]
    if case let .bonjour(record) = result.metadata { txt = record.dictionary }
    resolvers[fullname]?.cancel()
    let params = NWParameters.udp
    if let ip = params.defaultProtocolStack.internetProtocol as? NWProtocolIP.Options {
      ip.version = .v4
    }
    let connection = NWConnection(to: result.endpoint, using: params)
    resolvers[fullname] = connection
    connection.stateUpdateHandler = { [weak self, weak connection] state in
      guard let self, let connection else { return }
      switch state {
      case .ready:
        if case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint,
          case let .ipv4(address) = host
        {
          let text = "\(address)".split(separator: "%").first.map(String.init) ?? "\(address)"
          self.emit([
            "event": "found", "fullname": fullname, "txt": txt,
            "addrs": [text], "port": Int(port.rawValue),
          ])
        }
        connection.cancel()
      case .failed, .cancelled:
        if self.resolvers[fullname] === connection {
          self.resolvers.removeValue(forKey: fullname)
        }
      default:
        break
      }
    }
    connection.start(queue: queue)
  }

  private func lost(_ result: NWBrowser.Result) {
    guard let fullname = Self.fullname(result.endpoint) else { return }
    resolvers.removeValue(forKey: fullname)?.cancel()
    emit(["event": "lost", "fullname": fullname])
  }

  private func emit(_ event: [String: Any]) {
    DispatchQueue.main.async { [weak self] in self?.sink?(event) }
  }
}
