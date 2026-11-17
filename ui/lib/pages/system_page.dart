/// Application preferences, login start, system DNS, and network services.
library;

import 'package:flutter/material.dart';

import '../app_state.dart';
import '../agent_client.dart';
import '../strings.dart';
import '../widgets.dart';
import 'overview_page.dart';

class SystemPage extends StatelessWidget {
  const SystemPage({
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
    final scheme = Theme.of(context).colorScheme;

    final systemDnsDetail = _systemDnsDetail(controller, strings);
    final systemDnsEnabled = controller.systemDnsEnabled;
    final canEnableSystemDns = controller.configurationValid &&
        controller.engineRunning &&
        controller.listenerSupportsSystemDns &&
        (status?.listenerReady ?? false);

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(32, 26, 32, 32),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          PageHeader(title: strings.systemTitle, copy: strings.systemCopy),
          const SizedBox(height: 20),
          EdgePanel(
            title: strings.application,
            subtitle: strings.applicationCopy,
            children: [
              Row(
                children: [
                  SizedBox(
                    width: 190,
                    child: EdgeDropdown<String>(
                      label: strings.language,
                      value: controller.preferences.isChinese
                          ? 'chinese'
                          : 'english',
                      items: const [
                        (value: 'chinese', label: '简体中文'),
                        (value: 'english', label: 'English'),
                      ],
                      onChanged: (value) async {
                        final error = await controller
                            .setLanguage(value == 'chinese');
                        if (error != null) {
                          onNotice(error);
                        }
                      },
                    ),
                  ),
                  const SizedBox(width: 16),
                  SizedBox(
                    width: 150,
                    child: EdgeDropdown<String>(
                      label: strings.appearance,
                      value: controller.preferences.isDark ? 'dark' : 'light',
                      items: [
                        (value: 'dark', label: strings.dark),
                        (value: 'light', label: strings.light),
                      ],
                      onChanged: (value) async {
                        final error =
                            await controller.setAppearance(value == 'dark');
                        if (error != null) {
                          onNotice(error);
                        }
                      },
                    ),
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.openAtLogin,
            subtitle: status?.autostartSupported ?? false
                ? (controller.autostartEnabled
                    ? strings.autostartEnabled
                    : strings.autostartDisabled)
                : strings.autostartUnsupported,
            actions: OutlinedButton(
              onPressed: controller.busy ||
                      (!controller.autostartEnabled &&
                          !(status?.autostartSupported ?? false))
                  ? null
                  : () => _run(
                        context,
                        controller.autostartEnabled
                            ? AgentAction.disableAutoStart
                            : AgentAction.enableAutoStart,
                      ),
              child: Text(
                controller.autostartEnabled
                    ? strings.disableAutoStart
                    : strings.enableAutoStart,
              ),
            ),
            children: const [],
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.systemDns,
            subtitle: systemDnsDetail,
            actions: OutlinedButton(
              onPressed: controller.busy ||
                      (!systemDnsEnabled && !canEnableSystemDns)
                  ? null
                  : () => _run(
                        context,
                        systemDnsEnabled
                            ? AgentAction.disableSystemDns
                            : AgentAction.enableSystemDns,
                      ),
              child: Text(
                systemDnsEnabled
                    ? strings.disableSystemDns
                    : strings.enableSystemDns,
              ),
            ),
            children: const [],
          ),
          if (status?.legacyServiceDetected ?? false) ...[
            const SizedBox(height: 16),
            EdgePanel(
              title: strings.legacyService,
              subtitle: strings.legacyServiceCopy,
              actions: OutlinedButton(
                onPressed: controller.busy
                    ? null
                    : () => _run(context, AgentAction.removeLegacyService),
                child: Text(strings.removeLegacyService),
              ),
              children: const [],
            ),
          ],
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.networkServices,
            children: [
              if ((status?.dnsServices ?? const <DnsServiceStatus>[]).isEmpty)
                Text(
                  strings.noNetworkServices,
                  style: TextStyle(
                    fontSize: 12.5,
                    color: scheme.onSurfaceVariant,
                  ),
                )
              else
                for (final service in status!.dnsServices)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Row(
                      children: [
                        SizedBox(
                          width: 220,
                          child: Text(
                            '${service.name} (${service.device})',
                            style: const TextStyle(fontSize: 12.5),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        SizedBox(
                          width: 80,
                          child: Text(
                            service.enabled
                                ? strings.t('已启用', 'Enabled')
                                : strings.t('已停用', 'Disabled'),
                            style: TextStyle(
                              fontSize: 12,
                              color: scheme.onSurfaceVariant,
                            ),
                          ),
                        ),
                        Expanded(
                          child: Text(
                            service.dnsDescription,
                            style: TextStyle(
                              fontSize: 12,
                              fontFamily: 'Menlo',
                              color: service.usesLoopbackDns
                                  ? scheme.primary
                                  : scheme.onSurfaceVariant,
                            ),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ],
                    ),
                  ),
            ],
          ),
          const SizedBox(height: 16),
          EdgePanel(
            title: strings.quitApplication,
            children: [
              Align(
                alignment: Alignment.centerLeft,
                child: OutlinedButton(
                  style: OutlinedButton.styleFrom(
                    foregroundColor: scheme.error,
                    side: BorderSide(color: scheme.error.withValues(alpha: 0.5)),
                  ),
                  onPressed: controller.busy
                      ? null
                      : () async {
                          final confirmed = await showConfirmDialog(
                            context,
                            strings,
                            AgentAction.quit,
                          );
                          if (!confirmed) {
                            return;
                          }
                          final message =
                              await controller.runAction(AgentAction.quit);
                          onNotice(message ?? '');
                          onQuit();
                        },
                  child: Text(strings.quitApplication),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  String _systemDnsDetail(EdgeSteerController controller, Strings strings) {
    if (controller.systemDnsEnabled) {
      return strings.systemDnsManaged;
    }
    if (!controller.configurationValid) {
      return strings.fixConfigFirst;
    }
    if (!controller.listenerSupportsSystemDns) {
      return strings.systemDnsNeedsPort53;
    }
    if (!controller.engineRunning) {
      return strings.systemDnsNeedsEngine;
    }
    if (!(controller.status?.listenerReady ?? false)) {
      return strings.systemDnsNeedsListener;
    }
    return strings.systemDnsReady;
  }

  Future<void> _run(BuildContext context, AgentAction action) async {
    final controller = EdgeSteerScope.of(context);
    final confirmed = await showConfirmDialog(context, strings, action);
    if (!confirmed) {
      return;
    }
    final message = await controller.runAction(action);
    onNotice(message ?? '');
  }
}
