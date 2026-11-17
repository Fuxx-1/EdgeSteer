import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:edgesteer_ui/agent_client.dart';
import 'package:edgesteer_ui/app_state.dart';
import 'package:edgesteer_ui/config_document.dart';
import 'package:edgesteer_ui/pages/cloudflare_page.dart';
import 'package:edgesteer_ui/pages/json_page.dart';
import 'package:edgesteer_ui/pages/overview_page.dart';
import 'package:edgesteer_ui/pages/resolver_page.dart';
import 'package:edgesteer_ui/pages/rules_page.dart';
import 'package:edgesteer_ui/strings.dart';
import 'package:edgesteer_ui/theme.dart';

const String sample = '''
{
  "listener": { "address": "127.0.0.1:53", "allow_remote": false },
  "cloudflare": { "range_refresh_secs": 86400 },
  "request_timeout_ms": 8000,
  "entry": "entry",
  "rule_sets": [
    { "tag": "geosite-cn", "type": "remote", "url": "https://e/x.srs" }
  ],
  "plugins": [
    { "tag": "cf", "type": "cloudflare_preferred", "rewrite_ttl_secs": 60,
      "preferred": { "ipv4": "104.16.0.1" },
      "optimizer": { "enabled": true, "candidates": ["104.16.0.0/13"] } }
  ],
  "layers": [
    { "tag": "entry", "type": "local", "timeout_ms": 1800, "refresh_secs": 30 },
    { "tag": "doh", "type": "doh", "address": "1.1.1.1:443",
      "url": "https://cloudflare-dns.com/dns-query", "fallback": "entry",
      "plugin": "cf", "match": { "mode": "contains", "keywords": ["b2c"] } }
  ]
}
''';

EdgeSteerController controllerWithDocument() {
  final controller = EdgeSteerController();
  controller.document = ConfigDocument.fromSnapshot(
    const ConfigSnapshot(path: '/tmp/edgesteer.json', contents: sample, exists: true),
  );
  controller.loading = false;
  return controller;
}

Future<void> renderPage(WidgetTester tester, Widget page) async {
  tester.view.physicalSize = const Size(1180, 820);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);

  final controller = controllerWithDocument();
  await tester.pumpWidget(
    AnimatedBuilder(
      animation: controller,
      builder: (context, _) => MaterialApp(
        theme: buildTheme(dark: true),
        home: EdgeSteerScope(
          controller: controller,
          child: Scaffold(body: page),
        ),
      ),
    ),
  );
  await tester.pump();
  expect(tester.takeException(), isNull);
}

void main() {
  testWidgets('status page shows the chain and engine controls', (tester) async {
    await renderPage(
      tester,
      OverviewPage(
        strings: Strings.zh,
        onNotice: (_) {},
        onQuit: () {},
      ),
    );
    expect(find.text('DNS 引擎'), findsOneWidget);
    expect(find.text('启动'), findsOneWidget);
  });

  testWidgets('resolver page shows the master list and detail form',
      (tester) async {
    await renderPage(tester, ResolverPage(strings: Strings.zh, onNotice: (_) {}));
    expect(find.text('解析层'), findsOneWidget);
    expect(find.text('entry'), findsWidgets);
    expect(find.text('标签'), findsOneWidget);
  });

  testWidgets('rule set page renders the selected rule set', (tester) async {
    await renderPage(tester, RulesPage(strings: Strings.zh, onNotice: (_) {}));
    expect(find.text('geosite-cn'), findsWidgets);
    expect(find.text('来源'), findsOneWidget);
  });

  testWidgets('preferred page renders plugin fields', (tester) async {
    await renderPage(
      tester,
      CloudflarePage(strings: Strings.zh, onNotice: (_) {}),
    );
    expect(find.text('cf'), findsWidgets);
    expect(find.text('定时优选'), findsOneWidget);
  });

  testWidgets('json page renders the document', (tester) async {
    await renderPage(tester, JsonPage(strings: Strings.zh, onNotice: (_) {}));
    expect(find.text('/tmp/edgesteer.json'), findsOneWidget);
  });

  testWidgets('status page carries the system controls', (tester) async {
    await renderPage(
      tester,
      OverviewPage(
        strings: Strings.zh,
        onNotice: (_) {},
        onQuit: () {},
      ),
    );
    // Preferences, login item, and system DNS all live on the status page now.
    expect(find.text('偏好'), findsOneWidget);
    expect(find.text('登录启动'), findsWidgets);
    expect(find.text('系统 DNS'), findsWidgets);
    expect(find.text('退出 EdgeSteer'), findsWidgets);
    // The resolver chain moved to its own page.
    expect(find.text('解析链'), findsNothing);
  });

  testWidgets('english strings render without missing labels', (tester) async {
    await renderPage(tester, ResolverPage(strings: Strings.en, onNotice: (_) {}));
    expect(find.text('Layers'), findsOneWidget);
    expect(find.text('Tag'), findsOneWidget);
  });
}
