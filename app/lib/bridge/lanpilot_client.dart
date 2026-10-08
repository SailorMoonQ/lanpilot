import 'dart:typed_data';

import 'generated/api/bridge.dart' as rust;
import 'generated/api/types.dart';

export 'generated/api/types.dart';

/// Everything the app needs from the Rust bridge. The app talks to this
/// interface only, so tests can use a fake (spec 3).
abstract class LanPilotClient {
  Uint8List generateSecret();

  /// Creates the bridge client; returns this phone's short id.
  String initClient({
    required Uint8List secret,
    required String deviceName,
    required String appVersion,
  });

  DiscoveredInfo? validateDiscovered({
    required String fullname,
    required Map<String, String> txt,
    required List<String> addrs,
    required int port,
  });

  Future<PairedServerInfo> pairWithUri(String uri);

  Future<PairedServerInfo> pairWithPassword({
    required String addr,
    required String password,
  });

  Future<SessionInfo> connect({
    required String serverKeyHex,
    required List<String> candidates,
  });

  Future<void> disconnect();

  Future<void> resetEndpoint();

  void beginGesture();

  /// Pointer motion in pixels, added to the current gesture.
  void sendPointer(double dx, double dy);

  /// Scroll in notches, added to the current gesture.
  void sendScroll(double dx, double dy);

  Future<void> button(MouseButtonKind button, {required bool down});

  Future<void> keyChord(List<int> usages);

  Future<void> media(MediaKind action);

  Future<void> zoom(double steps);

  Future<void> unpair();

  /// One event whenever a session ends, for any reason.
  Stream<ConnectionEvent> events();
}

class RustLanPilotClient implements LanPilotClient {
  Stream<ConnectionEvent>? _events;

  @override
  Uint8List generateSecret() => rust.generateSecret();

  @override
  String initClient({
    required Uint8List secret,
    required String deviceName,
    required String appVersion,
  }) => rust.initClient(
    secret: secret,
    deviceName: deviceName,
    appVersion: appVersion,
  );

  @override
  DiscoveredInfo? validateDiscovered({
    required String fullname,
    required Map<String, String> txt,
    required List<String> addrs,
    required int port,
  }) => rust.validateDiscovered(
    fullname: fullname,
    txt: txt,
    addrs: addrs,
    port: port,
  );

  @override
  Future<PairedServerInfo> pairWithUri(String uri) =>
      rust.pairWithUri(uri: uri);

  @override
  Future<PairedServerInfo> pairWithPassword({
    required String addr,
    required String password,
  }) => rust.pairWithPassword(addr: addr, password: password);

  @override
  Future<SessionInfo> connect({
    required String serverKeyHex,
    required List<String> candidates,
  }) => rust.connect(serverKeyHex: serverKeyHex, candidates: candidates);

  @override
  Future<void> disconnect() => rust.disconnect();

  @override
  Future<void> resetEndpoint() => rust.resetEndpoint();

  @override
  void beginGesture() => rust.beginGesture();

  @override
  void sendPointer(double dx, double dy) => rust.sendPointer(dx: dx, dy: dy);

  @override
  void sendScroll(double dx, double dy) => rust.sendScroll(dx: dx, dy: dy);

  @override
  Future<void> button(MouseButtonKind button, {required bool down}) =>
      rust.button(button: button, down: down);

  @override
  Future<void> keyChord(List<int> usages) =>
      rust.keyChord(usages: Uint32List.fromList(usages));

  @override
  Future<void> media(MediaKind action) => rust.media(action: action);

  @override
  Future<void> zoom(double steps) => rust.zoom(steps: steps);

  @override
  Future<void> unpair() => rust.unpair();

  @override
  Stream<ConnectionEvent> events() =>
      _events ??= rust.connectionEvents().asBroadcastStream();
}
