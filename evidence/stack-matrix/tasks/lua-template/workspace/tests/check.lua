-- A small assertion harness: each check prints on failure, `done` exits with
-- the verdict.
package.path = "./src/?.lua;./tests/?.lua;" .. package.path

local check = { failures = 0, passed = 0 }

local function report(name, message)
  check.failures = check.failures + 1
  io.stderr:write(("FAIL %s: %s\n"):format(name, message))
end

function check.equal(name, got, want)
  if got == want then
    check.passed = check.passed + 1
  else
    report(name, ("want %q, got %q"):format(tostring(want), tostring(got)))
  end
end

-- Renders and compares, reporting an error raised as a failure.
function check.render(name, source, view, want)
  local template = require("template")
  local ok, got = pcall(template.render, source, view)
  if not ok then
    report(name, "raised " .. tostring(got))
  else
    check.equal(name, got, want)
  end
end

-- `source` must raise an error whose message contains `text`.
function check.raises(name, source, view, text)
  local template = require("template")
  local ok, message = pcall(template.render, source, view)
  if ok then
    report(name, ("rendered %q, want an error containing %q"):format(tostring(message), text))
  elseif not tostring(message):find(text, 1, true) then
    report(name, ("error %q does not contain %q"):format(tostring(message), text))
  else
    check.passed = check.passed + 1
  end
end

function check.done()
  print(("%s: %d passed, %d failed"):format(arg and arg[0] or "?", check.passed, check.failures))
  os.exit(check.failures == 0 and 0 or 1)
end

return check
