import 'dart:convert';
import 'dart:io';

import 'package:edgesteer_ui/agent_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('decodes an agent status reply', () {
    final status = AgentStatus.fromJson(jsonDecode('''
    {
      "engine_running": true,
      "listener": "127.0.0.1:53",
      "listener_ready": true,
      "system_dns_enabled": false,
      "autostart_enabled": true,
      "configuration_error": null,
      "legacy_service_detected": false,
      "autostart_supported": true,
      "dns_services": [
        {"name": "Wi-Fi", "device": "en0", "enabled": true,
         "servers": ["127.0.0.1"], "automatic": false}
      ],
      "ui_running": true
    }
    ''') as Map<String, dynamic>);

    expect(status.engineRunning, isTrue);
    expect(status.listener, '127.0.0.1:53');
    expect(status.listenerSupportsSystemDns, isTrue);
    expect(status.dnsServices.single.usesLoopbackDns, isTrue);
    expect(status.dnsServices.single.dnsDescription, '127.0.0.1');
  });

  test('listener must be loopback port 53 for system DNS', () {
    AgentStatus withListener(String listener) => AgentStatus.fromJson({
          'listener': listener,
        });
    expect(withListener('127.0.0.1:53').listenerSupportsSystemDns, isTrue);
    expect(withListener('[::1]:53').listenerSupportsSystemDns, isTrue);
    expect(withListener('127.0.0.1:53535').listenerSupportsSystemDns, isFalse);
    expect(withListener('0.0.0.0:53').listenerSupportsSystemDns, isFalse);
    expect(withListener('Unavailable').listenerSupportsSystemDns, isFalse);
  });

  test('preferences default to Chinese dark', () {
    const preferences = UiPreferences();
    expect(preferences.isChinese, isTrue);
    expect(preferences.isDark, isTrue);
    expect(preferences.toJson(), {'language': 'chinese', 'appearance': 'dark'});
  });

  test('reads the agent record written by the Rust agent', () async {
    final directory = await Directory.systemTemp.createTemp('edgesteer-ui-test');
    addTearDown(() => directory.delete(recursive: true));
    File('${directory.path}/agent.json').writeAsStringSync(jsonEncode({
      'protocol': controlProtocol,
      'pid': 42,
      'address': '127.0.0.1:53535',
      'token': 'a' * 64,
    }));

    final record = AgentClient(stateDirectory: directory).readRecord();
    expect(record, isNotNull);
    expect(record!.protocol, controlProtocol);
    expect(record.address.address, '127.0.0.1');
    expect(record.port, 53535);
    expect(record.token.length, 64);
  });

  test('a missing record reports the agent as unavailable', () async {
    final directory = await Directory.systemTemp.createTemp('edgesteer-ui-test');
    addTearDown(() => directory.delete(recursive: true));
    final client = AgentClient(stateDirectory: directory);

    expect(client.isAvailable, isFalse);
    await expectLater(
      client.request(AgentCommand.status),
      throwsA(isA<AgentException>()),
    );
  });
}
