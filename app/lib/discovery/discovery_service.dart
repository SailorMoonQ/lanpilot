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
    final sub = _sub;
    _sub = null;
    _idByFullname.clear();
    devices.value = const {};
    await sub?.cancel();
  }

  void _onEvent(RawService raw) {
    if (raw.found) {
      final info = _validate(raw);
      if (info == null) {
        _forget(raw.fullname);
        return;
      }
      final previous = _idByFullname[raw.fullname];
      _idByFullname[raw.fullname] = info.shortId;
      final next = {...devices.value};
      if (previous != null && previous != info.shortId) {
        _removeIfUnclaimed(next, previous);
      }
      next[info.shortId] = info;
      devices.value = next;
    } else {
      _forget(raw.fullname);
    }
  }

  /// Drops a fullname, and its device unless another fullname still claims it.
  void _forget(String fullname) {
    final id = _idByFullname.remove(fullname);
    if (id == null) return;
    final next = {...devices.value};
    _removeIfUnclaimed(next, id);
    devices.value = next;
  }

  void _removeIfUnclaimed(Map<String, DiscoveredInfo> map, String id) {
    if (!_idByFullname.containsValue(id)) map.remove(id);
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
