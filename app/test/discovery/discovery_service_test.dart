import 'dart:async';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/discovery/discovery_service.dart';
import 'package:lanpilot_discovery/lanpilot_discovery.dart';

import '../support/fake_discovery.dart';

const fullA = 'aaaaaaaaaaaaaaaa._lanpilot._udp.local.';

RawService found(String fullname) => RawService(
  found: true,
  fullname: fullname,
  addrs: const ['10.0.0.2'],
  port: 45810,
);

void main() {
  late StreamController<RawService> events;
  late Map<String, DiscoveredInfo> valid;
  late DiscoveryService service;

  setUp(() {
    events = StreamController<RawService>.broadcast();
    valid = {fullA: fakeDiscovered(shortId: 'aaaaaaaaaaaaaaaa')};
    service = DiscoveryService(
      () => events.stream,
      (raw) => valid[raw.fullname],
    )..start();
  });

  tearDown(() => service.stop());

  test('valid results are added and lost ones removed', () {
    fakeAsync((async) {
      events.add(found(fullA));
      async.flushMicrotasks();
      expect(service.devices.value.keys, ['aaaaaaaaaaaaaaaa']);
      events.add(const RawService(found: false, fullname: fullA));
      async.flushMicrotasks();
      expect(service.devices.value, isEmpty);
    });
  });

  test('invalid results are ignored', () {
    fakeAsync((async) {
      events.add(found('evil._lanpilot._udp.local.'));
      async.flushMicrotasks();
      expect(service.devices.value, isEmpty);
    });
  });

  test('waitFor completes when the device appears, or with null', () {
    fakeAsync((async) {
      DiscoveredInfo? got;
      var done = false;
      service.waitFor('aaaaaaaaaaaaaaaa', const Duration(seconds: 2)).then((d) {
        got = d;
        done = true;
      });
      async.elapse(const Duration(milliseconds: 500));
      expect(done, isFalse);
      events.add(found(fullA));
      async.flushMicrotasks();
      expect(got?.shortId, 'aaaaaaaaaaaaaaaa');

      var timedOut = false;
      service
          .waitFor('bbbbbbbbbbbbbbbb', const Duration(seconds: 2))
          .then((d) => timedOut = d == null);
      async.elapse(const Duration(seconds: 2));
      expect(timedOut, isTrue);
    });
  });

  test('stream errors do not stop discovery', () {
    fakeAsync((async) {
      events.addError(Exception('NWBrowser failed'));
      events.add(found(fullA));
      async.flushMicrotasks();
      expect(service.devices.value, hasLength(1));
    });
  });

  test('a re-announcement with a new short id replaces the entry', () {
    fakeAsync((async) {
      events.add(found(fullA));
      async.flushMicrotasks();
      valid[fullA] = fakeDiscovered(shortId: 'cccccccccccccccc');
      events.add(found(fullA));
      async.flushMicrotasks();
      expect(service.devices.value.keys, ['cccccccccccccccc']);
    });
  });

  test('a re-announcement that fails validation removes the entry', () {
    fakeAsync((async) {
      events.add(found(fullA));
      async.flushMicrotasks();
      valid.remove(fullA);
      events.add(found(fullA));
      async.flushMicrotasks();
      expect(service.devices.value, isEmpty);
    });
  });

  test('lost keeps a device another fullname still announces', () {
    fakeAsync((async) {
      const fullB = 'bbbbbbbbbbbbbbbb._lanpilot._udp.local.';
      valid[fullB] = fakeDiscovered(shortId: 'aaaaaaaaaaaaaaaa');
      events.add(found(fullA));
      events.add(found(fullB));
      async.flushMicrotasks();
      events.add(const RawService(found: false, fullname: fullA));
      async.flushMicrotasks();
      expect(service.devices.value.keys, ['aaaaaaaaaaaaaaaa']);
    });
  });

  test('stop clears devices', () {
    fakeAsync((async) {
      events.add(found(fullA));
      async.flushMicrotasks();
      service.stop();
      async.flushMicrotasks();
      expect(service.devices.value, isEmpty);
    });
  });

  test('lost for an unknown fullname is a no-op', () {
    fakeAsync((async) {
      events.add(found(fullA));
      async.flushMicrotasks();
      events.add(const RawService(found: false, fullname: 'zzz'));
      async.flushMicrotasks();
      expect(service.devices.value, hasLength(1));
    });
  });
}
