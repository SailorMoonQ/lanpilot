import 'dart:async';
import 'dart:typed_data';

import 'package:lanpilot/bridge/lanpilot_client.dart';

BridgeError bridgeError(ErrorKind kind, [String message = 'fake']) =>
    BridgeError(kind: kind, message: message, retryAfterSecs: 0);

SessionInfo fakeSession(
  int generation, {
  List<String> capabilities = const ['zoom'],
  String addr = '192.168.1.10:45810',
}) => SessionInfo(
  generation: generation,
  serverName: 'Desk',
  serverOs: OsKind.windows,
  version: 1,
  capabilities: capabilities,
  addr: addr,
);

PairedServerInfo fakeServerInfo({
  String shortId = '0123456789abcdef',
  String name = 'Desk',
  List<String> addrs = const ['192.168.1.10:45810'],
}) => PairedServerInfo(
  publicKeyHex: 'ab' * 32,
  shortId: shortId,
  name: name,
  os: OsKind.windows,
  addrs: addrs,
);

/// Records every call and answers from queues the test fills in.
class FakeLanPilotClient implements LanPilotClient {
  final calls = <String>[];

  /// Each entry is a SessionInfo, a BridgeError or a `Completer<SessionInfo>`.
  /// Consumed in order; when empty, connect succeeds with the next generation.
  final connectResults = <Object>[];

  /// A PairedServerInfo or a BridgeError; null means fakeServerInfo().
  /// Used when [pairResults] is empty.
  Object? pairResult;

  /// Results for the next pair calls, consumed in order before [pairResult].
  final pairResults = <Object>[];

  /// Thrown by every request (button, media, zoom, keyChord, unpair) if set.
  Object? requestError;

  /// Results for validateDiscovered, by fullname.
  final discoverable = <String, DiscoveredInfo>{};

  final eventsController = StreamController<ConnectionEvent>.broadcast();
  int _generation = 0;

  int get generation => _generation;

  @override
  Uint8List generateSecret() => Uint8List.fromList(List.filled(32, 7));

  @override
  String initClient({
    required Uint8List secret,
    required String deviceName,
    required String appVersion,
  }) {
    calls.add('init $deviceName $appVersion');
    return 'fedcba9876543210';
  }

  @override
  DiscoveredInfo? validateDiscovered({
    required String fullname,
    required Map<String, String> txt,
    required List<String> addrs,
    required int port,
  }) => discoverable[fullname];

  @override
  Future<PairedServerInfo> pairWithUri(String uri) async {
    calls.add('pairWithUri $uri');
    return _pair();
  }

  @override
  Future<PairedServerInfo> pairWithPassword({
    required String addr,
    required String password,
  }) async {
    calls.add('pairWithPassword $addr $password');
    return _pair();
  }

  PairedServerInfo _pair() {
    final result = pairResults.isEmpty ? pairResult : pairResults.removeAt(0);
    if (result is BridgeError) throw result;
    return result is PairedServerInfo ? result : fakeServerInfo();
  }

  @override
  Future<SessionInfo> connect({
    required String serverKeyHex,
    required List<String> candidates,
  }) async {
    calls.add('connect ${candidates.join(',')}');
    final next = connectResults.isEmpty ? null : connectResults.removeAt(0);
    if (next is BridgeError) throw next;
    if (next is Completer<SessionInfo>) return next.future;
    if (next is SessionInfo) return next;
    _generation++;
    return fakeSession(_generation);
  }

  @override
  Future<void> disconnect() async => calls.add('disconnect');

  @override
  Future<void> resetEndpoint() async => calls.add('resetEndpoint');

  @override
  void beginGesture() => calls.add('beginGesture');

  @override
  void sendPointer(double dx, double dy) =>
      calls.add('pointer ${dx.toStringAsFixed(1)},${dy.toStringAsFixed(1)}');

  @override
  void sendScroll(double dx, double dy) =>
      calls.add('scroll ${dx.toStringAsFixed(2)},${dy.toStringAsFixed(2)}');

  @override
  Future<void> button(MouseButtonKind button, {required bool down}) =>
      _request('button ${button.name} ${down ? 'down' : 'up'}');

  @override
  Future<void> keyChord(List<int> usages) => _request('keyChord $usages');

  @override
  Future<void> media(MediaKind action) => _request('media ${action.name}');

  @override
  Future<void> zoom(double steps) =>
      _request('zoom ${steps.toStringAsFixed(2)}');

  @override
  Future<void> unpair() => _request('unpair');

  Future<void> _request(String call) async {
    calls.add(call);
    final error = requestError;
    if (error is BridgeError) throw error;
  }

  @override
  Stream<ConnectionEvent> events() => eventsController.stream;

  /// Simulates the end of session [generation].
  void closeSession(int generation, CloseReason reason) => eventsController.add(
    ConnectionEvent(
      generation: generation,
      reason: reason,
      message: reason.name,
    ),
  );
}
