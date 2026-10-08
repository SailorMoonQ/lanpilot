import 'package:flutter/foundation.dart';

import '../storage/json_store.dart';
import 'settings.dart';

class SettingsController extends ValueNotifier<Settings> {
  SettingsController(this._json) : super(const Settings());

  final JsonStore _json;

  Future<void> load() async => value = Settings.fromJson(await _json.read());

  /// Applies `change` at once (the UI updates immediately) and persists it.
  Future<void> update(Settings Function(Settings) change) async {
    value = change(value);
    try {
      await _json.write(value.toJson());
    } on Object catch (e) {
      // The new value stays in memory; only persistence failed.
      debugPrint('settings write failed: $e');
    }
  }
}
