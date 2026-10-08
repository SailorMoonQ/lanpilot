import 'package:flutter_test/flutter_test.dart';
import 'package:lanpilot_discovery/lanpilot_discovery.dart';

void main() {
  test('parses a found event', () {
    final s = RawService.fromEvent({
      'event': 'found',
      'fullname': 'abc._lanpilot._udp.local.',
      'txt': {'id': 'abc', 'bad': 3},
      'addrs': ['10.0.0.2', 7],
      'port': 45810,
    })!;
    expect(s.found, isTrue);
    expect(s.txt, {'id': 'abc'});
    expect(s.addrs, ['10.0.0.2']);
    expect(s.port, 45810);
  });

  test('parses lost and drops junk', () {
    final lost = RawService.fromEvent({'event': 'lost', 'fullname': 'x'})!;
    expect(lost.found, isFalse);
    expect(RawService.fromEvent({'event': 'error', 'message': 'm'}), isNull);
    expect(RawService.fromEvent('nonsense'), isNull);
    expect(RawService.fromEvent({'event': 'found'}), isNull);
  });
}
