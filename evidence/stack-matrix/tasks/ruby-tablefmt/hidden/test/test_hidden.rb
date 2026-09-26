require "minitest/autorun"
require "tablefmt"

class TestHidden < Minitest::Test
  def test_two_tables_and_no_trailing_newline
    input = "|a|\n|-|\n|b|\n\n|long header|\n|--:|\n|1|"
    expected = "| a   |\n| --- |\n| b   |\n\n| long header |\n| ----------: |\n|           1 |"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_rows_without_outer_pipes_continue_the_table_only_when_they_start_with_a_pipe
    input = "| a | b |\n|---|---|\n| 1 | 2 |\nplain line\n"
    expected = "| a   | b   |\n| --- | --- |\n| 1   | 2   |\nplain line\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_a_header_without_trailing_pipe
    input = "| a | b\n| - | -\n| 1 | 2\n"
    expected = "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_idempotent
    input = "|x|yy|\n|:-:|--:|\n|1|2|\n"
    once = Tablefmt.format(input)
    assert_equal once, Tablefmt.format(once)
  end

  def test_empty_cells_and_unicode_length
    input = "|città|n|\n|-|-|\n||é|\n"
    expected = "| città | n   |\n| ----- | --- |\n|       | é   |\n"
    assert_equal expected, Tablefmt.format(input)
  end
end
