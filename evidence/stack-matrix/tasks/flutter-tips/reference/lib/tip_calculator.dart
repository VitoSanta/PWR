import 'package:flutter/material.dart';

class TipCalculator extends StatefulWidget {
  const TipCalculator({super.key});

  @override
  State<TipCalculator> createState() => _TipCalculatorState();
}

class _TipCalculatorState extends State<TipCalculator> {
  static const percentages = [10, 15, 20];
  String _bill = '';
  int _percent = 15;
  int _people = 1;

  /// The bill in cents, or null when it is not a valid amount.
  int? get _billCents {
    final value = double.tryParse(_bill.trim());
    if (value == null || value < 0 || value.isNaN || value.isInfinite) return null;
    return (value * 100).round();
  }

  String _money(int cents) => '\$${(cents ~/ 100)}.${(cents % 100).toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final bill = _billCents;
    final invalid = _bill.trim().isNotEmpty && bill == null;
    String tip = '—', total = '—', share = '—';
    if (bill != null) {
      final tipCents = (bill * _percent + 50) ~/ 100;
      final totalCents = bill + tipCents;
      final shareCents = (totalCents + _people - 1) ~/ _people;
      tip = _money(tipCents);
      total = _money(totalCents);
      share = _money(shareCents);
    }
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: InputDecoration(
              labelText: 'Bill',
              errorText: invalid ? 'Enter a valid amount' : null,
            ),
            onChanged: (value) => setState(() => _bill = value),
          ),
          Wrap(
            spacing: 8,
            children: [
              for (final percent in percentages)
                ChoiceChip(
                  label: Text('$percent%'),
                  selected: _percent == percent,
                  onSelected: (_) => setState(() => _percent = percent),
                ),
            ],
          ),
          Row(
            children: [
              IconButton(
                tooltip: 'Remove person',
                icon: const Icon(Icons.remove),
                onPressed: _people > 1 ? () => setState(() => _people--) : null,
              ),
              Text('People: $_people'),
              IconButton(
                tooltip: 'Add person',
                icon: const Icon(Icons.add),
                onPressed: _people < 20 ? () => setState(() => _people++) : null,
              ),
            ],
          ),
          Text('Tip: $tip'),
          Text('Total: $total'),
          Text('Per person: $share'),
        ],
      ),
    );
  }
}
