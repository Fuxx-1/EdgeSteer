import 'dart:convert';

import 'package:edgesteer_ui/agent_client.dart';
import 'package:edgesteer_ui/config_document.dart';
import 'package:flutter_test/flutter_test.dart';

ConfigDocument documentFrom(String contents) => ConfigDocument.fromSnapshot(
      ConfigSnapshot(path: '/tmp/edgesteer.json', contents: contents, exists: true),
    );

const String exampleConfig = '''
{
  "listener": { "address": "127.0.0.1:53535", "allow_remote": false },
  "cloudflare": { "range_refresh_secs": 86400 },
  "request_timeout_ms": 8000,
  "entry": "entry",
  "rule_sets": [],
  "plugins": [],
  "layers": [
    { "tag": "entry", "type": "local", "timeout_ms": 1800, "refresh_secs": 30 }
  ]
}
''';

void main() {
  test('reads listener, entry, and layer values', () {
    final document = documentFrom(exampleConfig);
    expect(document.listenerAddress, '127.0.0.1:53535');
    expect(document.allowRemote, isFalse);
    expect(document.entry, 'entry');
    expect(document.requestTimeoutMs, 8000);
    expect(document.rangeRefreshSecs, 86400);
    expect(document.layers.length, 1);
    expect(document.layerAt(0)?['type'], 'local');
  });

  test('structured edits re-serialize the draft', () {
    final document = documentFrom(exampleConfig);
    document.setListenerAddress('127.0.0.1:53');
    document.setEntry('cn');
    document.setRangeRefreshSecs(3600);

    final decoded = jsonDecode(document.draft) as Map<String, dynamic>;
    expect((decoded['listener'] as Map<String, dynamic>)['address'],
        '127.0.0.1:53');
    expect(decoded['entry'], 'cn');
    expect((decoded['cloudflare'] as Map<String, dynamic>)['range_refresh_secs'],
        3600);
  });

  test('switching a layer type drops fields the schema rejects', () {
    final document = documentFrom(exampleConfig);
    document.setLayerType(0, LayerKind.doh);

    final layer = document.layerAt(0)!;
    expect(layer['type'], 'doh');
    expect(layer.containsKey('refresh_secs'), isFalse);
    expect(layer['url'], isNotNull);
    expect(layer['address'], isNotNull);
  });

  test('switching to local drops network-only fields', () {
    final document = documentFrom(exampleConfig);
    document.setLayerType(0, LayerKind.doh);
    document.setLayerType(0, LayerKind.local);

    final layer = document.layerAt(0)!;
    expect(layer['type'], 'local');
    expect(layer.containsKey('url'), isFalse);
    expect(layer.containsKey('address'), isFalse);
    expect(layer['refresh_secs'], 30);
  });

  test('adding layers and rule sets produces unique tags', () {
    final document = documentFrom(exampleConfig);
    document.addLayer(LayerKind.local);
    document.addLayer(LayerKind.local);
    expect(document.layerTags, ['entry', 'new-layer', 'new-layer-2']);

    document.addRuleSet(RuleSetKind.remote);
    document.addRuleSet(RuleSetKind.remote);
    expect(document.ruleSetTags, ['new-rule-set', 'new-rule-set-2']);
  });

  test('moving and removing layers keeps the list consistent', () {
    final document = documentFrom(exampleConfig);
    document.addLayer(LayerKind.doh);
    expect(document.layerTags, ['entry', 'new-layer']);

    document.moveLayer(1, upward: true);
    expect(document.layerTags, ['new-layer', 'entry']);

    document.removeLayer(0);
    expect(document.layerTags, ['entry']);
  });

  test('empty optional values are removed instead of written as empty strings',
      () {
    final document = documentFrom(exampleConfig);
    document.setLayerField(0, 'next', '');
    document.setLayerField(0, 'fallback', 'other');
    document.setLayerField(0, 'fallback', '');

    final layer = document.layerAt(0)!;
    expect(layer.containsKey('next'), isFalse);
    expect(layer.containsKey('fallback'), isFalse);
  });

  test('match lists are written as arrays and cleared when empty', () {
    final document = documentFrom(exampleConfig);
    document.setLayerMatchList(0, 'keywords', parseList('b2c, mi ,local'));
    expect((document.layerAt(0)!['match'] as Map<String, dynamic>)['keywords'],
        ['b2c', 'mi', 'local']);

    document.setLayerMatchList(0, 'keywords', parseList('  '));
    expect(
      (document.layerAt(0)!['match'] as Map<String, dynamic>)
          .containsKey('keywords'),
      isFalse,
    );
  });

  test('dirty state tracks the saved baseline', () {
    final document = documentFrom(exampleConfig);
    expect(document.isDirty, isFalse);
    document.setEntry('other');
    expect(document.isDirty, isTrue);
    document.saved = document.draft;
    expect(document.isDirty, isFalse);
  });

  test('an unparsable draft keeps the structured model usable', () {
    final document = documentFrom(exampleConfig);
    document.replaceDraft('{ not json');
    expect(document.draft, '{ not json');
    // The previous model is retained so the form pages do not go blank while
    // the agent reports the syntax error.
    expect(document.layers.length, 1);
  });

  test('list helpers trim and drop empty entries', () {
    expect(parseList('a, ,b ,'), ['a', 'b']);
    expect(joinList(['a', 'b']), 'a, b');
    expect(joinList(null), '');
  });
}
