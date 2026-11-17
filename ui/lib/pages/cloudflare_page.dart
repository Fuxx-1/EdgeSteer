/// Editor for the Cloudflare-preferred plugin: rewrite settings, fixed
/// preferred addresses, and the stable probing optimizer.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../config_document.dart';
import '../strings.dart';
import '../widgets.dart';

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

class _CloudflarePageState extends State<CloudflarePage> {
  int _selected = 0;
  int _loadedSignature = -1;

  final _tag = TextEditingController();
  final _ttl = TextEditingController();
  final _ipv4 = TextEditingController();
  final _ipv6 = TextEditingController();
  final _interval = TextEditingController();
  final _host = TextEditingController();
  final _path = TextEditingController();
  final _port = TextEditingController();
  final _timeout = TextEditingController();
  final _concurrency = TextEditingController();
  final _samples = TextEditingController();
  final _probes = TextEditingController();
  final _max = TextEditingController();
  final _candidates = TextEditingController();
  final _compatibility = TextEditingController();
  final _excluded = TextEditingController();

  @override
  void dispose() {
    for (final controller in [
      _tag,
      _ttl,
      _ipv4,
      _ipv6,
      _interval,
      _host,
      _path,
      _port,
      _timeout,
      _concurrency,
      _samples,
      _probes,
      _max,
      _candidates,
      _compatibility,
      _excluded,
    ]) {
      controller.dispose();
    }
    super.dispose();
  }

  void _syncFromDocument(ConfigDocument document) {
    final plugin = document.pluginAt(_selected);
    final signature = Object.hash(
      document.plugins.length,
      _selected,
      plugin?['tag'],
      document.draft.length,
    );
    if (signature == _loadedSignature) {
      return;
    }
    _loadedSignature = signature;

    final preferred = plugin?['preferred'];
    final optimizer = plugin?['optimizer'];
    final preferredMap =
        preferred is Map<String, dynamic> ? preferred : const <String, dynamic>{};
    final optimizerMap =
        optimizer is Map<String, dynamic> ? optimizer : const <String, dynamic>{};

    _tag.text = plugin?['tag'] as String? ?? '';
    _ttl.text = '${plugin?['rewrite_ttl_secs'] ?? 60}';
    _ipv4.text = preferredMap['ipv4'] as String? ?? '';
    _ipv6.text = preferredMap['ipv6'] as String? ?? '';
    _interval.text = '${optimizerMap['interval_secs'] ?? 21600}';
    _host.text = optimizerMap['test_host'] as String? ?? 'www.cloudflare.com';
    _path.text = optimizerMap['test_path'] as String? ?? '/cdn-cgi/trace';
    _port.text = '${optimizerMap['test_port'] ?? 443}';
    _timeout.text = '${optimizerMap['timeout_ms'] ?? 3000}';
    _concurrency.text = '${optimizerMap['concurrency'] ?? 32}';
    _samples.text = '${optimizerMap['samples_per_cidr'] ?? 40}';
    _probes.text = '${optimizerMap['probes_per_candidate'] ?? 3}';
    _max.text = '${optimizerMap['max_candidates'] ?? 640}';
    _candidates.text = joinList(optimizerMap['candidates']);
    _compatibility.text = joinList(optimizerMap['compatibility_hosts']);
    _excluded.text = joinList(optimizerMap['excluded_candidates']);
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

    final count = document.plugins.length;
    if (_selected >= count) {
      _selected = count == 0 ? 0 : count - 1;
    }
    final plugin = document.pluginAt(_selected);
    final optimizer = plugin?['optimizer'];
    final optimizerEnabled = optimizer is Map<String, dynamic>
        ? optimizer['enabled'] as bool? ?? false
        : false;

    void edit(void Function(ConfigDocument document) change) {
      controller.editDocument(change);
      _loadedSignature = -1;
    }

    Widget numberField(
      String label,
      TextEditingController textController,
      String key, {
      String? hint,
    }) {
      return LabeledField(
        label: label,
        child: EdgeTextField(
          controller: textController,
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

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(32, 26, 32, 32),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          PageHeader(
            title: strings.cloudflareTitle,
            copy: strings.cloudflareCopy,
          ),
          const SizedBox(height: 20),
          EdgePanel(
            title: strings.cloudflareTitle,
            actions: ActionRow(
              children: [
                OutlinedButton(
                  onPressed: () => edit((document) {
                    _selected = document.addPlugin();
                  }),
                  child: Text(strings.addPlugin),
                ),
                TextButton(
                  onPressed: count == 0
                      ? null
                      : () => edit((document) {
                            document.removePlugin(_selected);
                            _selected =
                                _selected.clamp(0, document.plugins.length - 1);
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
                    strings.noPlugins,
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
                    flex: 2,
                    child: LabeledField(
                      label: strings.pluginTag,
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
                              strings.addPlugin,
                              style: const TextStyle(fontSize: 13),
                            ),
                            items: [
                              for (var index = 0; index < count; index++)
                                DropdownMenuItem<int>(
                                  value: index,
                                  child: Text(
                                    '${(index + 1).toString().padLeft(2, '0')}  '
                                    '${document.pluginAt(index)?['tag'] ?? 'plugin'}',
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
                    child: LabeledField(
                      label: strings.rewriteTtl,
                      child: EdgeTextField(
                        controller: _ttl,
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
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: LabeledField(
                      label: strings.preferredIpv4,
                      child: EdgeTextField(
                        controller: _ipv4,
                        hint: '104.16.0.1',
                        onChanged: (value) => edit((document) =>
                            document.setPreferred(_selected, 'ipv4', value)),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: LabeledField(
                      label: strings.preferredIpv6,
                      child: EdgeTextField(
                        controller: _ipv6,
                        hint: '2606:4700::1',
                        onChanged: (value) => edit((document) =>
                            document.setPreferred(_selected, 'ipv6', value)),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 14),
              Row(
                children: [
                  Checkbox(
                    value: optimizerEnabled,
                    onChanged: (value) => edit((document) =>
                        document.setOptimizerField(
                            _selected, 'enabled', value ?? false)),
                  ),
                  Text(
                    strings.enableOptimizer,
                    style: const TextStyle(fontSize: 13),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: numberField(
                      strings.probeInterval,
                      _interval,
                      'interval_secs',
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    flex: 2,
                    child: LabeledField(
                      label: strings.probeHost,
                      child: EdgeTextField(
                        controller: _host,
                        hint: 'www.cloudflare.com',
                        onChanged: (value) => edit((document) =>
                            document.setOptimizerField(
                                _selected, 'test_host', value)),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    flex: 2,
                    child: LabeledField(
                      label: strings.probePath,
                      child: EdgeTextField(
                        controller: _path,
                        hint: '/cdn-cgi/trace',
                        onChanged: (value) => edit((document) =>
                            document.setOptimizerField(
                                _selected, 'test_path', value)),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: numberField(strings.probePort, _port, 'test_port'),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: numberField(
                      strings.probeTimeout,
                      _timeout,
                      'timeout_ms',
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: numberField(
                      strings.concurrency,
                      _concurrency,
                      'concurrency',
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: numberField(
                      strings.samplesPerCidr,
                      _samples,
                      'samples_per_cidr',
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: numberField(
                      strings.probesPerCandidate,
                      _probes,
                      'probes_per_candidate',
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: numberField(
                      strings.maxCandidates,
                      _max,
                      'max_candidates',
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              LabeledField(
                label: strings.candidateList,
                child: EdgeTextField(
                  controller: _candidates,
                  hint: '173.245.48.0/20, 104.16.0.0/13',
                  maxLines: 3,
                  monospace: true,
                  onChanged: (value) => edit((document) =>
                      document.setOptimizerList(
                          _selected, 'candidates', parseList(value))),
                ),
              ),
              const SizedBox(height: 12),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: LabeledField(
                      label: strings.compatibilityHosts,
                      child: EdgeTextField(
                        controller: _compatibility,
                        hint: 'www.cloudflare.com',
                        maxLines: 2,
                        onChanged: (value) => edit((document) =>
                            document.setOptimizerList(_selected,
                                'compatibility_hosts', parseList(value))),
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: LabeledField(
                      label: strings.excludedCandidates,
                      child: EdgeTextField(
                        controller: _excluded,
                        hint: '172.64.228.0/24',
                        maxLines: 2,
                        onChanged: (value) => edit((document) =>
                            document.setOptimizerList(_selected,
                                'excluded_candidates', parseList(value))),
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
}
