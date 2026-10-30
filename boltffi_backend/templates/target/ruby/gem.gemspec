# frozen_string_literal: true

# BoltFFI wrote this starter gemspec because it was missing. You own it, and
# BoltFFI does not change it again.
#
# `boltffi pack ruby` rewrites the extension directory: the C extension, its
# `extconf.rb`, and the Rust crate source. The install builds the crate with
# cargo, so it needs cargo and a C compiler.
Gem::Specification.new do |spec|
  spec.name = {{ gem }}
  spec.version = {{ version }}
  spec.summary = {{ summary }}
  spec.authors = ["BoltFFI"]
  spec.required_ruby_version = ">= 3.3"
  spec.require_paths = ["lib"]
  spec.extensions = [{{ extconf }}]
  spec.files = Dir.chdir(__dir__) do
    Dir["lib/**/*.rb", {{ extension_files|join(", ") }}].select { |path| File.file?(path) }
  end
end
