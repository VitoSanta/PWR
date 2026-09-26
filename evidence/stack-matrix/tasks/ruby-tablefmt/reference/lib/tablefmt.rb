module Tablefmt
  DELIMITER_CELL = /\A:?-+:?\z/

  module_function

  def format(text)
    lines = text.split("\n", -1)
    out = []
    i = 0
    fenced = false
    while i < lines.length
      line = lines[i]
      if line.start_with?("```")
        fenced = !fenced
        out << line
        i += 1
        next
      end
      if !fenced && row?(line) && i + 1 < lines.length && delimiter?(lines[i + 1])
        j = i + 2
        j += 1 while j < lines.length && row?(lines[j])
        out.concat(table(lines[i...j]))
        i = j
      else
        out << line
        i += 1
      end
    end
    out.join("\n")
  end

  def row?(line)
    line.lstrip.start_with?("|")
  end

  def delimiter?(line)
    return false unless row?(line)
    cells = cells(line)
    !cells.empty? && cells.all? { |cell| cell =~ DELIMITER_CELL }
  end

  def cells(line)
    body = line.strip
    body = body[1..-1] if body.start_with?("|")
    body = body[0...-1] if body.end_with?("|") && !body.end_with?("\\|")
    body.split(/(?<!\\)\|/, -1).map(&:strip)
  end

  def table(rows)
    header = cells(rows[0])
    marks = cells(rows[1])
    body = rows[2..-1].map { |row| cells(row) }
    count = ([header, marks] + body).map(&:length).max
    align = (0...count).map do |c|
      mark = marks[c] || "---"
      if mark.start_with?(":") && mark.end_with?(":") && mark.length > 1 then :center
      elsif mark.end_with?(":") then :right
      elsif mark.start_with?(":") then :explicit_left
      else :left
      end
    end
    grid = [header] + body
    widths = (0...count).map { |c| [3, *grid.map { |r| (r[c] || "").length }].max }
    render = lambda do |row|
      "| " + (0...count).map { |c| pad(row[c] || "", widths[c], align[c]) }.join(" | ") + " |"
    end
    delimiter = "| " + (0...count).map { |c| dashes(widths[c], align[c]) }.join(" | ") + " |"
    [render.call(header), delimiter] + body.map { |row| render.call(row) }
  end

  def pad(cell, width, align)
    space = width - cell.length
    case align
    when :right then " " * space + cell
    when :center then " " * (space / 2) + cell + " " * (space - space / 2)
    else cell + " " * space
    end
  end

  def dashes(width, align)
    case align
    when :center then ":" + "-" * (width - 2) + ":"
    when :right then "-" * (width - 1) + ":"
    when :explicit_left then ":" + "-" * (width - 1)
    else "-" * width
    end
  end
end
