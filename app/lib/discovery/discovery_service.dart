import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:lanpilot_discovery/lanpilot_discovery.dart';

import '../bridge/lanpilot_client.dart';
import '../connection/discovery_lookup.dart';

/// Computers announcing `_lanpilot._udp`, validated by core and keyed by
/// short id (spec 3.2).
class DiscoveryService implements DiscoveryLookup {
  DiscoveryService(this._events, this._validate);

  final Stream<RawService> Function() _events;
  final DiscoveredInfo? Function(RawService raw) _validate;

  final ValueNotifier<Map<String, DiscoveredInfo>> devices = ValueNotifier(
    const {},
  );
  final _idByFullname = <String, String>{};
  StreamSubscription<RawService>? _sub;

  void start() {
    _sub ??= _events().listen(_onEvent, onError: (Object _) {});
  }

  Future<void> stop() async {
    await _sub?.cancel();
    _sub = null;
  }

  void _onEvent(RawService raw) {
    if (raw.found) {
      final info = _validate(raw);
      if (info == null) return;
      _idByFullname[raw.fullname] = info.shortId;
      devices.value = {...devices.value, info.shortId: info};
    } else {
      final id = _idByFullname.remove(raw.fullname);
      if (id == null) return;
      devices.value = {...devices.value}..remove(id);
    }
  }

  @override
  DiscoveredInfo? lookup(String shortId) => devices.value[shortId];

  @override
  Future<DiscoveredInfo?> waitFor(String shortId, Duration timeout) {
    final now = lookup(shortId);
    if (now != null) return Future.value(now);
    final done = Completer<DiscoveredInfo?>();
    void check() {
      final found = lookup(shortId);
      if (found != null && !done.isCompleted) done.complete(found);
    }

    devices.addListener(check);
    final timer = Timer(timeout, () {
      if (!done.isCompleted) done.complete(null);
    });
    return done.future.whenComplete(() {
      devices.removeListener(check);
      timer.cancel();
    });
  }
}
