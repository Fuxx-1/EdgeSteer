/// System page: appearance, login item, system DNS, network services, quit.
///
/// Each capability is one row: label, current state, single action. State text
/// explains itself, so there is no descriptive paragraph per section.
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
    final scheme = Theme.of(context).colorScheme;
    final status = controller.status;

    final systemDnsState = _systemDnsState(controller, strings);
    final canEnableSystemDns = systemDnsState == _SystemDnsState.ready;
    final autostartSupported = status?.autostartSupported ?? false;

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          EdgeGroup(
            header: strings.application,
            children: [
              FormRow(
                label: strings.language,
                labelWidth: 96,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: SizedBox(
                    width: 160,
                    child: EdgeSelect<String>(
                      value:
                          controller.preferences.isChinese ? 'chinese' : 'english',
                      items: const [
                        (value: 'chinese', label: '简体中文'),
                        (value: 'english', label: 'English'),
                      ],
                      onChanged: (value) async {
                        final error =
                            await controller.setLanguage(value == 'chinese');
                        if (error != null) {
                          onNotice(error);
                        }
                      },
                    ),
                  ),
                ),
              ),
              FormRow(
                label: strings.appearance,
                labelWidth: 96,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: SizedBox(
                    width: 160,
                    child: EdgeSelect<String>(
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
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          EdgeGroup(
            header: strings.openAtLogin,
            children: [
              _CapabilityRow(
                state: controller.autostartEnabled
                    ? strings.autostartOn
                    : autostartSupported
                        ? strings.autostartOff
                        : strings.autostartNeedsBundle,
                active: controller.autostartEnabled,
                warning: !controller.autostartEnabled && !autostartSupported,
                action: controller.autostartEnabled
                    ? strings.disableAutoStart
                    : strings.enableAutoStart,
                onAction: controller.busy ||
                        (!controller.autostartEnabled && !autostartSupported)
                    ? null
                    : () => _run(
                          context,
                          controller.autostartEnabled
                              ? AgentAction.disableAutoStart
                              : AgentAction.enableAutoStart,
                        ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          EdgeGroup(
            header: strings.systemDns,
            children: [
              _CapabilityRow(
                state: _systemDnsLabel(systemDnsState, strings),
                active: controller.systemDnsEnabled,
                warning: systemDnsState != _SystemDnsState.ready &&
                    systemDnsState != _SystemDnsState.managed,
                action: controller.systemDnsEnabled
                    ? strings.disableSystemDns
                    : strings.enableSystemDns,
                onAction: controller.busy ||
                        (!controller.systemDnsEnabled && !canEnableSystemDns)
                    ? null
                    : () => _run(
                          context,
                          controller.systemDnsEnabled
                              ? AgentAction.disableSystemDns
                              : AgentAction.enableSystemDns,
                        ),
              ),
            ],
          ),
          if (status?.legacyServiceDetected ?? false) ...[
            const SizedBox(height: 12),
            EdgeGroup(
              header: strings.legacyService,
              children: [
                _CapabilityRow(
                  state: 'root daemon',
                  active: false,
                  warning: true,
                  action: strings.removeLegacyService,
                  onAction: controller.busy
                      ? null
                      : () => _run(context, AgentAction.removeLegacyService),
                ),
              ],
            ),
          ],
          const SizedBox(height: 12),
          EdgeGroup(
            header: strings.networkServices,
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
                          width: 210,
                          child: Text(
                            service.name,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontSize: 12.5),
                          ),
                        ),
                        SizedBox(
                          width: 52,
                          child: Text(
                            service.device,
                            style: TextStyle(
                              fontSize: 11.5,
                              fontFamily: 'Menlo',
                              color: scheme.onSurfaceVariant,
                            ),
                          ),
                        ),
                        Expanded(
                          child: Text(
                            service.dnsDescription,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 11.5,
                              fontFamily: 'Menlo',
                              color: service.usesLoopbackDns
                                  ? scheme.primary
                                  : scheme.onSurfaceVariant,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
            ],
          ),
          const SizedBox(height: 12),
          EdgeGroup(
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      strings.quit,
                      style: const TextStyle(fontSize: 12.5),
                    ),
                  ),
                  OutlinedButton(
                    style: OutlinedButton.styleFrom(
                      foregroundColor: scheme.error,
                      side: BorderSide(
                        color: scheme.error.withValues(alpha: 0.5),
                      ),
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
                    child: Text(strings.quit),
                  ),
                ],
              ),
            ],
          ),
        ],
      ),
    );
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

  _SystemDnsState _systemDnsState(
    EdgeSteerController controller,
    Strings strings,
  ) {
    if (controller.systemDnsEnabled) {
      return _SystemDnsState.managed;
    }
    if (!controller.configurationValid) {
      return _SystemDnsState.invalidConfig;
    }
    if (!controller.listenerSupportsSystemDns) {
      return _SystemDnsState.needsPort53;
    }
    if (!controller.engineRunning) {
      return _SystemDnsState.needsEngine;
    }
    if (!(controller.status?.listenerReady ?? false)) {
      return _SystemDnsState.needsListener;
    }
    return _SystemDnsState.ready;
  }

  String _systemDnsLabel(_SystemDnsState state, Strings strings) =>
      switch (state) {
        _SystemDnsState.managed => strings.managed,
        _SystemDnsState.ready => strings.ready,
        _SystemDnsState.needsPort53 => strings.needsPort53,
        _SystemDnsState.needsEngine => strings.needsEngine,
        _SystemDnsState.needsListener => strings.needsListener,
        _SystemDnsState.invalidConfig => strings.invalid,
      };
}

enum _SystemDnsState {
  managed,
  ready,
  needsPort53,
  needsEngine,
  needsListener,
  invalidConfig,
}

/// One capability: state on the left, the single available action on the right.
class _CapabilityRow extends StatelessWidget {
  const _CapabilityRow({
    required this.state,
    required this.active,
    required this.action,
    required this.onAction,
    this.warning = false,
  });

  final String state;
  final bool active;
  final bool warning;
  final String action;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Row(
      children: [
        Container(
          width: 7,
          height: 7,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: active
                ? scheme.primary
                : warning
                    ? scheme.error
                    : scheme.outline,
          ),
        ),
        const SizedBox(width: 8),
        Expanded(
          child: Text(
            state,
            style: TextStyle(
              fontSize: 12.5,
              color: warning && !active ? scheme.error : scheme.onSurface,
            ),
          ),
        ),
        OutlinedButton(
          onPressed: onAction,
          child: Text(action),
        ),
      ],
    );
  }
}
