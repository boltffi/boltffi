# frozen_string_literal: true

require "demo"

# Minimal, stdlib-only test harness for the Ruby runtime e2e suite.
#
# minitest is no longer stdlib (nor even a bundled gem on Ruby 4.0), and the
# Ruby target deliberately avoids gem dependencies — so this provides just
# enough of minitest's surface to run the suite hermetically: test discovery,
# the three assertions the suite uses, `demo_case` tagging, and a pass/fail
# summary. The runner (DemoTestCase.run!) returns false when anything fails, and
# tests/all.rb turns that into a non-zero exit — so a broken expectation
# genuinely fails the build, it is not silently swallowed.
class DemoTestCase
  # Raised by the assertions on mismatch. The runner distinguishes these
  # (test failures) from any other exception (an unexpected error).
  class AssertionFailed < StandardError; end

  @subclasses = []

  class << self
    attr_reader :subclasses
  end

  # Every subclass auto-registers, so requiring a *_test.rb file is enough to
  # enrol its cases — no explicit suite list to keep in sync.
  def self.inherited(subclass)
    super
    DemoTestCase.subclasses << subclass
  end

  # Test cases are the public `test_*` methods declared directly on the subclass
  # (not the inherited helpers/assertions on DemoTestCase).
  def self.test_methods
    public_instance_methods(false).map(&:to_s).select { |name| name.start_with?("test_") }.sort
  end

  # Run every registered case. Prints a progress line, then any failures/errors,
  # then a summary. Returns true only if everything passed.
  def self.run!(io: $stdout)
    failures = []
    runs = 0
    assertions = 0

    subclasses.each do |klass|
      klass.test_methods.each do |method_name|
        runs += 1
        instance = klass.new
        begin
          instance.public_send(method_name)
          assertions += instance.assertion_count
          io.print "."
        rescue AssertionFailed => e
          assertions += instance.assertion_count
          failures << ["Failure", "#{klass}##{method_name}", e.message]
          io.print "F"
        rescue StandardError, ScriptError => e
          failures << ["Error", "#{klass}##{method_name}", "#{e.class}: #{e.message}"]
          io.print "E"
        end
      end
    end

    io.puts
    io.puts
    failures.each_with_index do |(label, name, message), index|
      io.puts "#{index + 1}) #{label}:"
      io.puts name
      io.puts message
      io.puts
    end

    failure_count = failures.count { |label, _, _| label == "Failure" }
    error_count = failures.count { |label, _, _| label == "Error" }
    io.puts "#{runs} runs, #{assertions} assertions, #{failure_count} failures, #{error_count} errors"

    failures.empty?
  end

  attr_reader :assertion_count

  def initialize
    @assertion_count = 0
    @demo_case = nil
  end

  # Record which cross-platform demo case the next assertion(s) exercise; the id
  # is prepended to any failure message so a red run names the exact behaviour.
  def demo_case(case_id)
    @demo_case = case_id
  end

  # Pack a list of byte values into the binary String that BoltFFI's Bytes
  # parameters expect (bytes cross as packed Strings, not Ruby Arrays).
  def packed(*values)
    values.pack("C*")
  end

  def assert_equal(expected, actual, message = nil)
    @assertion_count += 1
    return if expected == actual

    fail_case(message || "Expected #{actual.inspect} to equal #{expected.inspect}.")
  end

  def assert_in_delta(expected, actual, delta, message = nil)
    @assertion_count += 1
    return if (expected - actual).abs <= delta

    fail_case(message || "Expected #{actual.inspect} to be within #{delta} of #{expected.inspect}.")
  end

  def assert_kind_of(klass, actual, message = nil)
    @assertion_count += 1
    return if actual.is_a?(klass)

    fail_case(message || "Expected #{actual.inspect} to be a kind of #{klass}, not #{actual.class}.")
  end

  def assert_raises(exception_class, message = nil)
    @assertion_count += 1
    begin
      yield
    rescue exception_class => e
      return e
    rescue StandardError, ScriptError => e
      fail_case(message || "Expected #{exception_class} to be raised, but got #{e.class}: #{e.message}.")
    end
    fail_case(message || "Expected #{exception_class} to be raised, but nothing was raised.")
  end

  private

  def fail_case(message)
    prefix = @demo_case ? "#{@demo_case}: " : ""
    raise AssertionFailed, "#{prefix}#{message}"
  end
end
