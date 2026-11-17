/// The editable configuration document.
///
/// The interface never re-implements validation: it keeps a JSON draft and asks
/// the agent to run the exact same strict parser the DNS service uses. This
/// file only models the document so the structured pages can read and write it
/// with the same defaults and normalization rules as the Rust schema.
library;

import 'dart:convert';

import 'agent_client.dart';

class ConfigDocument {
  ConfigDocument({
    required this.path,
    required this.saved,
    required this.draft,
    required this.exists,
    required this.value,
  });

  factory ConfigDocument.fromSnapshot(ConfigSnapshot snapshot) {
    final value = _decodeObject(snapshot.contents);
    return ConfigDocument(
      path: snapshot.path,
      saved: snapshot.contents,
      draft: snapshot.contents,
      exists: snapshot.exists,
      value: value,
    );
  }

  final String path;
  String saved;
  String draft;
  bool exists;

  /// The parsed draft. Editing a structured field mutates this object and
  /// re-serializes [draft]; editing raw JSON replaces both.
  Map<String, dynamic> value;

  /// The authoritative validation message from the agent, or `null` when the
  /// document is valid.
  String? validationError;
  bool validating = false;

  bool get isValid => validationError == null && !validating;
  bool get isDirty => draft != saved;

  String get validationSummary {
    if (validating) {
      return 'Validating';
    }
    if (validationError != null) {
      return 'Invalid configuration: $validationError';
    }
    if (isDirty) {
      return 'Valid draft, not saved';
    }
    return 'Configuration saved';
  }

  static Map<String, dynamic> _decodeObject(String contents) {
    try {
      final decoded = jsonDecode(contents);
      if (decoded is Map<String, dynamic>) {
        return decoded;
      }
    } on FormatException {
      // An unparsable draft keeps an empty object so the structured pages stay
      // usable; the agent reports the syntax error on the JSON page.
    }
    return <String, dynamic>{};
  }

  /// Replaces the draft from the raw JSON editor. A syntactically invalid
  /// document keeps the previous structured model.
  void replaceDraft(String contents) {
    draft = contents;
    final decoded = _decodeObject(contents);
    if (decoded.isNotEmpty || contents.trim() == '{}') {
      value = decoded;
    }
  }

  /// Re-serializes the structured model into the draft.
  String serialize() {
    draft = const JsonEncoder.withIndent('  ').convert(value);
    return draft;
  }

  // --- Listener -------------------------------------------------------------

  Map<String, dynamic> get listener =>
      _object(value, 'listener');

  String get listenerAddress =>
      listener['address'] as String? ?? '127.0.0.1:53';

  bool get allowRemote => listener['allow_remote'] as bool? ?? false;

  void setListenerAddress(String address) {
    listener['address'] = address;
    serialize();
  }

  void setAllowRemote(bool allow) {
    listener['allow_remote'] = allow;
    serialize();
  }

  // --- Top level ------------------------------------------------------------

  String get entry => value['entry'] as String? ?? '';

  void setEntry(String entry) {
    value['entry'] = entry;
    serialize();
  }

  int get requestTimeoutMs => _number(value, 'request_timeout_ms', 8000);

  void setRequestTimeoutMs(int timeout) {
    value['request_timeout_ms'] = timeout;
    serialize();
  }

  int get rangeRefreshSecs =>
      _number(_object(value, 'cloudflare'), 'range_refresh_secs', 86400);

  void setRangeRefreshSecs(int seconds) {
    _object(value, 'cloudflare')['range_refresh_secs'] = seconds;
    serialize();
  }

  // --- Layers ---------------------------------------------------------------

  List<dynamic> get layers => _array(value, 'layers');

  Map<String, dynamic>? layerAt(int index) {
    if (index < 0 || index >= layers.length) {
      return null;
    }
    final entry = layers[index];
    return entry is Map<String, dynamic> ? entry : null;
  }

  List<String> get layerTags => [
        for (final layer in layers)
          if (layer is Map<String, dynamic>) layer['tag'] as String? ?? '',
      ];

  int addLayer(LayerKind kind) {
    final tag = _uniqueTag('new-layer', layerTags);
    layers.add(_defaultLayer(tag, kind));
    serialize();
    return layers.length - 1;
  }

  void removeLayer(int index) {
    if (index >= 0 && index < layers.length) {
      layers.removeAt(index);
      serialize();
    }
  }

  void moveLayer(int index, {required bool upward}) {
    final destination = upward ? index - 1 : index + 1;
    if (index < 0 || index >= layers.length) {
      return;
    }
    if (destination < 0 || destination >= layers.length) {
      return;
    }
    final moved = layers.removeAt(index);
    layers.insert(destination, moved);
    serialize();
  }

  void setLayerField(int index, String key, Object? fieldValue) {
    final layer = layerAt(index);
    if (layer == null) {
      return;
    }
    if (fieldValue == null ||
        (fieldValue is String && fieldValue.trim().isEmpty)) {
      layer.remove(key);
    } else {
      layer[key] = fieldValue;
    }
    serialize();
  }

  void setLayerType(int index, LayerKind kind) {
    final layer = layerAt(index);
    if (layer == null) {
      return;
    }
    layer['type'] = kind.wire;
    _normalizeLayer(layer, kind);
    serialize();
  }

  void setLayerMatchMode(int index, MatchMode mode) {
    final layer = layerAt(index);
    if (layer == null) {
      return;
    }
    _object(layer, 'match')['mode'] = mode.wire;
    serialize();
  }

  void setLayerMatchList(int index, String key, List<String> values) {
    final layer = layerAt(index);
    if (layer == null) {
      return;
    }
    final match = _object(layer, 'match');
    if (values.isEmpty) {
      match.remove(key);
    } else {
      match[key] = values;
    }
    serialize();
  }

  // --- Rule sets ------------------------------------------------------------

  List<dynamic> get ruleSets => _array(value, 'rule_sets');

  Map<String, dynamic>? ruleSetAt(int index) {
    if (index < 0 || index >= ruleSets.length) {
      return null;
    }
    final entry = ruleSets[index];
    return entry is Map<String, dynamic> ? entry : null;
  }

  List<String> get ruleSetTags => [
        for (final ruleSet in ruleSets)
          if (ruleSet is Map<String, dynamic>)
            ruleSet['tag'] as String? ?? '',
      ];

  int addRuleSet(RuleSetKind kind) {
    final tag = _uniqueTag('new-rule-set', ruleSetTags);
    ruleSets.add(_defaultRuleSet(tag, kind));
    serialize();
    return ruleSets.length - 1;
  }

  void removeRuleSet(int index) {
    if (index >= 0 && index < ruleSets.length) {
      ruleSets.removeAt(index);
      serialize();
    }
  }

  void setRuleSetType(int index, RuleSetKind kind) {
    final ruleSet = ruleSetAt(index);
    if (ruleSet == null) {
      return;
    }
    ruleSet['type'] = kind.wire;
    _normalizeRuleSet(ruleSet, kind);
    serialize();
  }

  void setRuleSetField(int index, String key, Object? fieldValue) {
    final ruleSet = ruleSetAt(index);
    if (ruleSet == null) {
      return;
    }
    if (fieldValue == null ||
        (fieldValue is String && fieldValue.trim().isEmpty)) {
      ruleSet.remove(key);
    } else {
      ruleSet[key] = fieldValue;
    }
    serialize();
  }

  // --- Plugins --------------------------------------------------------------

  List<dynamic> get plugins => _array(value, 'plugins');

  Map<String, dynamic>? pluginAt(int index) {
    if (index < 0 || index >= plugins.length) {
      return null;
    }
    final entry = plugins[index];
    return entry is Map<String, dynamic> ? entry : null;
  }

  List<String> get pluginTags => [
        for (final plugin in plugins)
          if (plugin is Map<String, dynamic>) plugin['tag'] as String? ?? '',
      ];

  int addPlugin() {
    final tag = _uniqueTag('new-plugin', pluginTags);
    plugins.add(_defaultPlugin(tag));
    serialize();
    return plugins.length - 1;
  }

  void removePlugin(int index) {
    if (index >= 0 && index < plugins.length) {
      plugins.removeAt(index);
      serialize();
    }
  }

  void setPluginField(int index, String key, Object? fieldValue) {
    final plugin = pluginAt(index);
    if (plugin == null) {
      return;
    }
    if (fieldValue == null ||
        (fieldValue is String && fieldValue.trim().isEmpty)) {
      plugin.remove(key);
    } else {
      plugin[key] = fieldValue;
    }
    serialize();
  }

  void setPreferred(int index, String key, String? address) {
    final plugin = pluginAt(index);
    if (plugin == null) {
      return;
    }
    final preferred = _object(plugin, 'preferred');
    if (address == null || address.trim().isEmpty) {
      preferred.remove(key);
    } else {
      preferred[key] = address.trim();
    }
    serialize();
  }

  void setOptimizerField(int index, String key, Object? fieldValue) {
    final plugin = pluginAt(index);
    if (plugin == null) {
      return;
    }
    final optimizer = _object(plugin, 'optimizer');
    if (fieldValue == null) {
      optimizer.remove(key);
    } else {
      optimizer[key] = fieldValue;
    }
    serialize();
  }

  void setOptimizerList(int index, String key, List<String> values) {
    final plugin = pluginAt(index);
    if (plugin == null) {
      return;
    }
    _object(plugin, 'optimizer')[key] = values;
    serialize();
  }

  /// A short description of the resolver chain for the overview page.
  String resolverSummary() {
    final buffer = StringBuffer('Entry: ${entry.isEmpty ? '(none)' : entry}');
    for (final layer in layers) {
      if (layer is! Map<String, dynamic>) {
        continue;
      }
      final tag = layer['tag'] as String? ?? 'layer';
      final kind = layer['type'] as String? ?? '?';
      final next = layer['next'] as String?;
      final fallback = layer['fallback'] as String?;
      final plugin = layer['plugin'] as String?;
      buffer.write('\n$tag ($kind)');
      if (next != null) {
        buffer.write(' -> $next');
      }
      if (fallback != null) {
        buffer.write(' | fallback $fallback');
      }
      if (plugin != null) {
        buffer.write(' | plugin: $plugin');
      }
    }
    buffer.write(
      '\nRule sets: ${ruleSets.length}   Plugins: ${plugins.length}',
    );
    return buffer.toString();
  }
}

enum LayerKind {
  udp('udp'),
  tcp('tcp'),
  doh('doh'),
  dot('dot'),
  local('local');

  const LayerKind(this.wire);

  final String wire;

  static const List<LayerKind> all = [udp, tcp, doh, dot, local];

  static LayerKind fromWire(String? wire) => all.firstWhere(
        (kind) => kind.wire == wire,
        orElse: () => LayerKind.local,
      );
}

enum MatchMode {
  label('label'),
  contains('contains');

  const MatchMode(this.wire);

  final String wire;

  static const List<MatchMode> all = [label, contains];

  static MatchMode fromWire(String? wire) => wire == 'contains'
      ? MatchMode.contains
      : MatchMode.label;
}

enum RuleSetKind {
  remote('remote'),
  local('local');

  const RuleSetKind(this.wire);

  final String wire;

  static const List<RuleSetKind> all = [remote, local];

  static RuleSetKind fromWire(String? wire) =>
      wire == 'local' ? RuleSetKind.local : RuleSetKind.remote;
}

Map<String, dynamic> _object(Map<String, dynamic> parent, String key) {
  final existing = parent[key];
  if (existing is Map<String, dynamic>) {
    return existing;
  }
  final created = <String, dynamic>{};
  parent[key] = created;
  return created;
}

List<dynamic> _array(Map<String, dynamic> parent, String key) {
  final existing = parent[key];
  if (existing is List<dynamic>) {
    return existing;
  }
  final created = <dynamic>[];
  parent[key] = created;
  return created;
}

int _number(Map<String, dynamic> parent, String key, int fallback) {
  final existing = parent[key];
  return existing is int ? existing : fallback;
}

String _uniqueTag(String prefix, List<String> existing) {
  if (!existing.contains(prefix)) {
    return prefix;
  }
  var index = 2;
  while (existing.contains('$prefix-$index')) {
    index += 1;
  }
  return '$prefix-$index';
}

Map<String, dynamic> _defaultLayer(String tag, LayerKind kind) {
  final layer = <String, dynamic>{'tag': tag, 'type': kind.wire};
  _normalizeLayer(layer, kind);
  return layer;
}

/// Applies the same per-kind field normalization as `normalize_layer` in
/// `src/config.rs`, so a type switch never leaves a field the schema rejects.
void _normalizeLayer(Map<String, dynamic> layer, LayerKind kind) {
  switch (kind) {
    case LayerKind.udp:
    case LayerKind.tcp:
      layer.remove('refresh_secs');
      layer.remove('url');
      layer.remove('server_name');
      layer.putIfAbsent('address', () => '1.1.1.1:53');
      layer.putIfAbsent('timeout_ms', () => 3000);
    case LayerKind.doh:
      layer.remove('refresh_secs');
      layer.remove('server_name');
      layer.putIfAbsent('address', () => '1.1.1.1:443');
      layer.putIfAbsent(
        'url',
        () => 'https://cloudflare-dns.com/dns-query',
      );
      layer.putIfAbsent('timeout_ms', () => 3000);
    case LayerKind.dot:
      layer.remove('refresh_secs');
      layer.remove('url');
      layer.putIfAbsent('address', () => '1.1.1.1:853');
      layer.putIfAbsent('server_name', () => 'cloudflare-dns.com');
      layer.putIfAbsent('timeout_ms', () => 3000);
    case LayerKind.local:
      layer.remove('address');
      layer.remove('url');
      layer.remove('server_name');
      layer.putIfAbsent('timeout_ms', () => 1800);
      layer.putIfAbsent('refresh_secs', () => 30);
  }
}

Map<String, dynamic> _defaultRuleSet(String tag, RuleSetKind kind) {
  final ruleSet = <String, dynamic>{'tag': tag, 'type': kind.wire};
  _normalizeRuleSet(ruleSet, kind);
  return ruleSet;
}

void _normalizeRuleSet(Map<String, dynamic> ruleSet, RuleSetKind kind) {
  switch (kind) {
    case RuleSetKind.remote:
      ruleSet.remove('path');
      ruleSet.putIfAbsent('url', () => 'https://example.com/rules.srs');
      ruleSet.putIfAbsent('update_interval_secs', () => 86400);
      ruleSet.putIfAbsent('timeout_ms', () => 10000);
    case RuleSetKind.local:
      ruleSet.remove('url');
      ruleSet.remove('timeout_ms');
      ruleSet.putIfAbsent('path', () => '/path/to/rules.srs');
      ruleSet.putIfAbsent('update_interval_secs', () => 60);
  }
}

Map<String, dynamic> _defaultPlugin(String tag) => <String, dynamic>{
      'tag': tag,
      'type': 'cloudflare_preferred',
      'rewrite_ttl_secs': 60,
      'preferred': <String, dynamic>{},
      'optimizer': <String, dynamic>{
        'enabled': false,
        'interval_secs': 21600,
        'test_host': 'www.cloudflare.com',
        'test_path': '/cdn-cgi/trace',
        'test_port': 443,
        'timeout_ms': 3000,
        'concurrency': 32,
        'samples_per_cidr': 40,
        'probes_per_candidate': 3,
        'compatibility_hosts': <String>[],
        'excluded_candidates': <String>[],
        'max_candidates': 640,
        'candidates': <String>[],
      },
    };

/// Splits a comma separated field into trimmed, non-empty entries.
List<String> parseList(String value) => value
    .split(',')
    .map((entry) => entry.trim())
    .where((entry) => entry.isNotEmpty)
    .toList(growable: false);

String joinList(Object? value) {
  if (value is List<dynamic>) {
    return value.map((entry) => '$entry').join(', ');
  }
  return '';
}
