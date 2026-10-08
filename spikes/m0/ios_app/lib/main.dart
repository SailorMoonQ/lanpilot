import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:lanpilot_spike/src/rust/api/spike.dart' as spike;
import 'package:lanpilot_spike/src/rust/frb_generated.dart';
import 'package:path_provider/path_provider.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(const SpikeApp());
}

class SpikeApp extends StatelessWidget {
  const SpikeApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'LanPilot spike',
      theme: ThemeData(colorSchemeSeed: Colors.indigo),
      home: const SpikePage(),
    );
  }
}

class SpikePage extends StatefulWidget {
  const SpikePage({super.key});

  @override
  State<SpikePage> createState() => _SpikePageState();
}

class _SpikePageState extends State<SpikePage> {
  final _uri = TextEditingController();
  final _log = <String>[];
  bool _busy = false;
  File? _logFile;

  @override
  void initState() {
    super.initState();
    _start();
  }

  /// Steps from LANPILOT_AUTO (e.g. "pair,connect,square,latency") run in
  /// order at launch, so a spike run can be driven from `devicectl`.
  Future<void> _start() async {
    final env = Platform.environment;
    _uri.text = env['LANPILOT_PAIR_URI'] ?? '';
    final dir = await getApplicationDocumentsDirectory();
    _logFile = File('${dir.path}/spike-log.txt');
    await _run('init', () => spike.initClient(docsDir: dir.path));
    for (final step in (env['LANPILOT_AUTO'] ?? '').split(',')) {
      final action = _actions[step.trim()];
      if (action != null) await _run(step.trim(), action);
    }
  }

  late final Map<String, Future<String> Function()> _actions = {
    'probe': () => spike.probeLan(
      addr: Platform.environment['LANPILOT_PROBE'] ?? '192.168.50.203:45810',
    ),
    'pair': () => spike.pair(uri: _uri.text),
    'connect': spike.connect,
    'square': spike.drawSquare,
    'latency': () => spike.latencyTest(rounds: 200),
    'disconnect': spike.disconnect,
  };

  String _now() {
    final t = DateTime.now();
    String two(int n) => n.toString().padLeft(2, '0');
    return '${two(t.hour)}:${two(t.minute)}:${two(t.second)}';
  }

  Future<void> _run(String label, Future<String> Function() action) async {
    if (_busy) return;
    setState(() => _busy = true);
    String line;
    try {
      line = '${_now()} $label: ${await action()}';
    } catch (e) {
      line = '${_now()} $label FAILED: $e';
    }
    stdout.writeln('[spike] $line');
    await _logFile?.writeAsString('$line\n', mode: FileMode.append);
    if (!mounted) return;
    setState(() {
      _log.insert(0, line);
      _busy = false;
    });
  }

  Widget _button(String label, Future<String> Function() action) {
    return FilledButton(
      onPressed: _busy ? null : () => _run(label, action),
      child: Text(label),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('LanPilot M0 spike')),
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                controller: _uri,
                decoration: InputDecoration(
                  labelText: 'Pairing URI (lanpilot://pair?d=...)',
                  border: const OutlineInputBorder(),
                  suffixIcon: IconButton(
                    icon: const Icon(Icons.content_paste),
                    onPressed: () async {
                      final data = await Clipboard.getData('text/plain');
                      _uri.text = data?.text?.trim() ?? '';
                    },
                  ),
                ),
                maxLines: 2,
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _button('Probe LAN', _actions['probe']!),
                  _button('Pair', _actions['pair']!),
                  _button('Connect', _actions['connect']!),
                  _button('Draw square', _actions['square']!),
                  _button('Latency test', _actions['latency']!),
                  _button('Disconnect', _actions['disconnect']!),
                ],
              ),
              const SizedBox(height: 8),
              if (_busy) const LinearProgressIndicator(),
              const Divider(),
              Expanded(
                child: ListView.builder(
                  itemCount: _log.length,
                  itemBuilder: (context, i) => Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: SelectableText(
                      _log[i],
                      style: const TextStyle(fontFamily: 'Menlo', fontSize: 12),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
