# frozen_string_literal: true

# Runs every Ruby demo test against the installed demo gem.
#
# A test proves a demo case with a "case:<id>" assertion message. The demo
# audit (`just demo-test-audit`) reads those markers.
require "minitest/autorun"
require "demo"

Dir[File.join(__dir__, "tests", "**", "*_test.rb")].each { |test| require test }
