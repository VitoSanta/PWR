import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:tips/tip_calculator.dart';

Future<void> pumpCalculator(WidgetTester tester) =>
    tester.pumpWidget(const MaterialApp(home: Scaffold(body: TipCalculator())));

Finder bill() => find.widgetWithText(TextField, 'Bill');

void main() {
  testWidgets('starts empty at 15% for one person', (tester) async {
    await pumpCalculator(tester);
    expect(find.text('Tip: —'), findsOneWidget);
    expect(find.text('Total: —'), findsOneWidget);
    expect(find.text('Per person: —'), findsOneWidget);
    expect(find.text('People: 1'), findsOneWidget);
    final chip = tester.widget<ChoiceChip>(find.widgetWithText(ChoiceChip, '15%'));
    expect(chip.selected, isTrue);
  });

  testWidgets('computes the tip, total and share', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), '42.50');
    await tester.pump();
    expect(find.text('Tip: \$6.38'), findsOneWidget);
    expect(find.text('Total: \$48.88'), findsOneWidget);
    expect(find.text('Per person: \$48.88'), findsOneWidget);
  });

  testWidgets('changing the percentage and the people', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), '100');
    await tester.tap(find.text('20%'));
    await tester.pump();
    expect(find.text('Tip: \$20.00'), findsOneWidget);
    expect(tester.widget<ChoiceChip>(find.widgetWithText(ChoiceChip, '15%')).selected, isFalse);
    await tester.tap(find.byTooltip('Add person'));
    await tester.tap(find.byTooltip('Add person'));
    await tester.pump();
    expect(find.text('People: 3'), findsOneWidget);
    expect(find.text('Per person: \$40.00'), findsOneWidget);
    await tester.enterText(bill(), '10');
    await tester.pump();
    // 12.00 / 3 = 4.00 exactly; 11.50 / 3 would round up.
    expect(find.text('Per person: \$4.00'), findsOneWidget);
  });

  testWidgets('invalid amounts', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), 'abc');
    await tester.pump();
    expect(find.text('Enter a valid amount'), findsOneWidget);
    expect(find.text('Total: —'), findsOneWidget);
    await tester.enterText(bill(), '-5');
    await tester.pump();
    expect(find.text('Enter a valid amount'), findsOneWidget);
    await tester.enterText(bill(), '');
    await tester.pump();
    expect(find.text('Enter a valid amount'), findsNothing);
  });

  testWidgets('people stay between 1 and 20', (tester) async {
    await pumpCalculator(tester);
    expect(tester.widget<IconButton>(find.ancestor(of: find.byTooltip('Remove person'), matching: find.byType(IconButton))).onPressed, isNull);
    for (var i = 0; i < 25; i++) {
      final add = find.ancestor(of: find.byTooltip('Add person'), matching: find.byType(IconButton));
      if (tester.widget<IconButton>(add).onPressed == null) break;
      await tester.tap(add);
      await tester.pump();
    }
    expect(find.text('People: 20'), findsOneWidget);
    expect(tester.widget<IconButton>(find.ancestor(of: find.byTooltip('Add person'), matching: find.byType(IconButton))).onPressed, isNull);
  });
}
