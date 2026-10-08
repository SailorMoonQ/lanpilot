import 'dart:convert';
import 'dart:typed_data';

import 'secret_store.dart';

/// The phone's long-term identity secret (32 bytes), kept in the keychain.
class IdentityStore {
  IdentityStore(this._secrets, this._generate);

  static const _key = 'identity_secret';

  final SecretStore _secrets;
  final Uint8List Function() _generate;

  Future<Uint8List> loadOrCreate() async {
    final stored = await _secrets.read(_key);
    if (stored != null) {
      try {
        final bytes = base64Decode(stored);
        if (bytes.length == 32) return bytes;
      } on FormatException {
        // Damaged: replaced below.
      }
    }
    final fresh = _generate();
    await _secrets.write(_key, base64Encode(fresh));
    return fresh;
  }
}
