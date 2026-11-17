import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:edgesteer_ui/agent_client.dart';
import 'package:edgesteer_ui/app_state.dart';
import 'package:edgesteer_ui/config_document.dart';
import 'package:edgesteer_ui/main.dart';
import 'package:edgesteer_ui/strings.dart';
import 'package:edgesteer_ui/theme.dart';

const String sample = '''
{
  "listener": { "address": "127.0.0.1:53", "allow_remote": false },
  "cloudflare": { "range_refresh_secs": 86400 },
  "request_timeout_ms": 8000,
  "entry": "entry",
  "rule_sets": [],
  "plugins": [],
  "layers": [ { "tag": "entry", "type": "local" } ]
}
''';

EdgeSteerController loadedController() {
  final controller = EdgeSteerController();
  controller.document = ConfigDocument.fromSnapshot(
    const ConfigSnapshot(path: '/tmp/e.json', contents: sample, exists: true),
  );
  controller.loading = false;
  return controller;
}

Future<void> pumpShell(WidgetTester tester) async {
  tester.view.physicalSize = const Size(1180, 820);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);

  final controller = loadedController();
  await tester.pumpWidget(
    AnimatedBuilder(
      animation: controller,
      builder: (context, _) => MaterialApp(
        theme: buildTheme(dark: true),
        home: EdgeSteerScope(
          controller: controller,
          child: const EdgeSteerShell(strings: Strings.zh),
        ),
      ),
    ),
  );
  await tester.pump();
}

void main() {
  testWidgets('top bar is one flat row of five sections', (tester) async {
    await pumpShell(tester);

    for (final label in ['状态', '解析', '规则集', '优选', 'JSON']) {
      expect(find.text(label), findsWidgets, reason: 'missing tab $label');
    }
    // There is no configuration wrapper left to click through.
    expect(find.text('配置'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('status page merges runtime and system controls',
      (tester) async {
    await pumpShell(tester);

    // Status strip.
    expect(find.text('DNS 引擎'), findsOneWidget);
    expect(find.text('系统 DNS'), findsWidgets);
    // Merged system controls.
    expect(find.text('偏好'), findsOneWidget);
    expect(find.text('登录启动'), findsWidgets);
    // The resolver chain is gone from this page.
    expect(find.text('解析链'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('each tab opens its own page directly', (tester) async {
    await pumpShell(tester);

    await tester.tap(find.text('解析').first);
    await tester.pump();
    expect(find.text('监听'), findsOneWidget);

    await tester.tap(find.text('JSON').first);
    await tester.pump();
    expect(find.text('/tmp/e.json'), findsOneWidget);

    await tester.tap(find.text('规则集').first);
    await tester.pump();
    expect(find.text('监听'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('legacy page names still land on a page', (tester) async {
    // `config` and `system` predate the flat tabs; the shell maps them onto a
    // real page so older scripts do not break.
    await pumpShell(tester);
    expect(tester.takeException(), isNull);
  });
}
