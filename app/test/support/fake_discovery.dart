import 'dart:async';

import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/discovery_lookup.dart';

DiscoveredInfo fakeDiscovered({
  String shortId = '0123456789abcdef',
  String name = 'Desk',
  List<String> addrs = const ['192.168.1.20:45810'],
}) => DiscoveredInfo(
  shortId: shortId,
  name: name,
  os: OsKind.windows,
  protoMin: 1,
  protoMax: 1,
  addrs: addrs,
);

class FakeDiscovery implements DiscoveryLookup {
  final found = <String, DiscoveredInfo>{};
  final _waiters = <String, List<Completer<DiscoveredInfo?>>>{};

  @override
  DiscoveredInfo? lookup(String shortId) => found[shortId];

  @override
  Future<DiscoveredInfo?> waitFor(String shortId, Duration timeout) {
    final now = found[shortId];
    if (now != null) return Future.value(now);
    final c = Completer<DiscoveredInfo?>();
    _waiters.putIfAbsent(shortId, () => []).add(c);
    Timer(timeout, () {
      if (!c.isCompleted) c.complete(null);
    });
    return c.future;
  }

  void announce(DiscoveredInfo info) {
    found[info.shortId] = info;
    for (final c
        in _waiters.remove(info.shortId) ?? <Completer<DiscoveredInfo?>>[]) {
      if (!c.isCompleted) c.complete(info);
    }
  }
}
