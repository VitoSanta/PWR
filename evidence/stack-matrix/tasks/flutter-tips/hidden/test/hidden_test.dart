import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:tips/tip_calculator.dart';

Future<void> pumpCalculator(WidgetTester tester) =>
    tester.pumpWidget(const MaterialApp(home: Scaffold(body: TipCalculator())));

Finder bill() => find.widgetWithText(TextField, 'Bill');

void main() {
  testWidgets('hidden: shares round up to cover the total', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), '10');
    for (var i = 0; i < 2; i++) {
      await tester.tap(find.ancestor(of: find.byTooltip('Add person'), matching: find.byType(IconButton)));
    }
    await tester.pump();
    expect(find.text('Total: \$11.50'), findsOneWidget);
    expect(find.text('Per person: \$3.84'), findsOneWidget);
  });

  testWidgets('hidden: half a cent rounds up, in cents', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), '0.10');
    await tester.pump();
    expect(find.text('Tip: \$0.02'), findsOneWidget);
    expect(find.text('Total: \$0.12'), findsOneWidget);
  });

  testWidgets('hidden: zero is a valid bill', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), '0');
    await tester.pump();
    expect(find.text('Enter a valid amount'), findsNothing);
    expect(find.text('Tip: \$0.00'), findsOneWidget);
  });

  testWidgets('hidden: removing people goes back down', (tester) async {
    await pumpCalculator(tester);
    final add = find.ancestor(of: find.byTooltip('Add person'), matching: find.byType(IconButton));
    final remove = find.ancestor(of: find.byTooltip('Remove person'), matching: find.byType(IconButton));
    await tester.tap(add);
    await tester.tap(add);
    await tester.pump();
    await tester.tap(remove);
    await tester.pump();
    expect(find.text('People: 2'), findsOneWidget);
    expect(tester.widget<IconButton>(remove).onPressed, isNotNull);
  });

  testWidgets('hidden: 10% on 19.99', (tester) async {
    await pumpCalculator(tester);
    await tester.enterText(bill(), '19.99');
    await tester.tap(find.text('10%'));
    await tester.pump();
    expect(find.text('Tip: \$2.00'), findsOneWidget);
    expect(find.text('Total: \$21.99'), findsOneWidget);
  });
}
