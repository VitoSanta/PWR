require "minitest/autorun"
require "tablefmt"

class TestTablefmt < Minitest::Test
  def test_aligns_a_simple_table
    input = <<~MD
      # Prices

      |Item|Price|
      |-|-:|
      |Coffee|1.20|
      |Cake with cream|4.5|

      Done.
    MD
    expected = <<~MD
      # Prices

      | Item            | Price |
      | --------------- | ----: |
      | Coffee          |  1.20 |
      | Cake with cream |   4.5 |

      Done.
    MD
    assert_equal expected, Tablefmt.format(input)
  end

  def test_center_and_explicit_left
    input = "| a | b |\n|:-:|:--|\n| x | yy |\n"
    expected = "|  a  | b   |\n| :-: | :-- |\n|  x  | yy  |\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_center_puts_the_odd_space_on_the_right
    input = "|name|\n|:-:|\n|abcd|\n|ab|\n"
    expected = "| name |\n| :--: |\n| abcd |\n|  ab  |\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_missing_and_extra_cells
    input = "| h1 | h2 |\n| --- | --- |\n| only |\n| a | b | c |\n"
    expected = "| h1   | h2  |     |\n| ---- | --- | --- |\n| only |     |     |\n| a    | b   | c   |\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_escaped_pipes_stay_in_their_cell
    input = "| expr | meaning |\n|---|---|\n| a \\| b | either |\n"
    expected = "| expr   | meaning |\n| ------ | ------- |\n| a \\| b | either  |\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_code_fences_and_other_text_are_untouched
    input = "text | not a table\n```\n|a|b|\n|-|-|\n```\n  |x|\n  |-|\n"
    expected = "text | not a table\n```\n|a|b|\n|-|-|\n```\n| x   |\n| --- |\n"
    assert_equal expected, Tablefmt.format(input)
  end

  def test_rows_without_a_delimiter_are_not_a_table
    input = "|a|b|\n|c|d|\n"
    assert_equal input, Tablefmt.format(input)
  end
end
