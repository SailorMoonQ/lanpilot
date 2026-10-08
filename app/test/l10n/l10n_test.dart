import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

Map<String, Object?> arb(String name) =>
    jsonDecode(File('lib/l10n/$name').readAsStringSync())
        as Map<String, Object?>;

void main() {
  test('Chinese and English have the same keys', () {
    Set<String> keys(Map<String, Object?> m) =>
        m.keys.where((k) => !k.startsWith('@')).toSet();
    expect(keys(arb('app_zh.arb')), keys(arb('app_en.arb')));
  });

  test('no em dash in any string', () {
    for (final name in ['app_en.arb', 'app_zh.arb']) {
      expect(
        File('lib/l10n/$name').readAsStringSync().contains('\u2014'),
        isFalse,
        reason: name,
      );
    }
  });
  test('every zh placeholder exists in the en string', () {
    Set<String> placeholders(Object? value) =>
        RegExp(r'\{(\w+)\}')
            .allMatches(value! as String)
            .map((m) => m.group(1)!)
            .toSet();
    final en = arb('app_en.arb');
    final zh = arb('app_zh.arb');
    for (final key in zh.keys.where(
      (k) => !k.startsWith('@') && en.containsKey(k),
    )) {
      expect(
        placeholders(zh[key]).difference(placeholders(en[key])),
        isEmpty,
        reason: key,
      );
    }
  });
}
