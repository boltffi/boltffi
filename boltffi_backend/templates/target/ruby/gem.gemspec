# frozen_string_literal: true

# BoltFFI wrote this starter gemspec because it was missing. You own it, and
# BoltFFI does not change it again.
#
# `boltffi pack ruby` rewrites the extension directory: the C extension, its
# `extconf.rb`, and the Rust crate source. The install builds the crate with
# cargo, so it needs cargo and a C compiler.
crate_manifest = File.expand_path(File.join(File.dirname({{ extconf }}), "crate", "Cargo.toml"), __dir__)
unless File.file?(crate_manifest)
  raise "boltffi: no crate source at #{crate_manifest}; run `boltffi pack ruby` to copy the crate source into the gem"
end

Gem::Specification.new do |spec|
  spec.name = {{ gem }}
  spec.version = {{ version }}
  spec.summary = {{ summary }}
  spec.authors = ["BoltFFI"]
  spec.required_ruby_version = ">= 3.3"
  spec.require_paths = ["lib"]
  spec.extensions = [{{ extconf }}]
  # FNM_DOTMATCH keeps hidden crate files, such as `.cargo/config.toml`.
  spec.files = Dir.chdir(__dir__) do
    Dir.glob(["lib/**/*.rb", {{ extension_files|join(", ") }}], File::FNM_DOTMATCH).select { |path| File.file?(path) }
  end
end
