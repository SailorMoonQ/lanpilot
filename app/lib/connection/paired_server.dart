import 'package:flutter/foundation.dart';

import '../bridge/lanpilot_client.dart';

/// A paired computer. The public key lives in the keychain; the rest is in
/// servers.json.
@immutable
class PairedServer {
  const PairedServer({
    required this.shortId,
    required this.publicKeyHex,
    required this.name,
    this.os = OsKind.unknown,
    this.addrs = const [],
    this.lastGoodAddr,
    this.lastUsed,
  });

  factory PairedServer.fromInfo(PairedServerInfo info) => PairedServer(
    shortId: info.shortId,
    publicKeyHex: info.publicKeyHex,
    name: info.name,
    os: info.os,
    addrs: info.addrs,
  );

  final String shortId;
  final String publicKeyHex;
  final String name;
  final OsKind os;

  /// Addresses from pairing or manual entry, "ip:port".
  final List<String> addrs;
  final String? lastGoodAddr;
  final DateTime? lastUsed;

  PairedServer copyWith({
    String? name,
    OsKind? os,
    List<String>? addrs,
    String? lastGoodAddr,
    DateTime? lastUsed,
  }) => PairedServer(
    shortId: shortId,
    publicKeyHex: publicKeyHex,
    name: name ?? this.name,
    os: os ?? this.os,
    addrs: addrs ?? this.addrs,
    lastGoodAddr: lastGoodAddr ?? this.lastGoodAddr,
    lastUsed: lastUsed ?? this.lastUsed,
  );

  Map<String, Object?> toJson() => {
    'shortId': shortId,
    'name': name,
    'os': os.name,
    'addrs': addrs,
    if (lastGoodAddr != null) 'lastGoodAddr': lastGoodAddr,
    if (lastUsed != null) 'lastUsed': lastUsed!.toUtc().toIso8601String(),
  };

  static PairedServer? fromJson(
    Map<String, Object?> json,
    String publicKeyHex,
  ) {
    final shortId = json['shortId'];
    final name = json['name'];
    if (shortId is! String || name is! String) return null;
    final osName = json['os'];
    final rawAddrs = json['addrs'];
    final lastGood = json['lastGoodAddr'];
    final lastUsed = json['lastUsed'];
    return PairedServer(
      shortId: shortId,
      publicKeyHex: publicKeyHex,
      name: name,
      os:
          OsKind.values.where((o) => o.name == osName).firstOrNull ??
          OsKind.unknown,
      addrs: [
        if (rawAddrs is List)
          for (final a in rawAddrs)
            if (a is String) a,
      ],
      lastGoodAddr: lastGood is String ? lastGood : null,
      lastUsed: lastUsed is String ? DateTime.tryParse(lastUsed) : null,
    );
  }
}
