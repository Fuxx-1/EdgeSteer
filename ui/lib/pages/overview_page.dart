/// Runtime status, engine controls, and the validated resolver chain.
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

    final engineLabel = status == null
        ? strings.checking
        : status.engineRunning
            ? strings.running
            : strings.stopped;
    final dnsLabel = status == null
        ? strings.checking
        : status.systemDnsEnabled
            ? strings.managed
            : strings.notManaged;

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(32, 26, 32, 32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          PageHeader(title: strings.overviewTitle, copy: strings.overviewCopy),
          const SizedBox(height: 20),
          // IntrinsicHeight keeps the three cards equal without asking for an
          // unbounded stretch inside a scroll view.
          IntrinsicHeight(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                  MetricCard(
                  label: strings.dnsEngine,
                  value: engineLabel,
                  accent:
                      status?.engineRunning ?? false ? scheme.primary : null,
                ),
                const SizedBox(width: 12),
                MetricCard(
                  label: strings.dnsListener,
                  value: status?.listener ?? strings.checking,
                ),
                const SizedBox(width: 12),
                MetricCard(
                  label: strings.systemDns,
                  value: dnsLabel,
                  accent: status?.systemDnsEnabled ?? false
                      ? scheme.primary
                      : null,
                ),
              ],
            ),
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.dnsEngine,
            subtitle: status?.configurationError ?? strings.runtimeHint,
            actions: ActionRow(
              children: [
                FilledButton(
                  onPressed: controller.busy
                      ? null
                      : () => _run(
                            context,
                            status?.engineRunning ?? false
                                ? AgentAction.stopEngine
                                : AgentAction.startEngine,
                          ),
                  child: Text(
                    status?.engineRunning ?? false
                        ? strings.stopEngine
                        : strings.startEngine,
                  ),
                ),
                OutlinedButton(
                  onPressed: controller.busy
                      ? null
                      : () => _run(context, AgentAction.restartEngine),
                  child: Text(strings.restartEngine),
                ),
                TextButton(
                  onPressed: controller.busy
                      ? null
                      : () async {
                          await controller.refreshRuntime();
                          onNotice(strings.refresh);
                        },
                  child: Text(strings.refresh),
                ),
              ],
            ),
            children: const [],
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.resolverChain,
            subtitle: document == null
                ? strings.checking
                : document.validationError,
            actions: TextButton(
              onPressed: onOpenResolvers,
              child: Text(strings.navResolvers),
            ),
            children: [
              if (document != null)
                Text(
                  document.resolverSummary(),
                  style: TextStyle(
                    fontSize: 12.5,
                    height: 1.6,
                    fontFamily: 'Menlo',
                    color: scheme.onSurfaceVariant,
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }

  Future<void> _run(BuildContext context, AgentAction action) async {
    final controller = EdgeSteerScope.of(context);
    final confirmed = action.isDestructive
        ? await showConfirmDialog(context, strings, action)
        : true;
    if (!confirmed) {
      return;
    }
    final message = await controller.runAction(action);
    onNotice(message ?? '');
  }
}

/// Shared confirmation dialog for destructive or privileged operations.
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
      title: Text('${strings.confirmTitle}: ${action.label(strings)}'),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: Text(
          action.confirmation(strings),
          style: TextStyle(fontSize: 13, height: 1.5, color: scheme.onSurfaceVariant),
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
          child: Text(
            action == AgentAction.quit
                ? action.label(strings)
                : strings.continueLabel,
          ),
        ),
      ],
    ),
  );
  return result ?? false;
}
