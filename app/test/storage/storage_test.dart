import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot/storage/identity_store.dart';
import 'package:lanpilot/storage/json_store.dart';

import '../support/memory_stores.dart';

void main() {
  group('JsonFile', () {
    late Directory dir;
    setUp(() => dir = Directory.systemTemp.createTempSync('lanpilot_json'));
    tearDown(() => dir.deleteSync(recursive: true));

    test('round trips an object', () async {
      final file = JsonFile(File('${dir.path}/sub/a.json'));
      await file.write({
        'x': 1,
        'y': ['z'],
      });
      expect(await file.read(), {
        'x': 1,
        'y': ['z'],
      });
    });

    test('a missing file reads as empty', () async {
      expect(await JsonFile(File('${dir.path}/none.json')).read(), isEmpty);
    });

    test('corrupt json reads as empty', () async {
      final f = File('${dir.path}/bad.json')..writeAsStringSync('{not json');
      expect(await JsonFile(f).read(), isEmpty);
      f.writeAsStringSync('[1, 2]');
      expect(await JsonFile(f).read(), isEmpty);
    });
  });

  group('IdentityStore', () {
    test('creates once, then loads the same secret', () async {
      final secrets = MemorySecretStore();
      var generated = 0;
      Uint8List generate() {
        generated++;
        return Uint8List.fromList(List.generate(32, (i) => i));
      }

      final first = await IdentityStore(secrets, generate).loadOrCreate();
      final second = await IdentityStore(secrets, generate).loadOrCreate();
      expect(second, first);
      expect(generated, 1);
    });

    test('a damaged secret is replaced', () async {
      final secrets = MemorySecretStore()..values['identity_secret'] = '@@@';
      final fresh = Uint8List.fromList(List.filled(32, 9));
      expect(await IdentityStore(secrets, () => fresh).loadOrCreate(), fresh);
      secrets.values['identity_secret'] = base64Encode([1, 2, 3]);
      expect(await IdentityStore(secrets, () => fresh).loadOrCreate(), fresh);
    });
  });
}
