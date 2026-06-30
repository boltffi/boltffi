# frozen_string_literal: true

# Loads every *_test.rb under this directory and runs them in one interpreter.
# Ruby has no built-in test discovery, so the loader is explicit; requiring a
# test file auto-registers its DemoTestCase subclass (see support.rb). The
# process exits non-zero on any failure, so a broken expectation fails the
# build rather than being silently swallowed.
require_relative "support"

Dir.glob(File.join(__dir__, "**", "*_test.rb")).sort.each { |test_file| require test_file }

exit(DemoTestCase.run! ? 0 : 1)
