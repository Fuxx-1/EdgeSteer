/// Application state shared by every page: the configuration document, the
/// agent status, preferences, and the busy/notice handling.
library;

import 'dart:async';

import 'package:flutter/material.dart';

import 'agent_client.dart';
import 'config_document.dart';
import 'strings.dart';

class EdgeSteerController extends ChangeNotifier {
  EdgeSteerController({AgentClient? client})
      : client = client ?? AgentClient();

  final AgentClient client;

  AgentStatus? status;
  ConfigDocument? document;
  UiPreferences preferences = const UiPreferences();
  bool busy = false;
  bool loading = true;
  String? startupNotice;

  Strings get strings => preferences.isChinese ? Strings.zh : Strings.en;

  bool get engineRunning => status?.engineRunning ?? false;
  bool get systemDnsEnabled => status?.systemDnsEnabled ?? false;
  bool get autostartEnabled => status?.autostartEnabled ?? false;
  bool get configurationValid => document?.isValid ?? false;

  bool get listenerSupportsSystemDns =>
      status?.listenerSupportsSystemDns ?? false;

  /// Loads preferences first so notices are already in the user's language,
  /// then the configuration document, then the runtime status.
  Future<void> initialise() async {
    loading = true;
    notifyListeners();

    try {
      preferences = await client.readPreferences();
    } on AgentException catch (error) {
      startupNotice = error.message;
    }

    try {
      final snapshot = await client.readConfig();
      final loaded = ConfigDocument.fromSnapshot(snapshot);
      document = loaded;
      if (!snapshot.exists) {
        startupNotice ??= strings.t(
          '配置文件不存在，已载入内置默认配置草稿。',
          'The configuration file does not exist; the bundled default is '
              'loaded as a draft.',
        );
      }
      await _revalidate();
    } on AgentException catch (error) {
      startupNotice = error.message;
      document = null;
    }

    try {
      status = await client.status();
    } on AgentException catch (error) {
      status = AgentStatus.unavailable(error.message);
    }

    // Clear the loading gate before the first paint that shows real data, then
    // refresh once more so the window never keeps a status read that happened
    // while the pages were still hidden behind the spinner.
    loading = false;
    notifyListeners();
    await refreshRuntime();
  }

  Future<void> refreshRuntime() async {
    try {
      status = await client.status();
    } on AgentException catch (error) {
      status = AgentStatus.unavailable(error.message);
    }
    notifyListeners();
  }

  /// Asks the agent to validate the current draft. The agent owns validation so
  /// the interface can never accept a document the DNS service would reject.
  Future<void> _revalidate() async {
    final current = document;
    if (current == null) {
      return;
    }
    current.validating = true;
    notifyListeners();
    try {
      current.validationError = await client.validateConfig(current.draft);
    } on AgentException catch (error) {
      current.validationError = error.message;
    }
    current.validating = false;
    notifyListeners();
  }

  void updateDraft(String contents) {
    final current = document;
    if (current == null) {
      return;
    }
    current.replaceDraft(contents);
    notifyListeners();
    unawaited(_revalidate());
  }

  /// Applies a structured edit and re-serializes the document.
  void editDocument(void Function(ConfigDocument document) edit) {
    final current = document;
    if (current == null) {
      return;
    }
    edit(current);
    notifyListeners();
    unawaited(_revalidate());
  }

  Future<String?> saveDocument() async {
    final current = document;
    if (current == null) {
      return strings.t('没有可保存的配置。', 'There is no configuration to save.');
    }
    try {
      final response = await client.saveConfig(current.draft);
      if (!response.ok) {
        return response.message;
      }
      current.saved = current.draft;
      current.exists = true;
      status = response.status;
      notifyListeners();
      return null;
    } on AgentException catch (error) {
      return error.message;
    }
  }

  Future<String?> reloadDocument() async {
    try {
      final snapshot = await client.readConfig();
      document = ConfigDocument.fromSnapshot(snapshot);
      await _revalidate();
      await refreshRuntime();
      return null;
    } on AgentException catch (error) {
      return error.message;
    }
  }

  Future<String?> validateNow() async {
    await _revalidate();
    final error = document?.validationError;
    return error;
  }

  Future<String?> setLanguage(bool chinese) async {
    final next = preferences.copyWith(language: chinese ? 'chinese' : 'english');
    return _savePreferences(next);
  }

  Future<String?> setAppearance(AppearanceMode mode) async {
    final next = preferences.copyWith(appearance: mode.wire);
    return _savePreferences(next);
  }

  Future<String?> _savePreferences(UiPreferences next) async {
    preferences = next;
    notifyListeners();
    try {
      await client.savePreferences(next);
      return null;
    } on AgentException catch (error) {
      return error.message;
    }
  }

  /// Runs a runtime command through the agent. The engine, system DNS, and the
  /// login item are only ever changed by the agent, so this is the single path
  /// for those operations.
  Future<String?> runAction(AgentAction action) async {
    if (busy) {
      return strings.operationInProgress;
    }
    if (action.requiresValidConfiguration && !configurationValid) {
      return strings.invalid;
    }
    busy = true;
    notifyListeners();
    try {
      final command = switch (action) {
        AgentAction.startEngine => AgentCommand.startEngine,
        AgentAction.stopEngine => AgentCommand.stopEngine,
        AgentAction.restartEngine => AgentCommand.restartEngine,
        AgentAction.enableSystemDns => AgentCommand.enableSystemDns,
        AgentAction.disableSystemDns => AgentCommand.disableSystemDns,
        AgentAction.enableAutoStart => AgentCommand.enableAutoStart,
        AgentAction.disableAutoStart => AgentCommand.disableAutoStart,
        AgentAction.removeLegacyService => AgentCommand.removeLegacyService,
        AgentAction.quit => AgentCommand.quit,
      };
      final response = await client.request(command);
      status = response.status;
      return response.message;
    } on AgentException catch (error) {
      return error.message;
    } finally {
      busy = false;
      notifyListeners();
    }
  }
}

/// Provides the controller to the widget tree.
class EdgeSteerScope extends InheritedNotifier<EdgeSteerController> {
  const EdgeSteerScope({
    super.key,
    required EdgeSteerController controller,
    required super.child,
  }) : super(notifier: controller);

  static EdgeSteerController of(BuildContext context) {
    final scope =
        context.dependOnInheritedWidgetOfExactType<EdgeSteerScope>();
    assert(scope != null, 'EdgeSteerScope is missing from the widget tree');
    return scope!.notifier!;
  }
}
