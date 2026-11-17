/// Rule-set editor.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';
import 'editor_scaffold.dart';

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

class _RulesPageState extends State<RulesPage> with FieldBinding<RulesPage> {
  int _selected = 0;

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final document = controller.document;
    final strings = widget.strings;
    if (document == null) {
      return AgentUnavailable(strings: strings);
    }

    final count = document.ruleSets.length;
    if (_selected >= count) {
      _selected = count == 0 ? 0 : count - 1;
    }
    final ruleSet = document.ruleSetAt(_selected);
    final kind = RuleSetKind.fromWire(ruleSet?['type'] as String?);

    syncFields(
      fieldSignature([
        _selected,
        count,
        ruleSet?['tag'],
        ruleSet?['type'],
        document.draft.length,
      ]),
      () {
        textField('tag').text = ruleSet?['tag'] as String? ?? '';
        final localPath = ruleSet?['path'] as String?;
        final remoteUrl = ruleSet?['url'] as String?;
        final source = kind == RuleSetKind.local ? localPath : remoteUrl;
        textField('source').text = source ?? '';
        final defaultInterval = kind == RuleSetKind.remote ? 86400 : 60;
        final interval = ruleSet?['update_interval_secs'] ?? defaultInterval;
        final timeout = ruleSet?['timeout_ms'] ?? 10000;
        textField('interval').text = '$interval';
        textField('timeout').text = '$timeout';
      },
    );

    void edit(void Function(ConfigDocument document) change) =>
        applyEdit(controller, this, change);

    return EditorScaffold(
      strings: strings,
      listTitle: strings.ruleSets,
      addLabel: strings.add,
      removeLabel: strings.remove,
      emptyLabel: strings.noRuleSets,
      onAdd: () => setState(() {
        _selected = document.addRuleSet(RuleSetKind.remote);
        invalidateFields();
      }),
      onRemove: () => setState(() {
        document.removeRuleSet(_selected);
        invalidateFields();
        _selected = count <= 1 ? 0 : _selected - 1;
      }),
      entries: [
        for (var index = 0; index < count; index++)
          _entry(document, index, strings),
      ],
      selected: _selected,
      onSelect: (index) => setState(() {
        _selected = index;
        invalidateFields();
      }),
      detail: ruleSet == null
          ? const SizedBox.shrink()
          : EdgeGroup(
              scrollable: true,
              children: [
                FormRow(
                  label: strings.tag,
                  child: EdgeField(
                    controller: textField('tag'),
                    onChanged: (value) => edit((document) =>
                        document.setRuleSetField(_selected, 'tag', value)),
                  ),
                ),
                FormRow(
                  label: strings.type,
                  child: EdgeSelect<RuleSetKind>(
                    value: kind,
                    items: [
                      for (final option in RuleSetKind.all)
                        (
                          value: option,
                          label: option == RuleSetKind.remote
                              ? strings.remote
                              : strings.local,
                        ),
                    ],
                    onChanged: (value) => edit((document) =>
                        document.setRuleSetType(_selected, value)),
                  ),
                ),
                FormRow(
                  label: strings.source,
                  child: EdgeField(
                    controller: textField('source'),
                    monospace: true,
                    hint: kind == RuleSetKind.remote
                        ? 'https://…/rules.srs'
                        : '/path/rules.srs',
                    onChanged: (value) => edit((document) =>
                        document.setRuleSetField(
                          _selected,
                          kind == RuleSetKind.remote ? 'url' : 'path',
                          value,
                        )),
                  ),
                ),
                FormRowPair(
                  left: FormRow(
                    label: strings.interval,
                    child: EdgeField(
                      controller: textField('interval'),
                      numeric: true,
                      onChanged: (value) {
                        final parsed = int.tryParse(value);
                        if (parsed != null) {
                          edit((document) => document.setRuleSetField(
                              _selected, 'update_interval_secs', parsed));
                        }
                      },
                    ),
                  ),
                  right: kind == RuleSetKind.remote
                      ? FormRow(
                          label: strings.downloadTimeout,
                          child: EdgeField(
                            controller: textField('timeout'),
                            numeric: true,
                            onChanged: (value) {
                              final parsed = int.tryParse(value);
                              if (parsed != null) {
                                edit((document) => document.setRuleSetField(
                                    _selected, 'timeout_ms', parsed));
                              }
                            },
                          ),
                        )
                      : const SizedBox.shrink(),
                ),
              ],
            ),
    );
  }

  ListEntry _entry(ConfigDocument document, int index, Strings strings) {
    final ruleSet = document.ruleSetAt(index);
    final tag = ruleSet?['tag'] as String? ?? '';
    final kind = RuleSetKind.fromWire(ruleSet?['type'] as String?);
    final localPath = ruleSet?['path'] as String? ?? '';
    final remoteUrl = ruleSet?['url'] as String? ?? '';
    final source = kind == RuleSetKind.local ? localPath : remoteUrl;
    return ListEntry(
      title: tag.isEmpty ? '—' : tag,
      subtitle: source.isEmpty ? strings.source : source,
      badge: kind == RuleSetKind.remote ? 'SRS' : 'FILE',
    );
  }
}
