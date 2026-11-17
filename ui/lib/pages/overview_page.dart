/// Runtime status: one status strip, engine controls, and the live chain.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../strings.dart';
import '../widgets.dart';

class OverviewPage extends StatelessWidget {
  const OverviewPage({
    super.key,
    required this.strings,
    required this.onNotice,
    required this.onOpenResolvers,
  });

  final Strings strings;
  final ValueChanged<String> onNotice;
  final VoidCallback onOpenResolvers;

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final scheme = Theme.of(context).colorScheme;
    final status = controller.status;
    final document = controller.document;
    final engineRunning = status?.engineRunning ?? false;
    final systemDns = status?.systemDnsEnabled ?? false;

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          EdgeGroup(
            children: [
              Row(
                children: [
                  StatusValue(
                    label: strings.engine,
                    value: status == null
                        ? strings.checking
                        : engineRunning
                            ? strings.running
                            : strings.stopped,
                    active: engineRunning,
                  ),
                  const SizedBox(width: 20),
                  StatusValue(
                    label: strings.listener,
                    value: status?.listener ?? '—',
                    monospace: true,
                    active: status?.listenerReady ?? false,
                  ),
                  const SizedBox(width: 20),
                  StatusValue(
                    label: strings.systemDns,
                    value: systemDns ? strings.managed : strings.notManaged,
                    active: systemDns,
                  ),
                  const Spacer(),
                  FilledButton(
                    onPressed: controller.busy
                        ? null
                        : () => _run(
                              context,
                              engineRunning
                                  ? AgentAction.stopEngine
                                  : AgentAction.startEngine,
                            ),
                    child: Text(
                      engineRunning ? strings.stopEngine : strings.startEngine,
                    ),
                  ),
                  const SizedBox(width: 6),
                  OutlinedButton(
                    onPressed: controller.busy
                        ? null
                        : () => _run(context, AgentAction.restartEngine),
                    child: Text(strings.restartEngine),
                  ),
                  const SizedBox(width: 2),
                  QuietAction(
                    label: strings.refresh,
                    icon: Icons.refresh,
                    onPressed: controller.busy
                        ? null
                        : () async {
                            await controller.refreshRuntime();
                            onNotice(strings.refresh);
                          },
                  ),
                ],
              ),
              if (status?.configurationError != null) ...[
                const SizedBox(height: 8),
                _Problem(message: status!.configurationError!),
              ],
            ],
          ),
          const SizedBox(height: 12),
          Expanded(
            child: EdgeGroup(
              header: strings.chain,
              trailing: QuietAction(
                label: strings.navResolvers,
                onPressed: onOpenResolvers,
              ),
              scrollable: true,
              children: [
                if (document == null)
                  Text(
                    strings.agentUnavailable,
                    style: TextStyle(
                      fontSize: 12.5,
                      color: scheme.onSurfaceVariant,
                    ),
                  )
                else if (document.layers.isEmpty)
                  Text(
                    strings.noLayers,
                    style: TextStyle(
                      fontSize: 12.5,
                      color: scheme.onSurfaceVariant,
                    ),
                  )
                else
                  for (var index = 0; index < document.layers.length; index++)
                    _ChainRow(
                      document: document,
                      index: index,
                      strings: strings,
                    ),
                if (document != null && document.validationError != null) ...[
                  const SizedBox(height: 10),
                  _Problem(message: document.validationError!),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _run(BuildContext context, AgentAction action) async {
    final controller = EdgeSteerScope.of(context);
    if (action.isDestructive) {
      final confirmed = await showConfirmDialog(context, strings, action);
      if (!confirmed) {
        return;
      }
    }
    final message = await controller.runAction(action);
    onNotice(message ?? '');
  }
}

/// One layer in the chain, rendered as a row so the order and the links between
/// layers are visible at a glance.
class _ChainRow extends StatelessWidget {
  const _ChainRow({
    required this.document,
    required this.index,
    required this.strings,
  });

  final dynamic document;
  final int index;
  final Strings strings;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final layer = document.layerAt(index);
    final tag = layer?['tag'] as String? ?? '—';
    final kind = layer?['type'] as String? ?? '?';
    final next = layer?['next'] as String?;
    final fallback = layer?['fallback'] as String?;
    final plugin = layer?['plugin'] as String?;
    final isEntry = document.entry == tag;

    return Container(
      margin: const EdgeInsets.only(bottom: 4),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest.withValues(alpha: 0.35),
        borderRadius: BorderRadius.circular(6),
        border: isEntry
            ? Border.all(color: scheme.primary.withValues(alpha: 0.6))
            : null,
      ),
      child: Row(
        children: [
          SizedBox(
            width: 26,
            child: Text(
              '${index + 1}',
              style: TextStyle(
                fontSize: 11,
                fontFamily: 'Menlo',
                color: scheme.onSurfaceVariant,
              ),
            ),
          ),
          SizedBox(
            width: 190,
            child: Text(
              tag,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 12.5,
                fontWeight: isEntry ? FontWeight.w600 : FontWeight.w400,
                color: isEntry ? scheme.primary : scheme.onSurface,
              ),
            ),
          ),
          SizedBox(
            width: 64,
            child: Text(
              kind.toUpperCase(),
              style: TextStyle(
                fontSize: 11,
                fontFamily: 'Menlo',
                color: scheme.onSurfaceVariant,
              ),
            ),
          ),
          Expanded(
            child: Text(
              [
                if (next != null && next.isNotEmpty) 'miss → $next',
                if (fallback != null && fallback.isNotEmpty) 'fail → $fallback',
                if (plugin != null && plugin.isNotEmpty) 'plugin $plugin',
              ].join('   '),
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 11.5,
                fontFamily: 'Menlo',
                color: scheme.onSurfaceVariant,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Problem extends StatelessWidget {
  const _Problem({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Icon(Icons.error_outline, size: 14, color: scheme.error),
        const SizedBox(width: 6),
        Expanded(
          child: Text(
            message,
            style: TextStyle(fontSize: 12, color: scheme.error),
          ),
        ),
      ],
    );
  }
}

/// Shared confirmation dialog. The body states the effect in one sentence; the
/// button names the action instead of saying "continue".
Future<bool> showConfirmDialog(
  BuildContext context,
  Strings strings,
  AgentAction action,
) async {
  final scheme = Theme.of(context).colorScheme;
  final result = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      backgroundColor: scheme.surfaceContainerLow,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      title: Text(
        action.label(strings),
        style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
      ),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 360),
        child: Text(
          strings.confirmFor(action),
          style: TextStyle(
            fontSize: 12.5,
            height: 1.5,
            color: scheme.onSurfaceVariant,
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(strings.cancel),
        ),
        FilledButton(
          style: action.isDestructive
              ? FilledButton.styleFrom(backgroundColor: scheme.error)
              : null,
          onPressed: () => Navigator.of(context).pop(true),
          child: Text(action.label(strings)),
        ),
      ],
    ),
  );
  return result ?? false;
}
