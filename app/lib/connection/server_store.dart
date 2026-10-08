import 'package:flutter/foundation.dart';

import '../storage/json_store.dart';
import '../storage/secret_store.dart';
import 'paired_server.dart';

/// Paired computers (spec 3.3): the list in a JSON file, each public key in
/// the keychain. An entry whose key is missing is dropped on load.
class ServerStore {
  ServerStore(this._json, this._secrets, {DateTime Function()? now})
    : _now = now ?? DateTime.now;

  final JsonStore _json;
  final SecretStore _secrets;
  final DateTime Function() _now;

  /// Most recently used first; never-used last.
  final ValueNotifier<List<PairedServer>> servers = ValueNotifier(const []);
  bool _everConnected = false;

  /// True once any computer was reached since install (spec 4.4).
  bool get everConnected => _everConnected;

  PairedServer? get lastUsed => servers.value.firstOrNull;

  PairedServer? byId(String shortId) =>
      servers.value.where((s) => s.shortId == shortId).firstOrNull;

  static String keyName(String shortId) => 'server_key_$shortId';

  Future<void> load() async {
    final data = await _json.read();
    _everConnected = data['everConnected'] == true;
    final raw = data['servers'];
    final loaded = <PairedServer>[];
    if (raw is List) {
      for (final item in raw) {
        if (item is! Map) continue;
        final json = item.cast<String, Object?>();
        final id = json['shortId'];
        if (id is! String) continue;
        final key = await _secrets.read(keyName(id));
        if (key == null) continue;
        final server = PairedServer.fromJson(json, key);
        if (server != null) loaded.add(server);
      }
    }
    _publish(loaded);
  }

  Future<void> upsert(PairedServer server) async {
    await _secrets.write(keyName(server.shortId), server.publicKeyHex);
    _publish([
      for (final s in servers.value)
        if (s.shortId != server.shortId) s,
      server,
    ]);
    await _save();
  }

  /// Marks a computer as just used; `goodAddr` is the address that answered.
  Future<PairedServer?> markUsed(String shortId, {String? goodAddr}) async {
    final current = byId(shortId);
    if (current == null) return null;
    final updated = current.copyWith(lastUsed: _now(), lastGoodAddr: goodAddr);
    if (goodAddr != null) _everConnected = true;
    _publish([
      for (final s in servers.value) s.shortId == shortId ? updated : s,
    ]);
    await _save();
    return updated;
  }

  Future<void> remove(String shortId) async {
    _publish([
      for (final s in servers.value)
        if (s.shortId != shortId) s,
    ]);
    await _secrets.delete(keyName(shortId));
    await _save();
  }

  void _publish(List<PairedServer> list) {
    final sorted = [...list]..sort(_byRecency);
    servers.value = List.unmodifiable(sorted);
  }

  static int _byRecency(PairedServer a, PairedServer b) {
    final x = a.lastUsed;
    final y = b.lastUsed;
    if (x == null && y == null) return a.name.compareTo(b.name);
    if (x == null) return 1;
    if (y == null) return -1;
    return y.compareTo(x);
  }

  Future<void> _save() => _json.write({
    'everConnected': _everConnected,
    'servers': [for (final s in servers.value) s.toJson()],
  });
}
