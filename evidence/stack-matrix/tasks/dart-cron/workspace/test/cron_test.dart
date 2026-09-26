import 'package:cron/cron.dart';
import 'package:test/test.dart';

DateTime utc(int y, int mo, int d, [int h = 0, int mi = 0, int s = 0]) => DateTime.utc(y, mo, d, h, mi, s);

void main() {
  test('every fifteen minutes', () {
    final cron = CronExpression.parse('*/15 * * * *');
    expect(cron.next(utc(2026, 9, 26, 10, 7, 30)), utc(2026, 9, 26, 10, 15));
    expect(cron.next(utc(2026, 9, 26, 10, 15)), utc(2026, 9, 26, 10, 30));
    expect(cron.next(utc(2026, 9, 26, 23, 50)), utc(2026, 9, 27, 0, 0));
  });

  test('lists, ranges and steps', () {
    final cron = CronExpression.parse('0,30 9-17/4 * * MON-FRI');
    // 2026-09-26 is a Saturday.
    expect(cron.next(utc(2026, 9, 26, 12)), utc(2026, 9, 28, 9, 0));
    expect(cron.next(utc(2026, 9, 28, 9, 0)), utc(2026, 9, 28, 9, 30));
    expect(cron.next(utc(2026, 9, 28, 9, 30)), utc(2026, 9, 28, 13, 0));
    expect(cron.next(utc(2026, 9, 28, 17, 30)), utc(2026, 9, 29, 9, 0));
  });

  test('month and weekday names, and 7 as Sunday', () {
    expect(CronExpression.parse('0 12 * jan,Jul sun').next(utc(2026, 9, 26)), utc(2027, 1, 3, 12));
    expect(CronExpression.parse('0 0 * * 7').matches(utc(2026, 9, 27)), isTrue);
    expect(CronExpression.parse('0 0 * * 0').matches(utc(2026, 9, 27)), isTrue);
  });

  test('day of month or day of week when both are restricted', () {
    final cron = CronExpression.parse('0 0 13 * FRI');
    expect(cron.next(utc(2026, 9, 26)), utc(2026, 10, 2));
    expect(cron.next(utc(2026, 10, 2)), utc(2026, 10, 9));
    expect(cron.next(utc(2026, 10, 9)), utc(2026, 10, 13));
    final both = CronExpression.parse('0 0 13 * *');
    expect(both.next(utc(2026, 9, 26)), utc(2026, 10, 13));
  });

  test('macros', () {
    expect(CronExpression.parse('@hourly').next(utc(2026, 9, 26, 10, 0)), utc(2026, 9, 26, 11, 0));
    expect(CronExpression.parse('@weekly').next(utc(2026, 9, 26)), utc(2026, 9, 27));
    expect(CronExpression.parse('@yearly').next(utc(2026, 9, 26)), utc(2027, 1, 1));
  });

  test('leap days and impossible dates', () {
    expect(CronExpression.parse('0 0 29 2 *').next(utc(2026, 1, 1)), utc(2028, 2, 29));
    expect(() => CronExpression.parse('0 0 30 2 *').next(utc(2026, 1, 1)), throwsStateError);
  });

  test('bad expressions', () {
    for (final bad in ['* * * *', '60 * * * *', '* 24 * * *', '* * 0 * *', '* * * 13 *', '* * * * 8', '5-1 * * * *', '*/0 * * * *', '* * * FOO *', '@often', '1,,2 * * * *']) {
      expect(() => CronExpression.parse(bad), throwsFormatException, reason: bad);
    }
  });
}
