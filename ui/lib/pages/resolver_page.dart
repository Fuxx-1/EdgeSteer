/// Structured editor for the listener, the entry layer, and every resolver
/// layer.
///
/// Fields are text controllers so typing never re-serializes the document on
/// each keystroke; the document is updated on change and the agent revalidates
/// it. Selecting a different layer reloads the controllers from the document.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';

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

class _ResolverPageState extends State<ResolverPage> {
  int _selectedLayer = 0;
  int _loadedSignature = -1;

  final _listenerAddress = TextEditingController();
  final _entryName = TextEditingController();
  final _requestTimeout = TextEditingController();
  final _rangeRefresh = TextEditingController();

  final _layerTag = TextEditingController();
  final _layerNext = TextEditingController();
  final _layerFallback = TextEditingController();
  final _layerPlugin = TextEditingController();
  final _layerAddress = TextEditingController();
  final _layerUrl = TextEditingController();
  final _layerServerName = TextEditingController();
  final _layerTimeout = TextEditingController();
  final _layerRefresh = TextEditingController();
  final _layerKeywords = TextEditingController();
  final _layerRuleSets = TextEditingController();

  @override
  void dispose() {
    for (final controller in [
      _listenerAddress,
      _entryName,
      _requestTimeout,
      _rangeRefresh,
      _layerTag,
      _layerNext,
      _layerFallback,
      _layerPlugin,
      _layerAddress,
      _layerUrl,
      _layerServerName,
      _layerTimeout,
      _layerRefresh,
      _layerKeywords,
      _layerRuleSets,
    ]) {
      controller.dispose();
    }
    super.dispose();
  }

  /// Reloads the text controllers when the selected layer or the underlying
  /// document changes identity (reload, layer add/remove/move).
  void _syncFromDocument(ConfigDocument document) {
    final signature = Object.hash(
      document.layers.length,
      _selectedLayer,
      document.layerAt(_selectedLayer)?['tag'],
      document.layerAt(_selectedLayer)?['type'],
      document.draft.length,
    );
    if (signature == _loadedSignature) {
      return;
    }
    _loadedSignature = signature;

    _listenerAddress.text = document.listenerAddress;
    _entryName.text = document.entry;
    _requestTimeout.text = '${document.requestTimeoutMs}';
    _rangeRefresh.text = '${document.rangeRefreshSecs}';

    final layer = document.layerAt(_selectedLayer);
    _layerTag.text = layer?['tag'] as String? ?? '';
    _layerNext.text = layer?['next'] as String? ?? '';
    _layerFallback.text = layer?['fallback'] as String? ?? '';
    _layerPlugin.text = layer?['plugin'] as String? ?? '';
    _layerAddress.text = layer?['address'] as String? ?? '';
    _layerUrl.text = layer?['url'] as String? ?? '';
    _layerServerName.text = layer?['server_name'] as String? ?? '';
    _layerTimeout.text = '${layer?['timeout_ms'] ?? 3000}';
    _layerRefresh.text = '${layer?['refresh_secs'] ?? 30}';
    final match = layer?['match'];
    if (match is Map<String, dynamic>) {
      _layerKeywords.text = joinList(match['keywords']);
      _layerRuleSets.text = joinList(match['rule_sets']);
    } else {
      _layerKeywords.text = '';
      _layerRuleSets.text = '';
    }
  }

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final document = controller.document;
    final strings = widget.strings;

    if (document == null) {
      return Center(child: Text(strings.agentUnavailable));
    }
    _syncFromDocument(document);

    final layerCount = document.layers.length;
    if (_selectedLayer >= layerCount) {
      _selectedLayer = layerCount == 0 ? 0 : layerCount - 1;
    }
    final layer = document.layerAt(_selectedLayer);
    final kind = LayerKind.fromWire(layer?['type'] as String?);
    final match = layer?['match'];
    final mode = MatchMode.fromWire(
      match is Map<String, dynamic> ? match['mode'] as String? : null,
    );

    void edit(void Function(ConfigDocument document) change) {
      controller.editDocument(change);
      _loadedSignature = -1;
    }

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(32, 26, 32, 32),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          PageHeader(title: strings.resolversTitle, copy: strings.resolversCopy),
          const SizedBox(height: 20),
          EdgePanel(
            title: strings.entryAndListener,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    flex: 2,
                    child: LabeledField(
                      label: strings.listenerAddress,
                      child: EdgeTextField(
                        controller: _listenerAddress,
                        hint: '127.0.0.1:53',
                        onChanged: (value) => edit(
                          (document) => document.setListenerAddress(value),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    flex: 2,
                    child: LabeledField(
                      label: strings.entryLayer,
                      child: EdgeTextField(
                        controller: _entryName,
                        hint: 'local-keyword',
                        onChanged: (value) => edit(
                          (document) => document.setEntry(value),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: LabeledField(
                      label: strings.requestTimeout,
                      child: EdgeTextField(
                        controller: _requestTimeout,
                        numeric: true,
                        hint: '8000',
                        onChanged: (value) {
                          final parsed = int.tryParse(value);
                          if (parsed != null) {
                            edit(
                              (document) =>
                                  document.setRequestTimeoutMs(parsed),
                            );
                          }
                        },
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: LabeledField(
                      label: strings.rangeRefresh,
                      child: EdgeTextField(
                        controller: _rangeRefresh,
                        numeric: true,
                        hint: '86400',
                        onChanged: (value) {
                          final parsed = int.tryParse(value);
                          if (parsed != null) {
                            edit(
                              (document) =>
                                  document.setRangeRefreshSecs(parsed),
                            );
                          }
                        },
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 14),
              Row(
                children: [
                  Checkbox(
                    value: document.allowRemote,
                    onChanged: (value) => edit(
                      (document) => document.setAllowRemote(value ?? false),
                    ),
                  ),
                  Text(
                    strings.allowRemote,
                    style: const TextStyle(fontSize: 13),
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.resolversTitle,
            actions: ActionRow(
              children: [
                OutlinedButton(
                  onPressed: () => edit((document) {
                    _selectedLayer = document.addLayer(LayerKind.local);
                  }),
                  child: Text(strings.addLayer),
                ),
                TextButton(
                  onPressed: _selectedLayer <= 0
                      ? null
                      : () => edit((document) {
                            document.moveLayer(_selectedLayer, upward: true);
                            _selectedLayer -= 1;
                          }),
                  child: Text(strings.moveUp),
                ),
                TextButton(
                  onPressed: _selectedLayer >= layerCount - 1
                      ? null
                      : () => edit((document) {
                            document.moveLayer(_selectedLayer, upward: false);
                            _selectedLayer += 1;
                          }),
                  child: Text(strings.moveDown),
                ),
                TextButton(
                  onPressed: layerCount == 0
                      ? null
                      : () => edit((document) {
                            document.removeLayer(_selectedLayer);
                            _selectedLayer = _selectedLayer
                                .clamp(0, document.layers.length - 1);
                          }),
                  style: TextButton.styleFrom(
                    foregroundColor: Theme.of(context).colorScheme.error,
                  ),
                  child: Text(strings.remove),
                ),
              ],
            ),
            children: [
              if (layerCount == 0)
                Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Text(
                    strings.noLayers,
                    style: TextStyle(
                      fontSize: 12.5,
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ),
                ),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    flex: 3,
                    child: LabeledField(
                      label: strings.layerTag,
                      child: Container(
                        height: 40,
                        padding: const EdgeInsets.symmetric(horizontal: 10),
                        decoration: BoxDecoration(
                          color: Theme.of(context)
                              .colorScheme
                              .surfaceContainerHighest
                              .withValues(alpha: 0.45),
                          borderRadius: BorderRadius.circular(6),
                          border: Border.all(
                            color: Theme.of(context).colorScheme.outlineVariant,
                          ),
                        ),
                        child: DropdownButtonHideUnderline(
                          child: DropdownButton<int>(
                            value: layerCount == 0 ? null : _selectedLayer,
                            isExpanded: true,
                            isDense: true,
                            hint: Text(
                              strings.addLayer,
                              style: const TextStyle(fontSize: 13),
                            ),
                            items: [
                              for (var index = 0; index < layerCount; index++)
                                DropdownMenuItem<int>(
                                  value: index,
                                  child: Text(
                                    '${(index + 1).toString().padLeft(2, '0')}  '
                                    '${document.layerAt(index)?['tag'] ?? 'layer'}'
                                    ' (${document.layerAt(index)?['type'] ?? '?'})',
                                    overflow: TextOverflow.ellipsis,
                                    style: const TextStyle(fontSize: 13),
                                  ),
                                ),
                            ],
                            onChanged: (value) {
                              if (value != null) {
                                setState(() {
                                  _selectedLayer = value;
                                  _loadedSignature = -1;
                                });
                              }
                            },
                          ),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    flex: 2,
                    child: EdgeDropdown<LayerKind>(
                      label: strings.layerType,
                      value: kind,
                      items: [
                        for (final option in LayerKind.all)
                          (value: option, label: _layerKindLabel(option, strings)),
                      ],
                      onChanged: (value) => edit(
                        (document) => document.setLayerType(_selectedLayer, value),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              _fieldRow(
                children: [
                  LabeledField(
                    label: strings.layerTag,
                    child: EdgeTextField(
                      controller: _layerTag,
                      onChanged: (value) => edit(
                        (document) => document.setLayerField(
                            _selectedLayer, 'tag', value),
                      ),
                    ),
                  ),
                  LabeledField(
                    label: strings.nextLayer,
                    child: EdgeTextField(
                      controller: _layerNext,
                      hint: 'next layer tag',
                      onChanged: (value) => edit(
                        (document) => document.setLayerField(
                            _selectedLayer, 'next', value),
                      ),
                    ),
                  ),
                  LabeledField(
                    label: strings.fallbackLayer,
                    child: EdgeTextField(
                      controller: _layerFallback,
                      hint: 'fallback layer tag',
                      onChanged: (value) => edit(
                        (document) => document.setLayerField(
                            _selectedLayer, 'fallback', value),
                      ),
                    ),
                  ),
                  LabeledField(
                    label: strings.responsePlugin,
                    child: EdgeTextField(
                      controller: _layerPlugin,
                      hint: 'plugin tag (optional)',
                      onChanged: (value) => edit(
                        (document) => document.setLayerField(
                            _selectedLayer, 'plugin', value),
                      ),
                    ),
                  ),
                ],
              ),
              if (kind != LayerKind.local) ...[
                const SizedBox(height: 12),
                _fieldRow(
                  children: [
                    LabeledField(
                      label: strings.upstreamAddress,
                      child: EdgeTextField(
                        controller: _layerAddress,
                        hint: '1.1.1.1:443',
                        onChanged: (value) => edit(
                          (document) => document.setLayerField(
                              _selectedLayer, 'address', value),
                        ),
                      ),
                    ),
                    if (kind == LayerKind.doh)
                      LabeledField(
                        label: strings.dohUrl,
                        child: EdgeTextField(
                          controller: _layerUrl,
                          hint: 'https://.../dns-query',
                          onChanged: (value) => edit(
                            (document) => document.setLayerField(
                                _selectedLayer, 'url', value),
                          ),
                        ),
                      ),
                    if (kind == LayerKind.dot)
                      LabeledField(
                        label: strings.dotServerName,
                        child: EdgeTextField(
                          controller: _layerServerName,
                          hint: 'cloudflare-dns.com',
                          onChanged: (value) => edit(
                            (document) => document.setLayerField(
                                _selectedLayer, 'server_name', value),
                          ),
                        ),
                      ),
                    LabeledField(
                      label: strings.layerTimeout,
                      child: EdgeTextField(
                        controller: _layerTimeout,
                        numeric: true,
                        hint: '3000',
                        onChanged: (value) {
                          final parsed = int.tryParse(value);
                          if (parsed != null) {
                            edit(
                              (document) => document.setLayerField(
                                  _selectedLayer, 'timeout_ms', parsed),
                            );
                          }
                        },
                      ),
                    ),
                  ],
                ),
              ],
              if (kind == LayerKind.local) ...[
                const SizedBox(height: 12),
                _fieldRow(
                  children: [
                    LabeledField(
                      label: strings.layerTimeout,
                      child: EdgeTextField(
                        controller: _layerTimeout,
                        numeric: true,
                        hint: '1800',
                        onChanged: (value) {
                          final parsed = int.tryParse(value);
                          if (parsed != null) {
                            edit(
                              (document) => document.setLayerField(
                                  _selectedLayer, 'timeout_ms', parsed),
                            );
                          }
                        },
                      ),
                    ),
                    LabeledField(
                      label: strings.localRefresh,
                      child: EdgeTextField(
                        controller: _layerRefresh,
                        numeric: true,
                        hint: '30',
                        onChanged: (value) {
                          final parsed = int.tryParse(value);
                          if (parsed != null) {
                            edit(
                              (document) => document.setLayerField(
                                  _selectedLayer, 'refresh_secs', parsed),
                            );
                          }
                        },
                      ),
                    ),
                  ],
                ),
              ],
              const SizedBox(height: 12),
              _fieldRow(
                children: [
                  EdgeDropdown<MatchMode>(
                    label: strings.matchMode,
                    value: mode,
                    items: [
                      for (final option in MatchMode.all)
                        (
                          value: option,
                          label: option == MatchMode.label
                              ? strings.t('完整 DNS 标签', 'Full DNS label')
                              : strings.t('包含关键词', 'Literal substring'),
                        ),
                    ],
                    onChanged: (value) => edit(
                      (document) =>
                          document.setLayerMatchMode(_selectedLayer, value),
                    ),
                  ),
                  LabeledField(
                    label: strings.keywords,
                    child: EdgeTextField(
                      controller: _layerKeywords,
                      hint: 'b2c, mi, local',
                      onChanged: (value) => edit(
                        (document) => document.setLayerMatchList(
                            _selectedLayer, 'keywords', parseList(value)),
                      ),
                    ),
                  ),
                  LabeledField(
                    label: strings.ruleSetTags,
                    child: EdgeTextField(
                      controller: _layerRuleSets,
                      hint: 'geosite-cn',
                      onChanged: (value) => edit(
                        (document) => document.setLayerMatchList(
                            _selectedLayer, 'rule_sets', parseList(value)),
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _fieldRow({required List<Widget> children}) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var index = 0; index < children.length; index++) ...[
          if (index > 0) const SizedBox(width: 12),
          Expanded(child: children[index]),
        ],
      ],
    );
  }

  String _layerKindLabel(LayerKind kind, Strings strings) => switch (kind) {
        LayerKind.udp => 'UDP',
        LayerKind.tcp => 'TCP',
        LayerKind.doh => 'DoH',
        LayerKind.dot => 'DoT',
        LayerKind.local => strings.t('动态本地 DNS', 'Dynamic local DNS'),
      };
}
