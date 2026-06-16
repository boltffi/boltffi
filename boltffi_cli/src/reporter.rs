use std::time::{Duration, Instant};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

pub struct Reporter {
    verbosity: Verbosity,
    start_time: Instant,
}

impl Reporter {
    pub fn new(verbosity: Verbosity) -> Self {
        Self {
            verbosity,
            start_time: Instant::now(),
        }
    }

    pub fn section(&self, icon: &str, title: &str) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        println!();
        println!("{} {}", icon, style(title).bold().cyan());
    }

    pub fn step(&self, message: &str) -> Step {
        Step::new(self.verbosity, message)
    }

    pub fn warning(&self, message: &str) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        println!("   {} {}", style("⚠").yellow(), style(message).yellow());
    }

    pub fn finish(&self) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        println!();
        println!(
            "{} Done in {:.1}s",
            style("✨").green(),
            self.start_time.elapsed().as_secs_f64()
        );
    }
}

pub struct Step {
    spinner: Option<ProgressBar>,
    verbosity: Verbosity,
    message: String,
}

impl Step {
    fn new(verbosity: Verbosity, message: &str) -> Self {
        let spinner = if verbosity == Verbosity::Normal {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("   {spinner:.cyan} {msg}")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
            );
            pb.set_message(message.to_string());
            pb.enable_steady_tick(Duration::from_millis(80));
            Some(pb)
        } else {
            None
        };

        if verbosity == Verbosity::Verbose {
            println!("   {} {}", style("→").cyan(), message);
        }

        Self {
            spinner,
            verbosity,
            message: message.to_string(),
        }
    }

    pub fn is_verbose(&self) -> bool {
        self.verbosity == Verbosity::Verbose
    }

    pub fn finish_success(self) {
        if let Some(pb) = self.spinner {
            pb.finish_and_clear();
        }
        if self.verbosity != Verbosity::Quiet {
            println!("   {} {}", style("✓").green(), self.message);
        }
    }

    pub fn finish_success_with(self, message: &str) {
        if let Some(pb) = self.spinner {
            pb.finish_and_clear();
        }
        if self.verbosity != Verbosity::Quiet {
            println!("   {} {}", style("✓").green(), message);
        }
    }

    pub fn finish_failure(self, message: &str) {
        if let Some(pb) = self.spinner {
            pb.finish_and_clear();
        }
        if self.verbosity != Verbosity::Quiet {
            println!("   {} {}", style("✗").red(), message);
        }
    }
}
