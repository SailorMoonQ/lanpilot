import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';

import '../bridge/lanpilot_client.dart';
import 'conn_state.dart';
import 'discovery_lookup.dart';
import 'input_controller.dart';
import 'paired_server.dart';
import 'server_store.dart';

/// Candidate addresses (spec 4.2): the latest mDNS addresses, then the last
/// address that worked, then the ones from pairing. No duplicates.
List<String> candidateAddrs(PairedServer server, DiscoveredInfo? discovered) {
  final out = <String>[];
  void add(String? a) {
    if (a != null && a.isNotEmpty && !out.contains(a)) out.add(a);
  }

  for (final a in discovered?.addrs ?? const <String>[]) {
    add(a);
  }
  add(server.lastGoodAddr);
  for (final a in server.addrs) {
    add(a);
  }
  return out;
}

/// Connection lifecycle (spec 4): which computer, which state, retries,
/// background and foreground, unpairing.
class ConnectionManager {
  ConnectionManager({
    required this._client,
    required this._store,
    required this._discovery,
    this.mdnsWait = const Duration(milliseconds: 1500),
    this.switcherDelay = const Duration(seconds: 5),
  });

  static const retryDelays = [
    Duration(seconds: 1),
    Duration(seconds: 2),
    Duration(seconds: 4),
    Duration(seconds: 8),
    Duration(seconds: 10),
  ];

  final LanPilotClient _client;
  final ServerStore _store;
  final DiscoveryLookup _discovery;
  final Duration mdnsWait;
  final Duration switcherDelay;

  final ValueNotifier<ConnState> state = ValueNotifier(
    const ConnState(status: ConnStatus.unpaired),
  );
  late final InputController input = InputController(
    _client,
    () => state.value.status == ConnStatus.connected,
  );

  int _attempt = 0;
  int _retryCount = 0;
  int? _inFlight;
  bool _paused = false;
  bool _disposed = false;

  /// Closes that arrived for a session the manager has not recorded yet.
  final _earlyCloses = <int, CloseReason>{};
  Timer? _retryTimer;
  Timer? _switcherTimer;
  StreamSubscription<ConnectionEvent>? _events;

  Future<void> start() async {
    _events ??= _client.events().listen(_onEvent);
    final last = _store.lastUsed;
    if (last == null) {
      _set(const ConnState(status: ConnStatus.unpaired));
      return;
    }
    _switcherTimer?.cancel();
    _switcherTimer = Timer(switcherDelay, () {
      if (state.value.status != ConnStatus.connected) {
        _set(state.value.copyWith(showSwitcher: true));
      }
    });
    await connectTo(last);
  }

  void acknowledgeSwitcher() {
    if (state.value.showSwitcher) {
      _set(state.value.copyWith(showSwitcher: false));
    }
  }

  /// Connects to `server`. Fresh connects (start, switch, resume, network
  /// change) wait up to `mdnsWait` for Bonjour; retries and reconnects after
  /// a drop use what is already known so they are not delayed.
  ///
  /// In the background nothing connects (spec 4.3): `server` becomes the
  /// computer that `onResumed` connects to.
  Future<void> connectTo(PairedServer server, {bool waitForMdns = true}) async {
    if (_paused) return _holdForResume(server);
    final attempt = ++_attempt;
    _inFlight = attempt;
    _retryTimer?.cancel();
    final current = state.value;
    final same = current.server?.shortId == server.shortId;
    final ConnState next;
    if (same &&
        (current.status == ConnStatus.connected ||
            current.status == ConnStatus.reconnecting)) {
      next = current.copyWith(
        status: ConnStatus.reconnecting,
        clearSession: true,
      );
    } else if (same && current.status == ConnStatus.failed) {
      next = current.copyWith(clearSession: true); // keep "retrying" on screen
    } else {
      next = ConnState(
        status: ConnStatus.connecting,
        server: server,
        showSwitcher: current.showSwitcher,
      );
    }
    _set(next);

    final discovered =
        _discovery.lookup(server.shortId) ??
        (waitForMdns
            ? await _discovery.waitFor(server.shortId, mdnsWait)
            : null);
    if (attempt != _attempt) return;
    final candidates = candidateAddrs(server, discovered);
    try {
      final session = await _connectResetting(
        attempt,
        server.publicKeyHex,
        candidates,
        fresh: waitForMdns,
      );
      if (attempt != _attempt) return;
      _retryCount = 0;
      _switcherTimer?.cancel();
      final stored = await _markUsed(server.shortId, goodAddr: session.addr);
      if (attempt != _attempt) return;
      if (stored == null) {
        // Forgotten while connecting: do not stay connected to it.
        await _client.disconnect();
        return;
      }
      final early = _earlyCloses[session.generation];
      _earlyCloses.removeWhere((g, _) => g <= session.generation);
      if (early != null && early != CloseReason.local) {
        _handleClose(server, early);
        return;
      }
      _set(
        ConnState(
          status: ConnStatus.connected,
          server: stored,
          session: session,
          showSwitcher: state.value.showSwitcher,
        ),
      );
    } on BridgeError catch (e) {
      if (attempt != _attempt) return;
      await _onFailure(server, e);
    } finally {
      if (_inFlight == attempt) _inFlight = null;
    }
  }

  /// A timeout on a fresh connect, or before the phone ever connected, gets
  /// one more try on a new socket: a socket made before Local Network access
  /// was granted stays blocked (spec 4.4). Backoff retries of a phone that
  /// has connected before do not reset.
  Future<SessionInfo> _connectResetting(
    int attempt,
    String key,
    List<String> candidates, {
    required bool fresh,
  }) async {
    try {
      return await _client.connect(serverKeyHex: key, candidates: candidates);
    } on BridgeError catch (e) {
      if (e.kind != ErrorKind.timeout) rethrow;
      if (!fresh && _store.everConnected) rethrow;
      if (attempt != _attempt) rethrow;
      await _client.resetEndpoint();
      if (attempt != _attempt) rethrow;
      return _client.connect(serverKeyHex: key, candidates: candidates);
    }
  }

  Future<void> _onFailure(PairedServer server, BridgeError e) async {
    switch (e.kind) {
      case ErrorKind.notPaired || ErrorKind.deviceRemoved:
        await _forget(server, removedByComputer: true);
      case ErrorKind.unpaired:
        await _forget(server, removedByComputer: false);
      case ErrorKind.serverTooOld:
        _set(_failed(server, FailReason.updateComputer));
      case ErrorKind.appTooOld:
        _set(_failed(server, FailReason.updateApp));
      default:
        final neverConnected = !_store.everConnected;
        final reason = e.kind == ErrorKind.timeout && neverConnected
            ? FailReason.localNetwork
            : FailReason.offline;
        _set(_failed(server, reason));
        _scheduleRetry(server);
    }
  }

  ConnState _failed(PairedServer server, FailReason reason) => ConnState(
    status: ConnStatus.failed,
    reason: reason,
    server: server,
    showSwitcher: state.value.showSwitcher,
  );

  Future<void> _forget(
    PairedServer server, {
    required bool removedByComputer,
  }) async {
    final attempt = ++_attempt;
    _retryTimer?.cancel();
    _switcherTimer?.cancel();
    await _remove(server.shortId);
    if (attempt != _attempt) return;
    if (removedByComputer) {
      _set(_failed(server, FailReason.removedByComputer));
      return;
    }
    await _connectLastUsed();
  }

  /// In the background: `onResumed` connects to `server`. A connect still in
  /// flight from before the pause ends here, since the bridge runs this
  /// disconnect after it.
  Future<void> _holdForResume(PairedServer server) async {
    _set(ConnState(status: ConnStatus.reconnecting, server: server));
    await _client.disconnect();
  }

  Future<void> _connectLastUsed() async {
    final next = _store.lastUsed;
    if (next == null) {
      _set(const ConnState(status: ConnStatus.unpaired));
    } else {
      await connectTo(next);
    }
  }

  /// `ServerStore.markUsed`, surviving a failed write: the list in memory is
  /// already updated then, and losing the recency on disk must not leave the
  /// state stuck on connecting. Null still means the computer was forgotten.
  Future<PairedServer?> _markUsed(String shortId, {String? goodAddr}) async {
    try {
      return await _store.markUsed(shortId, goodAddr: goodAddr);
    } on Object catch (e) {
      debugPrint('LanPilot: could not save the paired computers: $e');
      return _store.byId(shortId);
    }
  }

  /// `ServerStore.remove`, surviving a failed write: the computer is gone
  /// from the list in memory either way.
  Future<void> _remove(String shortId) async {
    try {
      await _store.remove(shortId);
    } on Object catch (e) {
      debugPrint('LanPilot: could not save the paired computers: $e');
    }
  }

  void _scheduleRetry(PairedServer server) {
    if (_paused || _disposed) return;
    final delay = retryDelays[math.min(_retryCount, retryDelays.length - 1)];
    _retryCount++;
    _retryTimer = Timer(delay, () {
      if (_store.byId(server.shortId) == null) return;
      unawaited(connectTo(server, waitForMdns: false));
    });
  }

  Future<void> retryNow() async {
    final server = state.value.server;
    if (server != null && _store.byId(server.shortId) != null) {
      _retryCount = 0;
      await connectTo(server);
    }
  }

  void _onEvent(ConnectionEvent event) {
    final current = state.value;
    final server = current.server;
    final known = current.session?.generation;
    if (known == null || event.generation > known) {
      // Not recorded yet: a connect may be about to return this session.
      _earlyCloses[event.generation] = event.reason;
      return;
    }
    if (server == null || known != event.generation) return;
    _handleClose(server, event.reason);
  }

  void _handleClose(PairedServer server, CloseReason reason) {
    switch (reason) {
      case CloseReason.local:
        return;
      case CloseReason.notPaired || CloseReason.deviceRemoved:
        unawaited(_forget(server, removedByComputer: true));
      case CloseReason.unpaired:
        unawaited(_forget(server, removedByComputer: false));
      default:
        if (!_paused) unawaited(connectTo(server, waitForMdns: false));
    }
  }

  Future<void> onPaused() async {
    if (_paused) return;
    _paused = true;
    final attempt = ++_attempt;
    _retryTimer?.cancel();
    _switcherTimer?.cancel();
    await input.releaseAll();
    // A resume (or a switch) took over while buttons were released: its
    // session must stay up.
    if (attempt != _attempt) return;
    await _client.disconnect();
    final current = state.value;
    if (attempt != _attempt || current.server == null) return;
    final settled =
        current.status == ConnStatus.failed &&
        (current.reason == FailReason.removedByComputer ||
            current.reason == FailReason.updateComputer ||
            current.reason == FailReason.updateApp);
    if (settled) return;
    _set(current.copyWith(status: ConnStatus.reconnecting, clearSession: true));
  }

  Future<void> onResumed() async {
    if (!_paused) return;
    _paused = false;
    final server = state.value.server;
    if (server != null && _store.byId(server.shortId) != null) {
      _retryCount = 0;
      await connectTo(server);
    }
  }

  Future<void> onNetworkChanged() async {
    final current = state.value;
    final server = current.server;
    if (_paused || server == null) return;
    final idleFailed =
        current.status == ConnStatus.failed && _inFlight != _attempt;
    if (current.status != ConnStatus.connected && !idleFailed) return;
    if (_store.byId(server.shortId) == null) return;
    _retryCount = 0;
    await connectTo(server);
  }

  Future<void> switchTo(PairedServer server) async {
    final attempt = ++_attempt;
    _retryTimer?.cancel();
    _switcherTimer?.cancel();
    _retryCount = 0;
    if (state.value.status == ConnStatus.connected) {
      await input.releaseAll();
      _set(state.value.copyWith(clearSession: true));
      await _client.disconnect();
    }
    final stored = await _markUsed(server.shortId);
    if (stored == null) {
      // Forgotten meanwhile: never connect to it. If nothing else took over,
      // settle on the most recent computer left.
      if (attempt == _attempt) await _connectLastUsed();
      return;
    }
    if (_paused) return _holdForResume(stored);
    _set(ConnState(status: ConnStatus.connecting, server: stored));
    await connectTo(stored);
  }

  Future<PairedServer> addPaired(PairedServerInfo info) async {
    final existing = _store.byId(info.shortId);
    final server = PairedServer.fromInfo(info)
        .copyWith(lastGoodAddr: existing?.lastGoodAddr);
    await _store.upsert(server);
    unawaited(switchTo(server));
    return server;
  }

  Future<void> unpair(PairedServer server) async {
    final current = state.value;
    final isCurrent = current.server?.shortId == server.shortId;
    if (isCurrent) {
      _attempt++;
      _retryTimer?.cancel();
      _switcherTimer?.cancel();
      // Forget the session first so its close event is not treated as a drop.
      _set(current.copyWith(clearSession: true));
      if (current.status == ConnStatus.connected) {
        try {
          await _client.unpair();
        } on BridgeError {
          // Unreachable: forget it locally only.
        }
      }
    }
    await _store.remove(server.shortId);
    if (!isCurrent) return;
    _attempt++;
    await _client.disconnect();
    final next = _store.lastUsed;
    if (next == null) {
      _set(const ConnState(status: ConnStatus.unpaired));
    } else {
      await switchTo(next);
    }
  }

  void dispose() {
    _disposed = true;
    _attempt++;
    _retryTimer?.cancel();
    _switcherTimer?.cancel();
    unawaited(_events?.cancel());
    state.dispose();
  }

  void _set(ConnState next) {
    if (!_disposed) state.value = next;
  }
}
