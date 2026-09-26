/// Cron expressions: parse, match, and find the next run.
library;

const _macros = {
  '@yearly': '0 0 1 1 *',
  '@annually': '0 0 1 1 *',
  '@monthly': '0 0 1 * *',
  '@weekly': '0 0 * * 0',
  '@daily': '0 0 * * *',
  '@hourly': '0 * * * *',
};

const _monthNames = ['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC'];
const _dayNames = ['SUN', 'MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT'];

class CronExpression {
  CronExpression._(this._minutes, this._hours, this._monthDays, this._months, this._weekDays, this._domStar, this._dowStar);

  final Set<int> _minutes, _hours, _monthDays, _months, _weekDays;
  final bool _domStar, _dowStar;

  static CronExpression parse(String source) {
    var text = source.trim();
    if (text.startsWith('@')) {
      final expanded = _macros[text.toLowerCase()];
      if (expanded == null) throw FormatException('unknown macro', source);
      text = expanded;
    }
    final fields = text.split(RegExp(r'\s+'));
    if (fields.length != 5) throw FormatException('expected 5 fields, got ${fields.length}', source);
    final weekDays = _field(fields[4], 0, 7, _dayNames).map((d) => d == 7 ? 0 : d).toSet();
    return CronExpression._(
      _field(fields[0], 0, 59, null),
      _field(fields[1], 0, 23, null),
      _field(fields[2], 1, 31, null),
      _field(fields[3], 1, 12, _monthNames, offset: 1),
      weekDays,
      fields[2] == '*',
      fields[4] == '*',
    );
  }

  static int _value(String text, int min, int max, List<String>? names, int offset) {
    final named = names?.indexOf(text.toUpperCase()) ?? -1;
    final value = named >= 0 ? named + offset : int.tryParse(text);
    if (value == null || value < min || value > max || !RegExp(r'^[0-9A-Za-z]+$').hasMatch(text)) {
      throw FormatException('bad value "$text" (allowed $min-$max)');
    }
    return value;
  }

  static Set<int> _field(String field, int min, int max, List<String>? names, {int offset = 0}) {
    final result = <int>{};
    for (final part in field.split(',')) {
      if (part.isEmpty) throw FormatException('empty list element in "$field"');
      final pieces = part.split('/');
      if (pieces.length > 2) throw FormatException('bad step in "$part"');
      var step = 1;
      if (pieces.length == 2) {
        step = int.tryParse(pieces[1]) ?? 0;
        if (step < 1 || !RegExp(r'^[0-9]+$').hasMatch(pieces[1])) throw FormatException('bad step in "$part"');
      }
      final range = pieces[0];
      int low, high;
      if (range == '*') {
        low = min;
        high = max;
      } else if (range.contains('-')) {
        final ends = range.split('-');
        if (ends.length != 2) throw FormatException('bad range "$range"');
        low = _value(ends[0], min, max, names, offset);
        high = _value(ends[1], min, max, names, offset);
        if (low > high) throw FormatException('reversed range "$range"');
      } else {
        low = _value(range, min, max, names, offset);
        high = pieces.length == 2 ? max : low;
      }
      for (var v = low; v <= high; v += step) {
        result.add(v);
      }
    }
    return result;
  }

  bool _dayMatches(DateTime t) {
    final dom = _monthDays.contains(t.day);
    final dow = _weekDays.contains(t.weekday % 7);
    if (!_domStar && !_dowStar) return dom || dow;
    return dom && dow;
  }

  bool matches(DateTime time) {
    final t = time.toUtc();
    return _minutes.contains(t.minute) && _hours.contains(t.hour) && _months.contains(t.month) && _dayMatches(t);
  }

  DateTime next(DateTime after) {
    final start = after.toUtc();
    var t = DateTime.utc(start.year, start.month, start.day, start.hour, start.minute).add(const Duration(minutes: 1));
    final limit = DateTime.utc(start.year + 9, start.month, start.day);
    while (t.isBefore(limit)) {
      if (!_months.contains(t.month)) {
        t = DateTime.utc(t.year, t.month + 1, 1);
        continue;
      }
      if (!_dayMatches(t)) {
        t = DateTime.utc(t.year, t.month, t.day + 1);
        continue;
      }
      if (!_hours.contains(t.hour)) {
        t = DateTime.utc(t.year, t.month, t.day, t.hour + 1);
        continue;
      }
      if (!_minutes.contains(t.minute)) {
        t = t.add(const Duration(minutes: 1));
        continue;
      }
      return t;
    }
    throw StateError('this expression never fires');
  }
}
