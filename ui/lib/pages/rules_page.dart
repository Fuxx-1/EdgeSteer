/// Editor for local and remote SRS rule sets.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';

class RulesPage extends StatefulWidget {
  const RulesPage({
    super.key,
    required this.strings,
    required this.onNotice,
  });

  final Strings strings;
  final ValueChanged<String> onNotice;

  @override
  State<RulesPage> createState() => _RulesPageState();
}

class _RulesPageState extends State<RulesPage> {
  int _selected = 0;
  int _loadedSignature = -1;

  final _tag = TextEditingController();
  final _source = TextEditingController();
  final _interval = TextEditingController();
  final _timeout = TextEditingController();

  @override
  void dispose() {
    _tag.dispose();
    _source.dispose();
    _interval.dispose();
    _timeout.dispose();
    super.dispose();
  }

  void _syncFromDocument(ConfigDocument document) {
    final ruleSet = document.ruleSetAt(_selected);
    final signature = Object.hash(
      document.ruleSets.length,
      _selected,
      ruleSet?['tag'],
      ruleSet?['type'],
      document.draft.length,
    );
    if (signature == _loadedSignature) {
      return;
    }
    _loadedSignature = signature;
    final kind = RuleSetKind.fromWire(ruleSet?['type'] as String?);
    _tag.text = ruleSet?['tag'] as String? ?? '';
    final path = ruleSet?['path'] as String? ?? '';
    final url = ruleSet?['url'] as String? ?? '';
    _source.text = kind == RuleSetKind.local ? path : url;
    final defaultInterval = kind == RuleSetKind.remote ? 86400 : 60;
    final interval = ruleSet?['update_interval_secs'] ?? defaultInterval;
    _interval.text = '$interval';
    final timeout = ruleSet?['timeout_ms'] ?? 10000;
    _timeout.text = '$timeout';
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

    final count = document.ruleSets.length;
    if (_selected >= count) {
      _selected = count == 0 ? 0 : count - 1;
    }
    final ruleSet = document.ruleSetAt(_selected);
    final kind = RuleSetKind.fromWire(ruleSet?['type'] as String?);

    void edit(void Function(ConfigDocument document) change) {
      controller.editDocument(change);
      _loadedSignature = -1;
    }

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(32, 26, 32, 32),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          PageHeader(title: strings.ruleSetsTitle, copy: strings.ruleSetsCopy),
          const SizedBox(height: 20),
          EdgePanel(
            title: strings.ruleSetsTitle,
            actions: ActionRow(
              children: [
                OutlinedButton(
                  onPressed: () => edit((document) {
                    _selected = document.addRuleSet(RuleSetKind.remote);
                  }),
                  child: Text(strings.addRuleSet),
                ),
                TextButton(
                  onPressed: count == 0
                      ? null
                      : () => edit((document) {
                            document.removeRuleSet(_selected);
                            _selected =
                                _selected.clamp(0, document.ruleSets.length - 1);
                          }),
                  style: TextButton.styleFrom(
                    foregroundColor: Theme.of(context).colorScheme.error,
                  ),
                  child: Text(strings.remove),
                ),
              ],
            ),
            children: [
              if (count == 0)
                Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Text(
                    strings.noRuleSets,
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
                      label: strings.ruleSetTag,
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
                            value: count == 0 ? null : _selected,
                            isExpanded: true,
                            isDense: true,
                            hint: Text(
                              strings.addRuleSet,
                              style: const TextStyle(fontSize: 13),
                            ),
                            items: [
                              for (var index = 0; index < count; index++)
                                DropdownMenuItem<int>(
                                  value: index,
                                  child: Text(
                                    '${(index + 1).toString().padLeft(2, '0')}  '
                                    '${document.ruleSetAt(index)?['tag'] ?? 'rule-set'}',
                                    overflow: TextOverflow.ellipsis,
                                    style: const TextStyle(fontSize: 13),
                                  ),
                                ),
                            ],
                            onChanged: (value) {
                              if (value != null) {
                                setState(() {
                                  _selected = value;
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
                    child: EdgeDropdown<RuleSetKind>(
                      label: strings.ruleSetType,
                      value: kind,
                      items: [
                        for (final option in RuleSetKind.all)
                          (
                            value: option,
                            label: option == RuleSetKind.remote
                                ? strings.t('远程 SRS', 'Remote SRS')
                                : strings.t('本地 SRS', 'Local SRS'),
                          ),
                      ],
                      onChanged: (value) => edit(
                        (document) =>
                            document.setRuleSetType(_selected, value),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    flex: 2,
                    child: LabeledField(
                      label: strings.ruleSetTag,
                      child: EdgeTextField(
                        controller: _tag,
                        onChanged: (value) => edit(
                          (document) =>
                              document.setRuleSetField(_selected, 'tag', value),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    flex: 3,
                    child: LabeledField(
                      label: strings.ruleSetSource,
                      child: EdgeTextField(
                        controller: _source,
                        hint: kind == RuleSetKind.remote
                            ? 'https://...srs'
                            : '/path/file.srs',
                        onChanged: (value) => edit(
                          (document) => document.setRuleSetField(
                            _selected,
                            kind == RuleSetKind.remote ? 'url' : 'path',
                            value,
                          ),
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: LabeledField(
                      label: strings.refreshInterval,
                      child: EdgeTextField(
                        controller: _interval,
                        numeric: true,
                        onChanged: (value) {
                          final parsed = int.tryParse(value);
                          if (parsed != null) {
                            edit(
                              (document) => document.setRuleSetField(
                                  _selected, 'update_interval_secs', parsed),
                            );
                          }
                        },
                      ),
                    ),
                  ),
                  if (kind == RuleSetKind.remote) ...[
                    const SizedBox(width: 12),
                    Expanded(
                      child: LabeledField(
                        label: strings.downloadTimeout,
                        child: EdgeTextField(
                          controller: _timeout,
                          numeric: true,
                          onChanged: (value) {
                            final parsed = int.tryParse(value);
                            if (parsed != null) {
                              edit(
                                (document) => document.setRuleSetField(
                                    _selected, 'timeout_ms', parsed),
                              );
                            }
                          },
                        ),
                      ),
                    ),
                  ],
                ],
              ),
            ],
          ),
        ],
      ),
    );
  }
}
