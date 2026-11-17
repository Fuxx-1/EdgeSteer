/// Resolver chain editor: listener settings above, layer master-detail below.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';
import 'editor_scaffold.dart';

class ResolverPage extends StatefulWidget {
  const ResolverPage({
    super.key,
    required this.strings,
    required this.onNotice,
  });

  final Strings strings;
  final ValueChanged<String> onNotice;

  @override
  State<ResolverPage> createState() => _ResolverPageState();
}

class _ResolverPageState extends State<ResolverPage>
    with FieldBinding<ResolverPage> {
  int _selected = 0;

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final document = controller.document;
    final strings = widget.strings;
    if (document == null) {
      return AgentUnavailable(strings: strings);
    }

    final layerCount = document.layers.length;
    if (_selected >= layerCount) {
      _selected = layerCount == 0 ? 0 : layerCount - 1;
    }
    final layer = document.layerAt(_selected);
    final kind = LayerKind.fromWire(layer?['type'] as String?);
    final match = layer?['match'];
    final matchMap =
        match is Map<String, dynamic> ? match : const <String, dynamic>{};
    final mode = MatchMode.fromWire(matchMap['mode'] as String?);

    syncFields(
      fieldSignature([
        _selected,
        layerCount,
        layer?['tag'],
        layer?['type'],
        document.draft.length,
        document.entry,
      ]),
      () {
        textField('listener').text = document.listenerAddress;
        textField('entry').text = document.entry;
        textField('timeout').text = '${document.requestTimeoutMs}';
        textField('range').text = '${document.rangeRefreshSecs}';
        textField('tag').text = layer?['tag'] as String? ?? '';
        textField('next').text = layer?['next'] as String? ?? '';
        textField('fallback').text = layer?['fallback'] as String? ?? '';
        textField('plugin').text = layer?['plugin'] as String? ?? '';
        textField('address').text = layer?['address'] as String? ?? '';
        textField('url').text = layer?['url'] as String? ?? '';
        textField('serverName').text = layer?['server_name'] as String? ?? '';
        textField('layerTimeout').text = '${layer?['timeout_ms'] ?? 3000}';
        textField('refresh').text = '${layer?['refresh_secs'] ?? 30}';
        textField('keywords').text = joinList(matchMap['keywords']);
        textField('ruleSets').text = joinList(matchMap['rule_sets']);
      },
    );

    void edit(void Function(ConfigDocument document) change) =>
        applyEdit(controller, this, change);

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          EdgeGroup(
            children: [
              FormRow(
                label: strings.listener,
                child: EdgeField(
                  controller: textField('listener'),
                  hint: '127.0.0.1:53',
                  monospace: true,
                  onChanged: (value) =>
                      edit((document) => document.setListenerAddress(value)),
                ),
              ),
              FormRow(
                label: strings.entry,
                hint: document.entry.isNotEmpty &&
                        !document.layerTags.contains(document.entry)
                    ? strings.entryMissing
                    : null,
                child: EdgeField(
                  controller: textField('entry'),
                  hint: 'local-keyword',
                  onChanged: (value) =>
                      edit((document) => document.setEntry(value)),
                ),
              ),
              FormRowPair(
                left: FormRow(
                  label: strings.requestTimeout,
                  child: EdgeField(
                    controller: textField('timeout'),
                    numeric: true,
                    onChanged: (value) {
                      final parsed = int.tryParse(value);
                      if (parsed != null) {
                        edit((document) => document.setRequestTimeoutMs(parsed));
                      }
                    },
                  ),
                ),
                right: FormRow(
                  label: strings.rangeRefresh,
                  child: EdgeField(
                    controller: textField('range'),
                    numeric: true,
                    onChanged: (value) {
                      final parsed = int.tryParse(value);
                      if (parsed != null) {
                        edit((document) => document.setRangeRefreshSecs(parsed));
                      }
                    },
                  ),
                ),
              ),
              FormRow(
                label: strings.allowRemote,
                child: FormSwitch(
                  value: document.allowRemote,
                  onChanged: (value) =>
                      edit((document) => document.setAllowRemote(value)),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Expanded(
            child: EditorScaffold(
              strings: strings,
              listTitle: strings.layers,
              addLabel: strings.add,
              removeLabel: strings.remove,
              emptyLabel: strings.noLayers,
              onAdd: () => setState(() {
                _selected = document.addLayer(LayerKind.local);
              }),
              onRemove: () => setState(() {
                document.removeLayer(_selected);
                invalidateFields();
                _selected = layerCount <= 1 ? 0 : _selected - 1;
              }),
              entries: [
                for (var index = 0; index < layerCount; index++)
                  _layerEntry(document, index, strings),
              ],
              selected: _selected,
              onSelect: (index) => setState(() {
                _selected = index;
                invalidateFields();
              }),
              detail: _LayerDetail(
                strings: strings,
                document: document,
                index: _selected,
                kind: kind,
                mode: mode,
                controllers: {
                  for (final key in const [
                    'tag',
                    'next',
                    'fallback',
                    'plugin',
                    'address',
                    'url',
                    'serverName',
                    'layerTimeout',
                    'refresh',
                    'keywords',
                    'ruleSets',
                  ])
                    key: textField(key),
                },
                onEdit: edit,
              ),
            ),
          ),
        ],
      ),
    );
  }

  ListEntry _layerEntry(
    ConfigDocument document,
    int index,
    Strings strings,
  ) {
    final layer = document.layerAt(index);
    final tag = layer?['tag'] as String? ?? '';
    final kind = LayerKind.fromWire(layer?['type'] as String?);
    final next = layer?['next'] as String?;
    final fallback = layer?['fallback'] as String?;
    // Two links on one line truncated both of them, so each gets its own row.
    final lines = [
      if (next != null && next.isNotEmpty)
        (label: strings.next, value: next),
      if (fallback != null && fallback.isNotEmpty)
        (label: strings.fallback, value: fallback),
    ];
    return ListEntry(
      title: tag.isEmpty ? '—' : tag,
      subtitle: kind.wire,
      lines: lines,
      badge: document.entry == tag ? strings.entry.toUpperCase() : null,
    );
  }
}

class _LayerDetail extends StatelessWidget {
  const _LayerDetail({
    required this.strings,
    required this.document,
    required this.index,
    required this.kind,
    required this.mode,
    required this.controllers,
    required this.onEdit,
  });

  final Strings strings;
  final ConfigDocument document;
  final int index;
  final LayerKind kind;
  final MatchMode mode;
  final Map<String, TextEditingController> controllers;
  final void Function(void Function(ConfigDocument document)) onEdit;

  @override
  Widget build(BuildContext context) {
    final layer = document.layerAt(index);
    if (layer == null) {
      return const SizedBox.shrink();
    }

    void setField(String key, Object? value) =>
        onEdit((document) => document.setLayerField(index, key, value));

    return EdgeGroup(
      scrollable: true,
      children: [
        FormRow(
          label: strings.tag,
          child: EdgeField(
            controller: controllers['tag']!,
            onChanged: (value) => setField('tag', value),
          ),
        ),
        FormRow(
          label: strings.type,
          child: EdgeSelect<LayerKind>(
            value: kind,
            items: [
              for (final option in LayerKind.all)
                (value: option, label: _kindLabel(option, strings)),
            ],
            onChanged: (value) =>
                onEdit((document) => document.setLayerType(index, value)),
          ),
        ),
        FormRowPair(
          left: FormRow(
            label: strings.next,
            child: EdgeField(
              controller: controllers['next']!,
              hint: '—',
              onChanged: (value) => setField('next', value),
            ),
          ),
          right: FormRow(
            label: strings.fallback,
            child: EdgeField(
              controller: controllers['fallback']!,
              hint: '—',
              onChanged: (value) => setField('fallback', value),
            ),
          ),
        ),
        FormRow(
          label: strings.plugin,
          child: EdgeField(
            controller: controllers['plugin']!,
            hint: '—',
            onChanged: (value) => setField('plugin', value),
          ),
        ),
        if (kind != LayerKind.local) ...[
          const _Rule(),
          FormRow(
            label: strings.address,
            child: EdgeField(
              controller: controllers['address']!,
              monospace: true,
              onChanged: (value) => setField('address', value),
            ),
          ),
          if (kind == LayerKind.doh)
            FormRow(
              label: strings.url,
              child: EdgeField(
                controller: controllers['url']!,
                monospace: true,
                onChanged: (value) => setField('url', value),
              ),
            ),
          if (kind == LayerKind.dot)
            FormRow(
              label: strings.serverName,
              child: EdgeField(
                controller: controllers['serverName']!,
                onChanged: (value) => setField('server_name', value),
              ),
            ),
        ],
        FormRowPair(
          left: FormRow(
            label: strings.timeout,
            child: EdgeField(
              controller: controllers['layerTimeout']!,
              numeric: true,
              onChanged: (value) {
                final parsed = int.tryParse(value);
                if (parsed != null) {
                  setField('timeout_ms', parsed);
                }
              },
            ),
          ),
          right: kind == LayerKind.local
              ? FormRow(
                  label: strings.layerRefresh,
                  child: EdgeField(
                    controller: controllers['refresh']!,
                    numeric: true,
                    onChanged: (value) {
                      final parsed = int.tryParse(value);
                      if (parsed != null) {
                        setField('refresh_secs', parsed);
                      }
                    },
                  ),
                )
              : const SizedBox.shrink(),
        ),
        const _Rule(),
        FormRow(
          label: strings.matchMode,
          child: EdgeSelect<MatchMode>(
            value: mode,
            items: [
              for (final option in MatchMode.all)
                (
                  value: option,
                  label: option == MatchMode.label
                      ? strings.labelMode
                      : strings.containsMode,
                ),
            ],
            onChanged: (value) =>
                onEdit((document) => document.setLayerMatchMode(index, value)),
          ),
        ),
        FormRow(
          label: strings.keywords,
          child: EdgeField(
            controller: controllers['keywords']!,
            hint: 'b2c, mi',
            onChanged: (value) => onEdit(
              (document) =>
                  document.setLayerMatchList(index, 'keywords', parseList(value)),
            ),
          ),
        ),
        FormRow(
          label: strings.matchRuleSets,
          child: EdgeField(
            controller: controllers['ruleSets']!,
            hint: 'geosite-cn',
            onChanged: (value) => onEdit(
              (document) => document.setLayerMatchList(
                  index, 'rule_sets', parseList(value)),
            ),
          ),
        ),
      ],
    );
  }

  String _kindLabel(LayerKind kind, Strings strings) => switch (kind) {
        LayerKind.udp => 'UDP',
        LayerKind.tcp => 'TCP',
        LayerKind.doh => 'DoH',
        LayerKind.dot => 'DoT',
        LayerKind.local => strings.localLayer,
      };
}

class _Rule extends StatelessWidget {
  const _Rule();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Divider(
        height: 1,
        color: Theme.of(context).colorScheme.outlineVariant,
      ),
    );
  }
}
