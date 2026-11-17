/// Cloudflare-preferred plugin editor.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';
import 'editor_scaffold.dart';

class CloudflarePage extends StatefulWidget {
  const CloudflarePage({
    super.key,
    required this.strings,
    required this.onNotice,
  });

  final Strings strings;
  final ValueChanged<String> onNotice;

  @override
  State<CloudflarePage> createState() => _CloudflarePageState();
}

class _CloudflarePageState extends State<CloudflarePage>
    with FieldBinding<CloudflarePage> {
  int _selected = 0;

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final document = controller.document;
    final strings = widget.strings;
    if (document == null) {
      return AgentUnavailable(strings: strings);
    }

    final count = document.plugins.length;
    if (_selected >= count) {
      _selected = count == 0 ? 0 : count - 1;
    }
    final plugin = document.pluginAt(_selected);
    final preferred = plugin?['preferred'];
    final optimizer = plugin?['optimizer'];
    final preferredMap = preferred is Map<String, dynamic>
        ? preferred
        : const <String, dynamic>{};
    final optimizerMap = optimizer is Map<String, dynamic>
        ? optimizer
        : const <String, dynamic>{};
    final scheduled = optimizerMap['enabled'] as bool? ?? false;

    syncFields(
      fieldSignature([
        _selected,
        count,
        plugin?['tag'],
        document.draft.length,
        scheduled,
      ]),
      () {
        textField('tag').text = plugin?['tag'] as String? ?? '';
        final ttl = plugin?['rewrite_ttl_secs'] ?? 60;
        textField('ttl').text = '$ttl';
        textField('ipv4').text = preferredMap['ipv4'] as String? ?? '';
        textField('ipv6').text = preferredMap['ipv6'] as String? ?? '';
        final interval = optimizerMap['interval_secs'] ?? 21600;
        textField('interval').text = '$interval';
        textField('host').text =
            optimizerMap['test_host'] as String? ?? 'www.cloudflare.com';
        textField('path').text =
            optimizerMap['test_path'] as String? ?? '/cdn-cgi/trace';
        final port = optimizerMap['test_port'] ?? 443;
        textField('port').text = '$port';
        final probeTimeout = optimizerMap['timeout_ms'] ?? 3000;
        textField('probeTimeout').text = '$probeTimeout';
        final concurrency = optimizerMap['concurrency'] ?? 32;
        textField('concurrency').text = '$concurrency';
        final samples = optimizerMap['samples_per_cidr'] ?? 40;
        textField('samples').text = '$samples';
        final probes = optimizerMap['probes_per_candidate'] ?? 3;
        textField('probes').text = '$probes';
        final maxCandidates = optimizerMap['max_candidates'] ?? 640;
        textField('max').text = '$maxCandidates';
        textField('candidates').text = joinList(optimizerMap['candidates']);
        textField('compatibility').text =
            joinList(optimizerMap['compatibility_hosts']);
        textField('excluded').text =
            joinList(optimizerMap['excluded_candidates']);
      },
    );

    void edit(void Function(ConfigDocument document) change) =>
        applyEdit(controller, this, change);

    Widget numberRow(
      String label,
      String key,
      String fieldName, {
      String? hint,
    }) {
      return FormRow(
        label: label,
        child: EdgeField(
          controller: textField(fieldName),
          numeric: true,
          hint: hint,
          onChanged: (value) {
            final parsed = int.tryParse(value);
            if (parsed != null) {
              edit((document) =>
                  document.setOptimizerField(_selected, key, parsed));
            }
          },
        ),
      );
    }

    Widget textRow(
      String label,
      String key,
      String fieldName, {
      String? hint,
      bool monospace = false,
    }) {
      return FormRow(
        label: label,
        child: EdgeField(
          controller: textField(fieldName),
          hint: hint,
          monospace: monospace,
          onChanged: (value) => edit((document) =>
              document.setOptimizerField(_selected, key, value)),
        ),
      );
    }

    return EditorScaffold(
      strings: strings,
      listTitle: strings.plugins,
      addLabel: strings.add,
      removeLabel: strings.remove,
      emptyLabel: strings.noPlugins,
      onAdd: () => setState(() {
        _selected = document.addPlugin();
        invalidateFields();
      }),
      onRemove: () => setState(() {
        document.removePlugin(_selected);
        invalidateFields();
        _selected = count <= 1 ? 0 : _selected - 1;
      }),
      entries: [
        for (var index = 0; index < count; index++) _entry(document, index),
      ],
      selected: _selected,
      onSelect: (index) => setState(() {
        _selected = index;
        invalidateFields();
      }),
      detail: plugin == null
          ? const SizedBox.shrink()
          : EdgeGroup(
              scrollable: true,
              children: [
                FormRow(
                  label: strings.tag,
                  child: EdgeField(
                    controller: textField('tag'),
                    onChanged: (value) => edit((document) =>
                        document.setPluginField(_selected, 'tag', value)),
                  ),
                ),
                FormRowPair(
                  left: FormRow(
                    label: strings.rewriteTtl,
                    child: EdgeField(
                      controller: textField('ttl'),
                      numeric: true,
                      onChanged: (value) {
                        final parsed = int.tryParse(value);
                        if (parsed != null) {
                          edit((document) => document.setPluginField(
                              _selected, 'rewrite_ttl_secs', parsed));
                        }
                      },
                    ),
                  ),
                  right: const SizedBox.shrink(),
                ),
                const _Rule(),
                FormRow(
                  label: strings.preferredIpv4,
                  child: EdgeField(
                    controller: textField('ipv4'),
                    monospace: true,
                    hint: '104.16.0.1',
                    onChanged: (value) => edit((document) =>
                        document.setPreferred(_selected, 'ipv4', value)),
                  ),
                ),
                FormRow(
                  label: strings.preferredIpv6,
                  child: EdgeField(
                    controller: textField('ipv6'),
                    monospace: true,
                    hint: '2606:4700::1',
                    onChanged: (value) => edit((document) =>
                        document.setPreferred(_selected, 'ipv6', value)),
                  ),
                ),
                const _Rule(),
                FormRow(
                  label: strings.scheduled,
                  child: FormSwitch(
                    value: scheduled,
                    onChanged: (value) => edit((document) =>
                        document.setOptimizerField(_selected, 'enabled', value)),
                  ),
                ),
                FormRowPair(
                  left: textRow(strings.host, 'test_host', 'host'),
                  right: textRow(strings.path, 'test_path', 'path'),
                ),
                FormRowPair(
                  left: numberRow(strings.port, 'test_port', 'port'),
                  right: numberRow(strings.timeout, 'timeout_ms',
                      'probeTimeout'),
                ),
                FormRowPair(
                  left: numberRow(strings.interval, 'interval_secs', 'interval'),
                  right: numberRow(strings.concurrency, 'concurrency',
                      'concurrency'),
                ),
                FormRowPair(
                  left: numberRow(strings.samples, 'samples_per_cidr', 'samples'),
                  right: numberRow(
                      strings.probes, 'probes_per_candidate', 'probes'),
                ),
                FormRowPair(
                  left: numberRow(
                      strings.maxCandidates, 'max_candidates', 'max'),
                  right: const SizedBox.shrink(),
                ),
                const _Rule(),
                FormRow(
                  label: strings.candidates,
                  child: EdgeField(
                    controller: textField('candidates'),
                    hint: '173.245.48.0/20, 104.16.0.0/13',
                    onChanged: (value) => edit((document) =>
                        document.setOptimizerList(
                            _selected, 'candidates', parseList(value))),
                  ),
                ),
                FormRow(
                  label: strings.compatibility,
                  child: EdgeField(
                    controller: textField('compatibility'),
                    hint: 'www.cloudflare.com',
                    onChanged: (value) => edit((document) =>
                        document.setOptimizerList(
                            _selected, 'compatibility_hosts', parseList(value))),
                  ),
                ),
                FormRow(
                  label: strings.excluded,
                  child: EdgeField(
                    controller: textField('excluded'),
                    hint: '172.64.228.0/24',
                    onChanged: (value) => edit((document) =>
                        document.setOptimizerList(
                            _selected, 'excluded_candidates', parseList(value))),
                  ),
                ),
              ],
            ),
    );
  }

  ListEntry _entry(ConfigDocument document, int index) {
    final plugin = document.pluginAt(index);
    final tag = plugin?['tag'] as String? ?? '';
    final optimizer = plugin?['optimizer'];
    final enabled = optimizer is Map<String, dynamic>
        ? optimizer['enabled'] as bool? ?? false
        : false;
    final preferred = plugin?['preferred'];
    final ipv4 = preferred is Map<String, dynamic>
        ? preferred['ipv4'] as String?
        : null;
    return ListEntry(
      title: tag.isEmpty ? '—' : tag,
      subtitle: ipv4 ?? 'cloudflare_preferred',
      badge: enabled ? 'AUTO' : null,
    );
  }
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
