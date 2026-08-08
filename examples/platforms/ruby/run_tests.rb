# frozen_string_literal: true

# Runtime end-to-end driver for the generated Ruby demo gem.
#
# Unlike a pure file-content check, this actually COMPILES the generated C
# extension (which statically links the Rust staticlib), loads it, and exercises
# the demo API at runtime. Steps:
#
#   1. copy the generated gem sources into a throwaway build dir
#   2. build the extension from source: `BOLTFFI_CRATE_MANIFEST=… ruby extconf.rb && make`
#      (extconf re-invokes itself in build mode to cargo-build libdemo_ffi.a)
#   3. stage the built artifact onto the temporary lib load path
#   4. run the suite in a fresh interpreter: `ruby -I<build>/lib tests/all.rb`
#
# The build dir is always removed; a crash (signal) in the suite is reported
# distinctly from an ordinary test failure.

require "fileutils"
require "tmpdir"
require "rbconfig"
require "optparse"
require "shellwords"

options = {
  # Default to the exact interpreter running this script so the compiled
  # extension's ABI matches the interpreter that loads it.
  ruby: RbConfig.ruby,
  tests_dir: File.join(__dir__, "tests"),
}

OptionParser.new do |parser|
  parser.banner = "Usage: run_tests.rb --gem-src DIR --crate-manifest PATH [options]"
  parser.on("--gem-src DIR", "Generated Ruby gem sources (boltffi generate ruby output)") { |v| options[:gem_src] = v }
  parser.on("--crate-manifest PATH", "Cargo.toml of the crate to statically link") { |v| options[:manifest] = v }
  parser.on("--tests-dir DIR", "Directory holding all.rb + *_test.rb (default: ./tests)") { |v| options[:tests_dir] = v }
  parser.on("--ruby PATH", "Ruby interpreter to build + run with (default: this one)") { |v| options[:ruby] = v }
end.parse!(ARGV)

def fail_with(message)
  warn "run_tests.rb: #{message}"
  exit 1
end

def global_defined_symbols(artifact)
  nm = RbConfig::CONFIG["NM"]
  nm = "nm" if nm.nil? || nm.empty?
  base_command = Shellwords.split(nm)
  command = base_command + ["-g", "--defined-only", artifact]

  output = IO.popen(command, err: [:child, :out], &:read)
  unless $?.success?
    command = base_command + ["-g", artifact]
    output = IO.popen(command, err: [:child, :out], &:read)
    fail_with("`#{command.shelljoin}` failed") unless $?.success?
  end

  output.lines.filter_map do |line|
    fields = line.split
    next if fields.empty?

    type = fields.length >= 3 ? fields[-2] : fields[0]
    name = fields[-1]
    next if type.upcase == "U"

    name.sub(/^_/, "")
  end
end

def verify_exported_symbols(artifact)
  extension_base = File.basename(artifact).sub(/\.(bundle|so)\z/, "")
  expected_init = "Init_#{extension_base}"
  allowed = [expected_init, "ruby_abi_version"]
  unexpected = global_defined_symbols(artifact).reject { |symbol| allowed.include?(symbol) }

  return if unexpected.empty?

  fail_with("unexpected exported symbols in #{File.basename(artifact)}: #{unexpected.sort.join(', ')}")
end

gem_src = options[:gem_src] or fail_with("missing --gem-src")
manifest = options[:manifest] or fail_with("missing --crate-manifest")
tests_dir = options[:tests_dir]
ruby = options[:ruby]

gem_src = File.expand_path(gem_src)
manifest = File.expand_path(manifest)
tests_dir = File.expand_path(tests_dir)

fail_with("gem sources not found: #{gem_src}/lib/demo.rb (run `boltffi generate ruby` first)") unless File.exist?(File.join(gem_src, "lib", "demo.rb"))
fail_with("crate manifest not found: #{manifest}") unless File.exist?(manifest)
fail_with("test loader not found: #{File.join(tests_dir, 'all.rb')}") unless File.exist?(File.join(tests_dir, "all.rb"))

exit_code = 1
build_root = Dir.mktmpdir("boltffi-ruby-demo-")
begin
  # 1. Copy generated sources into the build dir.
  FileUtils.cp_r(File.join(gem_src, "."), build_root)

  ext_extconf = Dir.glob(File.join(build_root, "ext", "*", "extconf.rb")).first
  fail_with("no ext/*/extconf.rb under generated sources #{gem_src}") unless ext_extconf
  ext_dir = File.dirname(ext_extconf)
  lib_dir = File.join(build_root, "lib")

  # Always build from source: drop any stale artifacts a dirty `dist/` may carry,
  # so we never silently link a pre-built archive instead of compiling the crate.
  Dir.glob(File.join(ext_dir, "*.{o,bundle,so,a,native-libs}")).each { |f| FileUtils.rm_f(f) }
  FileUtils.rm_f(File.join(ext_dir, "Makefile"))

  # 2. Configure + build the extension (cargo-builds the staticlib via the
  #    Makefile rule extconf injects).
  make = RbConfig::CONFIG["MAKE"]
  make = "make" if make.nil? || make.empty?

  puts "==> building demo extension in #{ext_dir}"
  Dir.chdir(ext_dir) do
    # `make` matters as much as extconf here: the Makefile rule extconf injects
    # re-invokes extconf in build mode to cargo-build the staticlib, and that
    # build-mode pass reads BOLTFFI_CRATE_MANIFEST — so both calls need the env.
    env = { "BOLTFFI_CRATE_MANIFEST" => manifest }
    system(env, ruby, "extconf.rb") or fail_with("extconf.rb failed")
    system(env, make) or fail_with("`#{make}` failed to build the extension")
  end

  # 3. Stage the built artifact onto the temporary lib load path.
  artifact = Dir.glob(File.join(ext_dir, "*.{bundle,so}")).first
  fail_with("build produced no .bundle/.so in #{ext_dir}") unless artifact
  verify_exported_symbols(artifact)
  FileUtils.cp(artifact, lib_dir)
  puts "==> built #{File.basename(artifact)}, staged into #{lib_dir}"

  # 4. Run the suite in a fresh interpreter that can load the staged gem.
  all_rb = File.join(tests_dir, "all.rb")
  puts "==> running suite: #{ruby} -I#{lib_dir} #{all_rb}"
  passed = system(ruby, "-I", lib_dir, all_rb)

  # A native crash (SIGSEGV/SIGABRT) in the extension surfaces as a signalled
  # child, not an ordinary non-zero exit — call it out distinctly.
  warn "run_tests.rb: test process crashed (signal #{$?.termsig})" if $?&.signaled?
  exit_code = passed ? 0 : 1
ensure
  FileUtils.remove_entry(build_root) if build_root && File.directory?(build_root)
end

exit exit_code
