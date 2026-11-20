# frozen_string_literal: true

# Runs every Ruby demo test against the installed demo gem.
#
# A test proves a demo case with a "case:<id>" assertion message. The demo
# audit (`just demo-test-audit`) reads those markers.
require "minitest/autorun"
require "demo"

class DemoGemTest < Minitest::Test
  def test_the_extension_defines_the_demo_module
    assert defined?(Demo), "case:ruby.module-defined"
  end
end

Dir[File.join(__dir__, "tests", "**", "*_test.rb")].each { |test| require test }

