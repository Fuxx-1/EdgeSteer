/// The loopback control protocol between the Flutter interface and the
/// resident EdgeSteer agent.
///
/// The agent owns the DNS engine, the system DNS registration, and the
/// configuration file. This client only sends one authenticated JSON request
/// per connection and decodes the single JSON reply, so the interface and the
/// DNS service always validate configuration through the exact same code.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart' show Brightness;

/// Protocol revision. It must match `CONTROL_PROTOCOL` in `src/control.rs`.
const int controlProtocol = 2;

/// Wire name of the control record published by the agent.
const String controlRecordName = 'agent.json';

class AgentException implements Exception {
  AgentException(this.message);

  final String message;

  @override
  String toString() => message;
}

/// The record the agent publishes in its state directory.
class AgentRecord {
  const AgentRecord({
    required this.protocol,
    required this.pid,
    required this.address,
    required this.port,
    required this.token,
  });

  factory AgentRecord.fromJson(Map<String, dynamic> json) {
    final rawAddress = json['address'] as String;
    final separator = rawAddress.lastIndexOf(':');
    final host = rawAddress
        .substring(0, separator)
        .replaceAll(RegExp(r'^\[|\]$'), '');
    return AgentRecord(
      protocol: json['protocol'] as int,
      pid: json['pid'] as int,
      address: InternetAddress(host),
      port: int.parse(rawAddress.substring(separator + 1)),
      token: json['token'] as String,
    );
  }

  final int protocol;
  final int pid;
  final InternetAddress address;
  final int port;
  final String token;
}

/// Commands understood by the agent. The wire names mirror the Rust
/// `AgentCommand` enum exactly.
enum AgentCommand {
  status('status'),
  startEngine('start_engine'),
  stopEngine('stop_engine'),
  restartEngine('restart_engine'),
  enableSystemDns('enable_system_dns'),
  disableSystemDns('disable_system_dns'),
  enableAutoStart('enable_auto_start'),
  disableAutoStart('disable_auto_start'),
  removeLegacyService('remove_legacy_service'),
  refresh('refresh'),
  openUi('open_ui'),
  quit('quit'),
  readConfig('read_config'),
  validateConfig('validate_config'),
  saveConfig('save_config'),
  readPreferences('read_preferences'),
  savePreferences('save_preferences');

  const AgentCommand(this.wire);

  final String wire;
}

class DnsServiceStatus {
  const DnsServiceStatus({
    required this.name,
    required this.device,
    required this.enabled,
    required this.servers,
    required this.automatic,
  });

  factory DnsServiceStatus.fromJson(Map<String, dynamic> json) {
    return DnsServiceStatus(
      name: json['name'] as String? ?? '',
      device: json['device'] as String? ?? '',
      enabled: json['enabled'] as bool? ?? false,
      servers: (json['servers'] as List<dynamic>? ?? const <dynamic>[])
          .map((value) => value as String)
          .toList(growable: false),
      automatic: json['automatic'] as bool? ?? false,
    );
  }

  final String name;
  final String device;
  final bool enabled;
  final List<String> servers;
  final bool automatic;

  bool get usesLoopbackDns =>
      servers.isNotEmpty && servers.every(_isLoopbackAddress);

  String get dnsDescription => automatic ? 'Automatic (DHCP)' : servers.join(', ');
}

bool _isLoopbackAddress(String server) {
  final normalized = server.trim().toLowerCase();
  return normalized == '::1' ||
      normalized == '0:0:0:0:0:0:0:1' ||
      normalized.startsWith('127.');
}

/// What the window shows, as reported by the agent. Lets a script confirm the
/// interface state without screenshots.
class UiSnapshot {
  const UiSnapshot({
    required this.running,
    required this.page,
    required this.language,
    required this.appearance,
    required this.configurationPath,
    required this.configurationExists,
    required this.configurationError,
  });

  factory UiSnapshot.fromJson(Map<String, dynamic> json) => UiSnapshot(
        running: json['running'] as bool? ?? false,
        page: json['page'] as String?,
        language: json['language'] as String? ?? 'chinese',
        appearance: json['appearance'] as String? ?? 'dark',
        configurationPath: json['configuration_path'] as String? ?? '',
        configurationExists: json['configuration_exists'] as bool? ?? false,
        configurationError: json['configuration_error'] as String?,
      );

  final bool running;
  final String? page;
  final String language;
  final String appearance;
  final String configurationPath;
  final bool configurationExists;
  final String? configurationError;
}

class AgentStatus {
  const AgentStatus({
    required this.engineRunning,
    required this.listener,
    required this.listenerReady,
    required this.systemDnsEnabled,
    required this.autostartEnabled,
    required this.configurationError,
    required this.legacyServiceDetected,
    required this.autostartSupported,
    required this.dnsServices,
    required this.uiRunning,
    this.ui,
  });

  factory AgentStatus.fromJson(Map<String, dynamic> json) {
    return AgentStatus(
      engineRunning: json['engine_running'] as bool? ?? false,
      listener: json['listener'] as String? ?? 'Unavailable',
      listenerReady: json['listener_ready'] as bool? ?? false,
      systemDnsEnabled: json['system_dns_enabled'] as bool? ?? false,
      autostartEnabled: json['autostart_enabled'] as bool? ?? false,
      configurationError: json['configuration_error'] as String?,
      legacyServiceDetected: json['legacy_service_detected'] as bool? ?? false,
      autostartSupported: json['autostart_supported'] as bool? ?? false,
      dnsServices: (json['dns_services'] as List<dynamic>? ?? const <dynamic>[])
          .map((value) =>
              DnsServiceStatus.fromJson(value as Map<String, dynamic>))
          .toList(growable: false),
      uiRunning: json['ui_running'] as bool? ?? false,
      ui: json['ui'] == null
          ? null
          : UiSnapshot.fromJson(json['ui'] as Map<String, dynamic>),
    );
  }

  factory AgentStatus.unavailable(String message) => AgentStatus(
        engineRunning: false,
        listener: 'Unavailable',
        listenerReady: false,
        systemDnsEnabled: false,
        autostartEnabled: false,
        configurationError: message,
        legacyServiceDetected: false,
        autostartSupported: false,
        dnsServices: const <DnsServiceStatus>[],
        uiRunning: false,
      );

  final bool engineRunning;
  final String listener;
  final bool listenerReady;
  final bool systemDnsEnabled;
  final bool autostartEnabled;
  final String? configurationError;
  final bool legacyServiceDetected;
  final bool autostartSupported;
  final List<DnsServiceStatus> dnsServices;
  final bool uiRunning;
  final UiSnapshot? ui;

  /// The listener can only take over system DNS when it is a loopback address
  /// on port 53, because that is what the network service configuration points
  /// at.
  bool get listenerSupportsSystemDns {
    final separator = listener.lastIndexOf(':');
    if (separator <= 0) {
      return false;
    }
    final host = listener
        .substring(0, separator)
        .replaceAll(RegExp(r'^\[|\]$'), '');
    return listener.substring(separator + 1) == '53' &&
        (host.startsWith('127.') || host == '::1');
  }
}

class ConfigSnapshot {
  const ConfigSnapshot({
    required this.path,
    required this.contents,
    required this.exists,
  });

  factory ConfigSnapshot.fromJson(Map<String, dynamic> json) => ConfigSnapshot(
        path: json['path'] as String? ?? '',
        contents: json['contents'] as String? ?? '',
        exists: json['exists'] as bool? ?? false,
      );

  final String path;
  final String contents;
  final bool exists;
}

/// Appearance choice. `system` follows the operating system.
enum AppearanceMode {
  system('system'),
  dark('dark'),
  light('light');

  const AppearanceMode(this.wire);

  final String wire;

  static AppearanceMode fromWire(String? wire) => values.firstWhere(
        (mode) => mode.wire == wire,
        orElse: () => AppearanceMode.system,
      );

  /// Resolves to an actual brightness, treating `system` as the platform value.
  bool resolvesToDark(Brightness platformBrightness) => switch (this) {
        AppearanceMode.system => platformBrightness == Brightness.dark,
        AppearanceMode.dark => true,
        AppearanceMode.light => false,
      };
}

class UiPreferences {
  const UiPreferences({
    this.language = 'chinese',
    this.appearance = 'system',
  });

  factory UiPreferences.fromJson(Map<String, dynamic> json) => UiPreferences(
        language: json['language'] as String? ?? 'chinese',
        appearance: json['appearance'] as String? ?? 'system',
      );

  final String language;
  final String appearance;

  bool get isChinese => language != 'english';

  AppearanceMode get appearanceMode => AppearanceMode.fromWire(appearance);

  UiPreferences copyWith({String? language, String? appearance}) =>
      UiPreferences(
        language: language ?? this.language,
        appearance: appearance ?? this.appearance,
      );

  Map<String, dynamic> toJson() => {
        'language': language,
        'appearance': appearance,
      };
}

class AgentResponse {
  const AgentResponse({
    required this.ok,
    required this.message,
    required this.status,
    this.config,
    this.preferences,
  });

  factory AgentResponse.fromJson(Map<String, dynamic> json) {
    return AgentResponse(
      ok: json['ok'] as bool? ?? false,
      message: json['message'] as String? ?? '',
      status: AgentStatus.fromJson(
        json['status'] as Map<String, dynamic>? ?? const <String, dynamic>{},
      ),
      config: json['config'] == null
          ? null
          : ConfigSnapshot.fromJson(json['config'] as Map<String, dynamic>),
      preferences: json['preferences'] == null
          ? null
          : UiPreferences.fromJson(json['preferences'] as Map<String, dynamic>),
    );
  }

  final bool ok;
  final String message;
  final AgentStatus status;
  final ConfigSnapshot? config;
  final UiPreferences? preferences;
}

/// Resolves the per-user directory that holds the agent control record. It
/// mirrors `control::state_directory` in the Rust agent.
Directory agentStateDirectory() {
  final environment = Platform.environment;
  final home = environment['HOME'];
  if (Platform.isMacOS) {
    if (home != null && home.isNotEmpty) {
      return Directory('$home/Library/Application Support/EdgeSteer');
    }
    return Directory('edgesteer');
  }
  if (Platform.isWindows) {
    final appData = environment['APPDATA'];
    if (appData != null && appData.isNotEmpty) {
      return Directory('$appData\\EdgeSteer');
    }
    if (home != null && home.isNotEmpty) {
      return Directory('$home\\AppData\\Roaming\\EdgeSteer');
    }
    return Directory('edgesteer');
  }
  final xdg = environment['XDG_CONFIG_HOME'];
  if (xdg != null && xdg.isNotEmpty) {
    return Directory('$xdg/edgesteer');
  }
  if (home != null && home.isNotEmpty) {
    return Directory('$home/.config/edgesteer');
  }
  return Directory('edgesteer');
}

class AgentClient {
  AgentClient({Directory? stateDirectory})
      : stateDirectory = stateDirectory ?? agentStateDirectory();

  final Directory stateDirectory;

  File get recordFile => File('${stateDirectory.path}/$controlRecordName');

  AgentRecord? readRecord() {
    final file = recordFile;
    if (!file.existsSync()) {
      return null;
    }
    try {
      final decoded = jsonDecode(file.readAsStringSync());
      if (decoded is! Map<String, dynamic>) {
        return null;
      }
      return AgentRecord.fromJson(decoded);
    } on FormatException {
      return null;
    } on FileSystemException {
      return null;
    }
  }

  bool get isAvailable => readRecord() != null;

  /// Sends one command. `fields` supplies the payload of commands that carry
  /// data; unit commands are sent as a bare string, matching the serde enum
  /// representation on the Rust side.
  Future<AgentResponse> request(
    AgentCommand command, {
    Map<String, dynamic>? fields,
    Duration timeout = const Duration(seconds: 75),
  }) async {
    final record = readRecord();
    if (record == null) {
      throw AgentException(
        'EdgeSteer Agent is not running: ${recordFile.path} is missing.',
      );
    }
    if (record.protocol != controlProtocol) {
      throw AgentException(
        'EdgeSteer Agent uses an incompatible control protocol; update the '
        'application and the interface together.',
      );
    }

    final payload = <String, dynamic>{
      'protocol': controlProtocol,
      'token': record.token,
      'command': fields == null || fields.isEmpty
          ? command.wire
          : {command.wire: fields},
    };

    Socket socket;
    try {
      socket = await Socket.connect(
        record.address,
        record.port,
        timeout: const Duration(seconds: 3),
      );
    } on SocketException catch (error) {
      throw AgentException('Connect to EdgeSteer Agent: ${error.message}');
    }

    try {
      socket.writeln(jsonEncode(payload));
      await socket.flush();
      final reply = await socket
          .cast<List<int>>()
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .first
          .timeout(timeout);
      final decoded = jsonDecode(reply);
      if (decoded is! Map<String, dynamic>) {
        throw AgentException('EdgeSteer Agent sent an unexpected reply.');
      }
      return AgentResponse.fromJson(decoded);
    } on TimeoutException {
      throw AgentException('EdgeSteer Agent did not answer in time.');
    } on SocketException catch (error) {
      throw AgentException('Read EdgeSteer Agent reply: ${error.message}');
    } on FormatException {
      throw AgentException('EdgeSteer Agent sent malformed JSON.');
    } finally {
      socket.destroy();
    }
  }

  Future<AgentStatus> status() async {
    final response = await request(AgentCommand.status);
    if (!response.ok) {
      throw AgentException(response.message);
    }
    return response.status;
  }

  Future<ConfigSnapshot> readConfig() async {
    final response = await request(AgentCommand.readConfig);
    final config = response.config;
    if (!response.ok || config == null) {
      throw AgentException(response.message);
    }
    return config;
  }

  /// Validates a draft with the agent's strict parser. Returns `null` when the
  /// document is valid, otherwise the parser's error message.
  Future<String?> validateConfig(String contents) async {
    final response = await request(
      AgentCommand.validateConfig,
      fields: {'contents': contents},
    );
    return response.ok ? null : response.message;
  }

  Future<AgentResponse> saveConfig(String contents) => request(
        AgentCommand.saveConfig,
        fields: {'contents': contents},
      );

  Future<UiPreferences> readPreferences() async {
    final response = await request(AgentCommand.readPreferences);
    return response.preferences ?? const UiPreferences();
  }

  Future<void> savePreferences(UiPreferences preferences) async {
    final response = await request(
      AgentCommand.savePreferences,
      fields: {'preferences': preferences.toJson()},
    );
    if (!response.ok) {
      throw AgentException(response.message);
    }
  }
}
