import 'package:flutter/services.dart';

/// One raw Bonjour result from NWBrowser. Untrusted: validate it with core
/// before using anything in it.
class RawService {
  const RawService({
    required this.found,
    required this.fullname,
    this.txt = const {},
    this.addrs = const [],
    this.port = 0,
  });

  final bool found;
  final String fullname;
  final Map<String, String> txt;
  final List<String> addrs;
  final int port;

  /// Parses one event map from the platform; null for anything else.
  static RawService? fromEvent(Object? event) {
    if (event is! Map) return null;
    final kind = event['event'];
    final fullname = event['fullname'];
    if (fullname is! String) return null;
    if (kind == 'lost') return RawService(found: false, fullname: fullname);
    if (kind != 'found') return null;
    final txt = <String, String>{};
    final rawTxt = event['txt'];
    if (rawTxt is Map) {
      rawTxt.forEach((k, v) {
        if (k is String && v is String) txt[k] = v;
      });
    }
    final rawAddrs = event['addrs'];
    final port = event['port'];
    return RawService(
      found: true,
      fullname: fullname,
      txt: txt,
      addrs: [
        if (rawAddrs is List)
          for (final a in rawAddrs)
            if (a is String) a,
      ],
      port: port is int ? port : 0,
    );
  }
}

abstract final class LanpilotDiscovery {
  static const _channel = EventChannel('lanpilot/discovery');

  /// Browses `_lanpilot._udp` while listened to.
  static Stream<RawService> events() => _channel
      .receiveBroadcastStream()
      .map(RawService.fromEvent)
      .where((s) => s != null)
      .cast<RawService>();
}
