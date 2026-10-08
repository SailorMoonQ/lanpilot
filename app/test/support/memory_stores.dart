import 'dart:convert';

import 'package:lanpilot/storage/json_store.dart';
import 'package:lanpilot/storage/secret_store.dart';

class MemoryJsonStore implements JsonStore {
  MemoryJsonStore([this.text]);

  String? text;

  @override
  Future<Map<String, Object?>> read() async =>
      text == null ? {} : decodeJsonObject(text!);

  @override
  Future<void> write(Map<String, Object?> data) async =>
      text = jsonEncode(data);
}

class MemorySecretStore implements SecretStore {
  final values = <String, String>{};

  @override
  Future<String?> read(String key) async => values[key];

  @override
  Future<void> write(String key, String value) async => values[key] = value;

  @override
  Future<void> delete(String key) async => values.remove(key);
}
