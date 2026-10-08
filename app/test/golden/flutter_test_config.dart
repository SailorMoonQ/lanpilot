import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

/// Allows tiny anti-aliasing differences between macOS versions so goldens
/// made on one Mac pass on the CI runner, without hiding real changes.
class TolerantComparator extends LocalFileComparator {
  TolerantComparator(super.testFile);

  static const tolerance = 0.005; // fraction of pixels

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    if (result.passed || result.diffPercent <= tolerance) {
      result.dispose();
      return true;
    }
    final error = await generateFailureOutput(result, golden, basedir);
    result.dispose();
    throw FlutterError(error);
  }
}

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  final current = goldenFileComparator;
  if (current is LocalFileComparator) {
    goldenFileComparator = TolerantComparator(
      current.basedir.resolve('golden_test.dart'),
    );
  }
  await testMain();
}
