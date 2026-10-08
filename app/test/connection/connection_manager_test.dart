import 'dart:async';

import 'package:fake_async/fake_async.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/conn_state.dart';
import 'package:lanpilot/connection/connection_manager.dart';
import 'package:lanpilot/connection/paired_server.dart';
import 'package:lanpilot/connection/server_store.dart';

import '../support/fake_client.dart';
import '../support/fake_discovery.dart';
import '../support/memory_stores.dart';

const idA = 'aaaaaaaaaaaaaaaa';
const idB = 'bbbbbbbbbbbbbbbb';

PairedServer serverA({String? lastGood}) => PairedServer(
  shortId: idA,
  publicKeyHex: 'ab' * 32,
  name: 'A',
  addrs: const ['10.0.0.1:45810'],
  lastGoodAddr: lastGood,
);

PairedServer serverB() => PairedServer(
  shortId: idB,
  publicKeyHex: 'cd' * 32,
  name: 'B',
  addrs: const ['10.0.0.2:45810'],
);

class Rig {
  Rig({bool everConnected = true})
    : json = FailingJsonStore(
        '{"everConnected": $everConnected, "servers": []}',
      );

  final client = FakeLanPilotClient();
  final discovery = FakeDiscovery();
  final secrets = MemorySecretStore();
  final FailingJsonStore json;
  var _clock = DateTime.utc(2026, 10, 8);
  late final store = ServerStore(
    json,
    secrets,
    now: () => _clock = _clock.add(const Duration(seconds: 1)),
  );
  late final manager = ConnectionManager(
    client: client,
    store: store,
    discovery: discovery,
  );

  ConnState get state => manager.state.value;
  List<String> get connects =>
      client.calls.where((c) => c.startsWith('connect')).toList();

  /// Loads the store with `servers`, most recently used first.
  void seed(FakeAsync async, List<PairedServer> servers) {
    store.load();
    async.flushMicrotasks();
    for (final s in servers.reversed) {
      store.upsert(s);
      async.flushMicrotasks();
      store.markUsed(s.shortId);
      async.flushMicrotasks();
    }
  }

  void start(FakeAsync async) {
    manager.start();
    async.elapse(const Duration(milliseconds: 1600));
  }
}

void main() {
  test('no paired computers means unpaired', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, []);
      r.start(async);
      expect(r.state.status, ConnStatus.unpaired);
      expect(r.connects, isEmpty);
    });
  });

  test('start connects to the most recently used computer', () {
    fakeAsync((async) {
      final r = Rig(everConnected: false)
        ..seed(async, [serverA(lastGood: '10.0.0.9:45810'), serverB()]);
      r.start(async);
      expect(r.connects, ['connect 10.0.0.9:45810,10.0.0.1:45810']);
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idA);
      expect(r.store.everConnected, isTrue);
      expect(r.store.byId(idA)!.lastGoodAddr, '192.168.1.10:45810');
    });
  });

  test('waits for mdns before connecting', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.manager.start();
      async.elapse(const Duration(seconds: 1));
      expect(r.connects, isEmpty);
      r.discovery.announce(
        fakeDiscovered(shortId: idA, addrs: ['10.0.0.77:45810']),
      );
      async.flushMicrotasks();
      expect(r.connects, ['connect 10.0.0.77:45810,10.0.0.1:45810']);
    });
  });

  test('gives up waiting for mdns after 1.5 s', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.manager.start();
      async.elapse(const Duration(milliseconds: 1400));
      expect(r.connects, isEmpty);
      async.elapse(const Duration(milliseconds: 200));
      expect(r.connects, ['connect 10.0.0.1:45810']);
    });
  });

  test('a timeout resets the endpoint and retries once', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.add(bridgeError(ErrorKind.timeout));
      r.start(async);
      expect(r.client.calls.where((c) => !c.startsWith('init')), [
        'connect 10.0.0.1:45810',
        'resetEndpoint',
        'connect 10.0.0.1:45810',
      ]);
      expect(r.state.status, ConnStatus.connected);
    });
  });

  test(
    'never connected and still timing out shows the local network guide',
    () {
      fakeAsync((async) {
        final r = Rig(everConnected: false)..seed(async, [serverA()]);
        r.client.connectResults.addAll([
          bridgeError(ErrorKind.timeout),
          bridgeError(ErrorKind.timeout),
        ]);
        r.start(async);
        expect(r.state.status, ConnStatus.failed);
        expect(r.state.reason, FailReason.localNetwork);
        async.elapse(const Duration(seconds: 1));
        expect(r.state.status, ConnStatus.connected);
      });
    },
  );

  test('an offline computer is retried with backoff', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.addAll(
        List.generate(20, (_) => bridgeError(ErrorKind.unreachable)),
      );
      r.start(async);
      expect(r.state.reason, FailReason.offline);
      expect(r.connects, hasLength(1));
      async.elapse(const Duration(seconds: 1));
      expect(r.connects, hasLength(2));
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, hasLength(3));
      async.elapse(const Duration(seconds: 4));
      expect(r.connects, hasLength(4));
      async.elapse(const Duration(seconds: 8));
      expect(r.connects, hasLength(5));
      async.elapse(const Duration(seconds: 10));
      expect(r.connects, hasLength(6));
      expect(r.state.status, ConnStatus.failed);
    });
  });

  test('retries pick up an address found later', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.add(bridgeError(ErrorKind.unreachable));
      r.start(async);
      expect(r.state.reason, FailReason.offline);
      r.discovery.announce(
        fakeDiscovered(shortId: idA, addrs: ['10.0.0.50:45810']),
      );
      async.elapse(const Duration(seconds: 1));
      expect(r.connects.last, 'connect 10.0.0.50:45810,10.0.0.1:45810');
      expect(r.state.status, ConnStatus.connected);
    });
  });

  test('the switcher opens after 5 s without a connection', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.add(Completer<SessionInfo>());
      r.start(async);
      expect(r.state.showSwitcher, isFalse);
      async.elapse(const Duration(seconds: 4));
      expect(r.state.showSwitcher, isTrue);
      r.manager.acknowledgeSwitcher();
      expect(r.state.showSwitcher, isFalse);
    });
  });

  test('removed by the computer forgets it and does not retry', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.add(bridgeError(ErrorKind.notPaired));
      r.start(async);
      expect(r.state.status, ConnStatus.failed);
      expect(r.state.reason, FailReason.removedByComputer);
      expect(r.store.byId(idA), isNull);
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
    });
  });

  test('device removed while connected', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.client.closeSession(1, CloseReason.deviceRemoved);
      async.flushMicrotasks();
      expect(r.state.reason, FailReason.removedByComputer);
      expect(r.store.byId(idA), isNull);
    });
  });

  test('version mismatch names the side to update and does not retry', () {
    for (final (kind, reason) in [
      (ErrorKind.serverTooOld, FailReason.updateComputer),
      (ErrorKind.appTooOld, FailReason.updateApp),
    ]) {
      fakeAsync((async) {
        final r = Rig()..seed(async, [serverA()]);
        r.client.connectResults.add(bridgeError(kind));
        r.start(async);
        expect(r.state.reason, reason);
        async.elapse(const Duration(seconds: 30));
        expect(r.connects, hasLength(1));
      });
    }
  });

  test('an unexpected close reconnects', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.client.closeSession(1, CloseReason.timedOut);
      async.flushMicrotasks();
      expect(r.connects, hasLength(2));
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.session!.generation, 2);
    });
  });

  test('events from an old session or local closes are ignored', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.client.closeSession(0, CloseReason.timedOut);
      r.client.closeSession(1, CloseReason.local);
      async.flushMicrotasks();
      expect(r.connects, hasLength(1));
      expect(r.state.status, ConnStatus.connected);
    });
  });

  test('pause releases held buttons and disconnects; resume reconnects', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.manager.input.button(MouseButtonKind.left, down: true);
      async.flushMicrotasks();
      r.manager.onPaused();
      async.flushMicrotasks();
      final tail = r.client.calls.reversed.take(2).toList().reversed.toList();
      expect(tail, ['button left up', 'disconnect']);
      expect(r.state.status, ConnStatus.reconnecting);
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, hasLength(2));
      expect(r.state.status, ConnStatus.connected);
    });
  });

  test('a connect that finishes after pause is dropped', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      final pending = Completer<SessionInfo>();
      r.client.connectResults.add(pending);
      r.start(async);
      r.manager.onPaused();
      async.flushMicrotasks();
      pending.complete(fakeSession(1));
      async.flushMicrotasks();
      expect(r.state.status, isNot(ConnStatus.connected));
    });
  });

  test('switching while connecting ends on the new computer', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      final pendingA = Completer<SessionInfo>();
      r.client.connectResults.addAll([pendingA, fakeSession(2)]);
      r.start(async);
      r.manager.switchTo(serverB());
      async.elapse(const Duration(seconds: 2));
      pendingA.complete(fakeSession(1));
      async.flushMicrotasks();
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idB);
      expect(r.state.session!.generation, 2);
      expect(r.store.lastUsed!.shortId, idB);
    });
  });

  test('a network change reconnects', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.manager.onNetworkChanged();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, hasLength(2));
    });
  });

  test('unpairing the connected computer tells it and moves on', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.start(async);
      r.manager.unpair(r.store.byId(idA)!);
      async.elapse(const Duration(seconds: 2));
      expect(r.client.calls, contains('unpair'));
      expect(r.store.byId(idA), isNull);
      expect(r.state.server!.shortId, idB);
      expect(r.state.status, ConnStatus.connected);
      // The close that follows the unpair must not reconnect to A.
      r.client.closeSession(1, CloseReason.unpaired);
      async.flushMicrotasks();
      expect(r.state.server!.shortId, idB);
    });
  });

  test('unpairing the last computer while offline forgets it locally', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.add(bridgeError(ErrorKind.unreachable));
      r.start(async);
      r.manager.unpair(r.store.byId(idA)!);
      async.flushMicrotasks();
      expect(r.client.calls, isNot(contains('unpair')));
      expect(r.store.servers.value, isEmpty);
      expect(r.state.status, ConnStatus.unpaired);
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
    });
  });

  test('addPaired stores the computer and connects to it', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, []);
      r.start(async);
      r.manager.addPaired(fakeServerInfo(shortId: idB, name: 'B'));
      async.elapse(const Duration(seconds: 2));
      expect(r.store.byId(idB), isNotNull);
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idB);
    });
  });

  test('a close that arrives before the session is recorded reconnects', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      final pending = Completer<SessionInfo>();
      r.client.connectResults.addAll([pending, fakeSession(2)]);
      r.start(async);
      pending.complete(fakeSession(1));
      r.client.closeSession(1, CloseReason.timedOut);
      async.flushMicrotasks();
      async.elapse(const Duration(seconds: 1));
      expect(r.connects, hasLength(2));
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.session!.generation, 2);
    });
  });

  test('a switch during the first connect cancels the endpoint reset', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      final pendingA = Completer<SessionInfo>();
      r.client.connectResults.addAll([pendingA, fakeSession(2)]);
      r.start(async);
      r.manager.switchTo(serverB());
      async.elapse(const Duration(seconds: 2));
      pendingA.completeError(bridgeError(ErrorKind.timeout));
      async.flushMicrotasks();
      expect(r.client.calls, isNot(contains('resetEndpoint')));
      expect(r.connects, ['connect 10.0.0.1:45810', 'connect 10.0.0.2:45810']);
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idB);
    });
  });

  test('a pause during the first connect cancels the endpoint reset', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      final pending = Completer<SessionInfo>();
      r.client.connectResults.add(pending);
      r.start(async);
      r.manager.onPaused();
      async.flushMicrotasks();
      pending.completeError(bridgeError(ErrorKind.timeout));
      async.elapse(const Duration(seconds: 30));
      expect(r.client.calls, isNot(contains('resetEndpoint')));
      expect(r.connects, hasLength(1));
    });
  });

  test('pausing keeps a failure that does not retry', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.add(bridgeError(ErrorKind.notPaired));
      r.start(async);
      r.manager.onPaused();
      async.flushMicrotasks();
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 30));
      expect(r.state.status, ConnStatus.failed);
      expect(r.state.reason, FailReason.removedByComputer);
      expect(r.connects, hasLength(1));
    });
  });

  test('a network change while unpairing does not bring the computer back', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      final gate = Completer<void>();
      r.client.unpairGate = gate;
      r.manager.unpair(r.store.byId(idA)!);
      async.flushMicrotasks();
      r.manager.onNetworkChanged();
      async.elapse(const Duration(seconds: 1));
      gate.complete();
      async.elapse(const Duration(seconds: 30));
      expect(r.state.status, ConnStatus.unpaired);
      expect(r.connects, hasLength(1));
    });
  });

  test('a failed computer forgotten with a retry pending is not retried', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.client.connectResults.add(bridgeError(ErrorKind.unreachable));
      r.start(async);
      expect(r.state.status, ConnStatus.failed);
      r.manager.unpair(r.store.byId(idA)!);
      async.elapse(const Duration(seconds: 30));
      expect(r.connects.where((c) => c.contains('10.0.0.1')), hasLength(1));
      expect(r.state.server!.shortId, idB);
    });
  });

  test('dispose stops work in flight', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      final pending = Completer<SessionInfo>();
      r.client.connectResults.add(pending);
      r.start(async);
      r.manager.dispose();
      pending.completeError(bridgeError(ErrorKind.unreachable));
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
    });
  });

  test('switching cancels the startup switcher timer', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.client.connectResults.addAll([
        Completer<SessionInfo>(),
        Completer<SessionInfo>(),
      ]);
      r.manager.start();
      async.elapse(const Duration(seconds: 3));
      r.manager.switchTo(serverB());
      async.elapse(const Duration(seconds: 3));
      expect(r.state.showSwitcher, isFalse);
    });
  });

  test('repeated network changes while reconnecting connect once', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.manager.onNetworkChanged();
      r.manager.onNetworkChanged();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, hasLength(2));
    });
  });

  test('a backoff retry of a phone that connected before does not reset', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.client.connectResults.addAll([
        bridgeError(ErrorKind.unreachable),
        bridgeError(ErrorKind.timeout),
      ]);
      r.start(async);
      async.elapse(const Duration(seconds: 1));
      expect(r.client.calls, isNot(contains('resetEndpoint')));
      expect(r.connects, hasLength(2));
      expect(r.state.reason, FailReason.offline);
    });
  });

  test('addPaired while paused waits for resume, then connects to it', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.manager.onPaused();
      async.flushMicrotasks();
      r.manager.addPaired(fakeServerInfo(shortId: idB, name: 'B'));
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
      expect(r.state.status, ConnStatus.reconnecting);
      expect(r.state.server!.shortId, idB);
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, hasLength(2));
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idB);
    });
  });

  test('a switch racing a pause does not connect until resume', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.start(async);
      r.manager.switchTo(serverB());
      r.manager.onPaused();
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
      expect(r.state.server!.shortId, idB);
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, ['connect 10.0.0.1:45810', 'connect 10.0.0.2:45810']);
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idB);
    });
  });

  test('forgetting a computer while paused waits for resume to move on', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.start(async);
      r.manager.input.button(MouseButtonKind.left, down: true);
      async.flushMicrotasks();
      final gate = Completer<void>();
      r.client.buttonGate = gate;
      r.manager.onPaused();
      async.flushMicrotasks();
      // The session is still recorded while the release is in flight.
      r.client.closeSession(1, CloseReason.unpaired);
      async.flushMicrotasks();
      gate.complete();
      async.elapse(const Duration(seconds: 30));
      expect(r.store.byId(idA), isNull);
      expect(r.connects, hasLength(1));
      expect(r.state.status, ConnStatus.reconnecting);
      expect(r.state.server!.shortId, idB);
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, ['connect 10.0.0.1:45810', 'connect 10.0.0.2:45810']);
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idB);
    });
  });

  test('removal by the computer while paused connects nowhere', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.start(async);
      r.manager.input.button(MouseButtonKind.left, down: true);
      async.flushMicrotasks();
      final gate = Completer<void>();
      r.client.buttonGate = gate;
      r.manager.onPaused();
      async.flushMicrotasks();
      r.client.closeSession(1, CloseReason.deviceRemoved);
      async.flushMicrotasks();
      gate.complete();
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 30));
      expect(r.connects, hasLength(1));
      expect(r.state.reason, FailReason.removedByComputer);
    });
  });

  test('a switch while paused ends a connect still in flight', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      final pending = Completer<SessionInfo>();
      r.client.connectResults.add(pending);
      r.start(async);
      r.manager.onPaused();
      r.manager.switchTo(serverB());
      async.flushMicrotasks();
      pending.complete(fakeSession(1));
      async.elapse(const Duration(seconds: 30));
      expect(r.client.calls.last, 'disconnect');
      expect(r.connects, hasLength(1));
      expect(r.state.status, ConnStatus.reconnecting);
      expect(r.state.server!.shortId, idB);
    });
  });

  test('a pause that finishes after resume keeps the new session', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA()]);
      r.start(async);
      r.manager.input.button(MouseButtonKind.left, down: true);
      async.flushMicrotasks();
      final gate = Completer<void>();
      r.client.buttonGate = gate;
      r.manager.onPaused();
      async.flushMicrotasks();
      r.manager.onResumed();
      async.elapse(const Duration(seconds: 2));
      expect(r.connects, hasLength(2));
      expect(r.state.status, ConnStatus.connected);
      final afterConnect = r.client.calls.length;
      gate.complete();
      async.elapse(const Duration(seconds: 2));
      expect(r.client.calls.skip(afterConnect), isNot(contains('disconnect')));
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.session!.generation, 2);
    });
  });

  test('switching to a computer forgotten meanwhile never connects to it', () {
    fakeAsync((async) {
      final r = Rig()..seed(async, [serverA(), serverB()]);
      r.start(async);
      r.manager.switchTo(serverB());
      r.manager.unpair(serverB());
      async.elapse(const Duration(seconds: 30));
      expect(r.store.byId(idB), isNull);
      expect(r.connects, isNot(contains('connect 10.0.0.2:45810')));
      expect(r.state.status, ConnStatus.connected);
      expect(r.state.server!.shortId, idA);
    });
  });

  group('when the store cannot be written', () {
    late List<String?> logged;
    late DebugPrintCallback original;
    setUp(() {
      logged = [];
      original = debugPrint;
      debugPrint = (message, {wrapWidth}) => logged.add(message);
    });
    tearDown(() => debugPrint = original);

    test('a connect still ends connected', () {
      fakeAsync((async) {
        final r = Rig()..seed(async, [serverA()]);
        r.json.failWrites = true;
        r.start(async);
        expect(r.state.status, ConnStatus.connected);
        expect(r.state.session!.generation, 1);
        expect(r.state.server!.lastGoodAddr, '192.168.1.10:45810');
        expect(logged, isNotEmpty);
      });
    });

    test('forgetting a computer still moves on to the next one', () {
      fakeAsync((async) {
        final r = Rig()..seed(async, [serverA(), serverB()]);
        r.start(async);
        r.json.failWrites = true;
        r.client.closeSession(1, CloseReason.unpaired);
        async.elapse(const Duration(seconds: 2));
        expect(r.store.byId(idA), isNull);
        expect(r.state.status, ConnStatus.connected);
        expect(r.state.server!.shortId, idB);
        expect(logged, isNotEmpty);
      });
    });

    test('a switch still connects to the chosen computer', () {
      fakeAsync((async) {
        final r = Rig()..seed(async, [serverA(), serverB()]);
        r.start(async);
        r.json.failWrites = true;
        r.manager.switchTo(serverB());
        async.elapse(const Duration(seconds: 2));
        expect(r.state.status, ConnStatus.connected);
        expect(r.state.server!.shortId, idB);
      });
    });
  });

  test('candidateAddrs orders mdns, last good, then pairing addresses', () {
    final server = serverA(lastGood: '10.0.0.9:45810')
        .copyWith(addrs: ['10.0.0.1:45810', '10.0.0.9:45810']);
    expect(
      candidateAddrs(
        server,
        fakeDiscovered(
          shortId: idA,
          addrs: ['10.0.0.5:45810', '10.0.0.1:45810'],
        ),
      ),
      ['10.0.0.5:45810', '10.0.0.1:45810', '10.0.0.9:45810'],
    );
    expect(candidateAddrs(server, null), ['10.0.0.9:45810', '10.0.0.1:45810']);
  });
}
