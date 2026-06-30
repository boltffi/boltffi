mod build;
mod gem;
mod layout;
mod plan;
mod platform;

use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackRubyOptions;
use crate::config::Config;
use crate::reporter::Reporter;

use self::build::RubyArchiveBuilder;
use self::gem::RubyGemBuilder;
use self::layout::RubyStagingLayout;
use self::plan::RubyPackagingPlan;

pub fn pack_ruby(config: &Config, options: PackRubyOptions, reporter: &Reporter) -> Result<()> {
    if !config.is_ruby_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.ruby.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("💎", "Packing Ruby");

    let step = reporter.step("Preparing Ruby packaging plan");
    let plan = RubyPackagingPlan::from_config(config, &options)?;
    step.finish_success();

    let layout = RubyStagingLayout::new(config, &plan)?;

    if options.execution.regenerate {
        let step = reporter.step("Generating Ruby source structure");
        run_generate_with_output(
            config,
            GenerateOptions {
                target: GenerateTarget::Ruby,
                output: Some(layout.source_root.clone()),
                experimental: true,
                ir: true,
                cargo_args: options.execution.cargo_args.clone(),
            },
        )?;
        step.finish_success();
    }

    let archive_builder = RubyArchiveBuilder::new(&plan, &layout);
    let gem_builder = RubyGemBuilder::new(&plan, &layout);

    for target in &plan.targets {
        let step = reporter.step(&format!(
            "Building static library for {}",
            target.zigbuild_target
        ));
        if options.execution.no_build {
            archive_builder.verify_existing(target, &step)?;
        } else {
            archive_builder.build(target, &step)?;
        }
        step.finish_success();

        let step = reporter.step(&format!(
            "Building RubyGem for {}",
            target.rubygems_platform
        ));
        let gem_path = gem_builder.build_gem(target, &step)?;
        step.finish_success_with(&format!("{}", gem_path.display()));
    }

    reporter.finish();
    Ok(())
}
