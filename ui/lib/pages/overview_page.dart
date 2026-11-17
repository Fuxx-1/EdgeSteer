/// Runtime status plus the machine-level controls that act on it.
///
/// One screen: a status strip with the engine actions, then the preferences,
/// login item, system DNS, network services, and quit controls. The resolver
/// chain lives on its own page now, so this page does not duplicate it.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../strings.dart';
import '../widgets.dart';
import 'system_page.dart';

class OverviewPage extends StatelessWidget {
  const OverviewPage({
    super.key,
    required this.strings,
    required this.onNotice,
    required this.onQuit,
  });

  final Strings strings;
  final ValueChanged<String> onNotice;
  final VoidCallback onQuit;

  @override
  Widget build(BuildContext context) {
    final controller = EdgeSteerScope.of(context);
    final status = controller.status;
    final engineRunning = status?.engineRunning ?? false;
    final systemDns = status?.systemDnsEnabled ?? false;

    return SingleChildScrollView(
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
                  const SizedBox(width: 20),
                  StatusValue(
                    label: strings.autostart,
                    value: controller.autostartEnabled
                        ? strings.autostartOn
                        : strings.autostartOff,
                    active: controller.autostartEnabled,
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
          SystemSection(
            strings: strings,
            onNotice: onNotice,
            onQuit: onQuit,
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
