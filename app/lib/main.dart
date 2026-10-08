import 'package:flutter/material.dart';
import 'package:lanpilot/bridge/generated/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(
    const MaterialApp(
      home: Scaffold(body: Center(child: Text('LanPilot'))),
    ),
  );
}
