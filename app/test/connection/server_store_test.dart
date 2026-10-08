import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/bridge/lanpilot_client.dart';
import 'package:lanpilot/connection/paired_server.dart';
import 'package:lanpilot/connection/server_store.dart';

import '../support/fake_client.dart';
import '../support/memory_stores.dart';

void main() {
  late MemoryJsonStore json;
  late MemorySecretStore secrets;
  var clock = DateTime.utc(2026, 10, 8, 12);
  ServerStore store() => ServerStore(json, secrets, now: () => clock);

  setUp(() {
    json = MemoryJsonStore();
    secrets = MemorySecretStore();
    clock = DateTime.utc(2026, 10, 8, 12);
  });

  PairedServer server(String id, String name) =>
      PairedServer.fromInfo(fakeServerInfo(shortId: id, name: name));

  test('keys go to the keychain, the rest to json', () async {
    final s = store();
    await s.upsert(server('aaaaaaaaaaaaaaaa', 'A'));
    expect(secrets.values[ServerStore.keyName('aaaaaaaaaaaaaaaa')], 'ab' * 32);
    expect(json.text, isNot(contains('ab' * 32)));

    final again = store();
    await again.load();
    expect(again.servers.value.single.name, 'A');
    expect(again.servers.value.single.publicKeyHex, 'ab' * 32);
    expect(again.servers.value.single.os, OsKind.windows);
  });

  test('most recently used comes first', () async {
    final s = store();
    await s.upsert(server('aaaaaaaaaaaaaaaa', 'A'));
    await s.upsert(server('bbbbbbbbbbbbbbbb', 'B'));
    await s.markUsed('aaaaaaaaaaaaaaaa');
    clock = clock.add(const Duration(minutes: 1));
    await s.markUsed('bbbbbbbbbbbbbbbb', goodAddr: '10.0.0.2:45810');
    expect(s.servers.value.map((e) => e.name), ['B', 'A']);
    expect(s.lastUsed!.lastGoodAddr, '10.0.0.2:45810');
    expect(s.everConnected, isTrue);
  });

  test('remove deletes the key too', () async {
    final s = store();
    await s.upsert(server('aaaaaaaaaaaaaaaa', 'A'));
    await s.remove('aaaaaaaaaaaaaaaa');
    expect(s.servers.value, isEmpty);
    expect(secrets.values, isEmpty);
  });

  test('servers without a keychain key are dropped', () async {
    final s = store();
    await s.upsert(server('aaaaaaaaaaaaaaaa', 'A'));
    await s.upsert(server('bbbbbbbbbbbbbbbb', 'B'));
    secrets.values.remove(ServerStore.keyName('aaaaaaaaaaaaaaaa'));
    final again = store();
    await again.load();
    expect(again.servers.value.map((e) => e.name), ['B']);
  });

  test('a corrupt file starts empty', () async {
    json.text = '{"servers": [42, {"name": 1}], "everConnected": "yes"}';
    final s = store();
    await s.load();
    expect(s.servers.value, isEmpty);
    expect(s.everConnected, isFalse);
  });
}
