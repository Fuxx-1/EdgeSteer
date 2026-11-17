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
  testWidgets('top bar has exactly three sections', (tester) async {
    await pumpShell(tester);
    expect(find.text('状态'), findsOneWidget);
    expect(find.text('配置'), findsOneWidget);
    expect(find.text('系统'), findsOneWidget);

    // The views that used to be top-level tabs must not be in the top bar.
    expect(find.text('规则集'), findsNothing);
    expect(find.text('优选'), findsNothing);
    expect(find.text('JSON'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('config section exposes four views, defaulting to resolvers',
      (tester) async {
    await pumpShell(tester);
    await tester.tap(find.text('配置'));
    await tester.pump();

    expect(find.text('解析'), findsWidgets);
    // A view label can also appear as a list heading inside the view itself.
    expect(find.text('规则集'), findsWidgets);
    expect(find.text('优选'), findsWidgets);
    expect(find.text('JSON'), findsWidgets);
    // Resolver detail form is shown first.
    expect(find.text('监听'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('switching config views swaps the editor', (tester) async {
    await pumpShell(tester);
    await tester.tap(find.text('配置'));
    await tester.pump();

    await tester.tap(find.text('JSON'));
    await tester.pump();
    expect(find.text('/tmp/e.json'), findsOneWidget);

    await tester.tap(find.text('规则集'));
    await tester.pump();
    expect(find.text('监听'), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('status section links into the resolver view', (tester) async {
    await pumpShell(tester);
    // Starts on status.
    expect(find.text('解析链'), findsOneWidget);
    await tester.tap(find.text('解析'));
    await tester.pump();
    // Now inside the config section on the resolver view.
    expect(find.text('监听'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
