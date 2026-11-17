import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:edgesteer_ui/window_controls.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final calls = <String>[];

  setUp(() {
    calls.clear();
    messenger.setMockMethodCallHandler(
      const MethodChannel('io.edgesteer/window'),
      (call) async {
        calls.add(call.method);
        return null;
      },
    );
  });

  tearDown(() {
    messenger.setMockMethodCallHandler(
      const MethodChannel('io.edgesteer/window'),
      null,
    );
  });

  Future<void> pump(WidgetTester tester) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: Center(child: WindowControls())),
      ),
    );
  }

  testWidgets('renders three lights', (tester) async {
    await pump(tester);
    expect(find.byType(WindowControls), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('each light drives its window action', (tester) async {
    await pump(tester);

    // The three dots are the only GestureDetectors in the tree.
    final dots = find.byType(GestureDetector);
    expect(dots, findsNWidgets(3));

    for (var index = 0; index < 3; index++) {
      await tester.tap(dots.at(index));
      await tester.pump();
    }

    expect(calls, ['close', 'minimize', 'zoom']);
  });

  testWidgets('a missing host channel does not throw', (tester) async {
    messenger.setMockMethodCallHandler(
      const MethodChannel('io.edgesteer/window'),
      (call) async => throw MissingPluginException('no host'),
    );
    await pump(tester);

    await tester.tap(find.byType(GestureDetector).first);
    await tester.pump();
    expect(tester.takeException(), isNull);
  });
}
