# Runs every test/test_*.rb with lib/ on the load path.
$LOAD_PATH.unshift(File.expand_path("../lib", __dir__))
Dir[File.expand_path("../test/test_*.rb", __dir__)].sort.each { |file| require file }
