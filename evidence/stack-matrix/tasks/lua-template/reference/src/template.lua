-- A Mustache-style template renderer: see README.md.
local template = {}

local ESCAPES = { ["&"] = "&amp;", ["<"] = "&lt;", [">"] = "&gt;", ['"'] = "&quot;", ["'"] = "&#39;" }

local function escape(text)
  return (text:gsub("[&<>\"']", ESCAPES))
end

local function trim(text)
  return (text:match("^%s*(.-)%s*$"))
end

-- The template as a tree: text, names, raw names, sections and inverted
-- sections, each section holding its children.
local function parse(source)
  local root = { children = {} }
  local open_sections = { root }
  local position = 1
  while position <= #source do
    local current = open_sections[#open_sections]
    local open = source:find("{{", position, true)
    if not open then
      current.children[#current.children + 1] = { kind = "text", text = source:sub(position) }
      break
    end
    if open > position then
      current.children[#current.children + 1] = { kind = "text", text = source:sub(position, open - 1) }
    end
    local kind, name
    if source:sub(open, open + 2) == "{{{" then
      local close = source:find("}}}", open + 3, true)
      if not close then error("unclosed tag") end
      kind, name, position = "raw", trim(source:sub(open + 3, close - 1)), close + 3
    else
      local close = source:find("}}", open + 2, true)
      if not close then error("unclosed tag") end
      local inner = source:sub(open + 2, close - 1)
      position = close + 2
      local sigil = inner:sub(1, 1)
      if sigil == "!" then
        kind = "comment"
      elseif sigil == "&" then
        kind, name = "raw", trim(inner:sub(2))
      elseif sigil == "#" then
        kind, name = "section", trim(inner:sub(2))
      elseif sigil == "^" then
        kind, name = "inverted", trim(inner:sub(2))
      elseif sigil == "/" then
        kind, name = "close", trim(inner:sub(2))
      else
        kind, name = "name", trim(inner)
      end
    end
    if kind == "section" or kind == "inverted" then
      local node = { kind = kind, name = name, children = {} }
      current.children[#current.children + 1] = node
      open_sections[#open_sections + 1] = node
    elseif kind == "close" then
      if #open_sections == 1 or current.name ~= name then
        error("unexpected closing tag '" .. name .. "'")
      end
      open_sections[#open_sections] = nil
    elseif kind ~= "comment" then
      current.children[#current.children + 1] = { kind = kind, name = name }
    end
  end
  if #open_sections > 1 then
    error("unclosed section '" .. open_sections[#open_sections].name .. "'")
  end
  return root
end

local function lookup(stack, name)
  if name == "." then
    return stack[#stack]
  end
  local head, rest = name:match("^([^.]*)(.*)$")
  local value
  for index = #stack, 1, -1 do
    local context = stack[index]
    if type(context) == "table" and context[head] ~= nil then
      value = context[head]
      break
    end
  end
  for part in rest:gmatch("%.([^.]+)") do
    if type(value) ~= "table" then
      return nil
    end
    value = value[part]
  end
  return value
end

local function falsy(value)
  return value == nil or value == false or (type(value) == "table" and next(value) == nil)
end

local function render_nodes(nodes, stack, out)
  for _, node in ipairs(nodes) do
    if node.kind == "text" then
      out[#out + 1] = node.text
    elseif node.kind == "name" or node.kind == "raw" then
      local value = lookup(stack, node.name)
      if value ~= nil and value ~= false then
        local text = tostring(value)
        out[#out + 1] = node.kind == "name" and escape(text) or text
      end
    elseif node.kind == "section" then
      local value = lookup(stack, node.name)
      if type(value) == "table" and value[1] ~= nil then
        for _, item in ipairs(value) do
          stack[#stack + 1] = item
          render_nodes(node.children, stack, out)
          stack[#stack] = nil
        end
      elseif not falsy(value) then
        stack[#stack + 1] = value
        render_nodes(node.children, stack, out)
        stack[#stack] = nil
      end
    elseif node.kind == "inverted" then
      if falsy(lookup(stack, node.name)) then
        render_nodes(node.children, stack, out)
      end
    end
  end
end

function template.render(source, view)
  local out = {}
  render_nodes(parse(source).children, { view }, out)
  return table.concat(out)
end

return template
