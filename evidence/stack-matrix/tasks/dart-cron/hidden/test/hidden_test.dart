import 'package:cron/cron.dart';
import 'package:test/test.dart';

void main() {
  test('hidden: a single value with a step runs to the end of the field', () {
    final cron = CronExpression.parse('50/5 * * * *');
    expect(cron.next(DateTime.utc(2026, 1, 1, 0, 51)), DateTime.utc(2026, 1, 1, 0, 55));
    expect(cron.next(DateTime.utc(2026, 1, 1, 0, 55)), DateTime.utc(2026, 1, 1, 1, 50));
  });

  test('hidden: non-UTC input is converted first', () {
    final local = DateTime.utc(2026, 3, 1, 10, 0).toLocal();
    expect(CronExpression.parse('1 10 * * *').next(local), DateTime.utc(2026, 3, 1, 10, 1));
    expect(CronExpression.parse('1 10 * * *').next(local).isUtc, isTrue);
  });

  test('hidden: */1 day of week still counts as restricted', () {
    final cron = CronExpression.parse('0 0 31 * */1');
    expect(cron.next(DateTime.utc(2026, 9, 26)), DateTime.utc(2026, 9, 27));
  });

  test('hidden: month end across years and matches ignores seconds', () {
    expect(CronExpression.parse('59 23 31 12 *').next(DateTime.utc(2026, 12, 31, 23, 59)), DateTime.utc(2027, 12, 31, 23, 59));
    expect(CronExpression.parse('30 6 * * *').matches(DateTime.utc(2026, 5, 5, 6, 30, 45)), isTrue);
  });

  test('hidden: whitespace and case', () {
    expect(CronExpression.parse('  0\t0   1 dec  Sat ').next(DateTime.utc(2026, 11, 20)), DateTime.utc(2026, 12, 1));
  });
}
