import 'dart:convert';
import 'dart:io';

/// One JSON object, persisted somewhere.
abstract class JsonStore {
  Future<Map<String, Object?>> read();

  Future<void> write(Map<String, Object?> data);
}

/// Decodes a JSON object. Anything else (corrupt text, a list) is `{}`, so a
/// damaged file never stops the app from starting.
Map<String, Object?> decodeJsonObject(String text) {
  try {
    final decoded = jsonDecode(text);
    return decoded is Map<String, Object?> ? decoded : {};
  } on FormatException {
    return {};
  }
}

/// A JSON file in the app documents directory, replaced atomically.
class JsonFile implements JsonStore {
  JsonFile(this.file);

  final File file;
  Future<void> _last = Future.value();

  @override
  Future<Map<String, Object?>> read() async {
    try {
      return decodeJsonObject(await file.readAsString());
    } on FileSystemException {
      return {};
    } on FormatException {
      return {};
    }
  }

  @override
  Future<void> write(Map<String, Object?> data) {
    // Writes run one at a time, in call order; a failure does not stop the
    // chain but is still reported to its own caller.
    final result = _last.then((_) => _write(data));
    _last = result.then((_) {}, onError: (Object _) {});
    return result;
  }

  Future<void> _write(Map<String, Object?> data) async {
    await file.parent.create(recursive: true);
    final tmp = File('${file.path}.tmp');
    await tmp.writeAsString(jsonEncode(data), flush: true);
    await tmp.rename(file.path);
  }
}
