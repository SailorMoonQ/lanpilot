import '../bridge/lanpilot_client.dart';

/// What the connection manager needs from discovery.
abstract interface class DiscoveryLookup {
  DiscoveredInfo? lookup(String shortId);

  /// Completes with the device once Bonjour reports it, or null after
  /// `timeout`.
  Future<DiscoveredInfo?> waitFor(String shortId, Duration timeout);
}
