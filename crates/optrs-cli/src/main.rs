// crates/optrs-cli/src/main.rs
//! Command line driver. Two subcommands: `price` for one engine, `compare` to
//! run every supporting engine side by side — the latter is how you eyeball
//! whether a change broke an engine before the test suite tells you.

use clap::{Parser, Subcommand, ValueEnum};
use optrs_core::analytic::{BsmInputs, OptionType};
use optrs_core::instrument::PriceRequest;
use optrs_engine::{Config, Engine};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliKind {
    Call,
    Put,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliEngine {
    Analytic,
    Cos,
    TreeCrr,
    TreeLr,
    Fd,
    Mc,
}

impl From<CliEngine> for Engine {
    fn from(e: CliEngine) -> Self {
        match e {
            CliEngine::Analytic => Engine::Analytic,
            CliEngine::Cos => Engine::Cos,
            CliEngine::TreeCrr => Engine::TreeCrr,
            CliEngine::TreeLr => Engine::TreeLr,
            CliEngine::Fd => Engine::FiniteDifference,
            CliEngine::Mc => Engine::MonteCarlo,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "optrs", version, about = "Option pricing across four numerical methods")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Debug, Clone)]
struct OptionArgs {
    #[arg(short = 's', long)]
    spot: f64,
    #[arg(short = 'k', long)]
    strike: f64,
    #[arg(short = 'r', long, default_value_t = 0.0)]
    rate: f64,
    #[arg(short = 'q', long, default_value_t = 0.0)]
    div_yield: f64,
    #[arg(short = 'v', long)]
    vol: f64,
    #[arg(short = 't', long)]
    time: f64,
    #[arg(long, value_enum, default_value_t = CliKind::Call)]
    kind: CliKind,
    /// American exercise.
    #[arg(long, conflicts_with = "bermudan")]
    american: bool,
    /// Comma-separated Bermudan exercise dates in years, e.g. 0.25,0.5,0.75,1.0
    #[arg(long, value_delimiter = ',')]
    bermudan: Option<Vec<f64>>,
    #[arg(long)]
    tree_steps: Option<usize>,
    #[arg(long)]
    mc_paths: Option<usize>,
    #[arg(long)]
    fd_steps: Option<usize>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Price with one engine.
    Price {
        #[command(flatten)]
        opt: OptionArgs,
        #[arg(short, long, value_enum)]
        engine: Option<CliEngine>,
        /// Refine until converged instead of a single fixed-resolution run.
        #[arg(long)]
        converged: bool,
        #[arg(long)]
        greeks: bool,
    },
    /// Run every supporting engine and show the spread against the first.
    Compare {
        #[command(flatten)]
        opt: OptionArgs,
    },
}

impl OptionArgs {
    fn build(&self) -> (PriceRequest, Config) {
        let inputs = BsmInputs {
            spot: self.spot,
            strike: self.strike,
            rate: self.rate,
            div_yield: self.div_yield,
            vol: self.vol,
            time: self.time,
        };
        let kind = match self.kind {
            CliKind::Call => OptionType::Call,
            CliKind::Put => OptionType::Put,
        };
        let req = if let Some(dates) = &self.bermudan {
            PriceRequest::bermudan(inputs, kind, dates.clone())
        } else if self.american {
            PriceRequest::american(inputs, kind)
        } else {
            PriceRequest::european(inputs, kind)
        };

        let mut cfg = Config::default();
        if let Some(n) = self.tree_steps {
            cfg.tree.steps = n;
        }
        if let Some(n) = self.mc_paths {
            cfg.mc.paths = n;
        }
        if let Some(n) = self.fd_steps {
            cfg.fd.space_steps = n;
            cfg.fd.time_steps = n;
        }
        (req, cfg)
    }
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> optrs_core::Result<()> {
    match cli.command {
        Command::Price { opt, engine, converged, greeks } => {
            let (req, cfg) = opt.build();
            let engine = engine.map(Engine::from).unwrap_or_else(|| Engine::best_for(&req));

            if greeks {
                let g = optrs_engine::greeks(engine, &req, &cfg)?;
                println!("engine   {}", engine.name());
                println!("price    {:>14.8}", g.price);
                println!("delta    {:>14.8}", g.delta);
                println!("gamma    {:>14.8}", g.gamma);
                println!("vega     {:>14.8}", g.vega);
                println!("theta    {:>14.8}", g.theta);
                println!("rho      {:>14.8}", g.rho);
                return Ok(());
            }

            if converged {
                let rep = optrs_engine::price_converged(engine, &req, &cfg)?;
                println!("engine   {}", engine.name());
                println!("price    {:>14.10}", rep.result.price);
                println!("refined  {} level(s), delta {:.3e}", rep.refinements, rep.final_delta);
                if rep.extrapolated {
                    println!("note     Richardson extrapolated");
                }
            } else {
                let r = engine.price_raw(&req, &cfg)?;
                println!("engine   {}", engine.name());
                println!("price    {:>14.10}", r.price);
                if let Some(se) = r.std_error {
                    println!("std err  {:>14.3e}  (95% CI +/- {:.6})", se, 1.96 * se);
                }
            }
            Ok(())
        }

        Command::Compare { opt } => {
            let (req, cfg) = opt.build();
            let results = optrs_engine::price_all(&req, &cfg);

            // Reference = first engine that succeeded, preferring exact ones.
            let reference = results.iter().find_map(|(_, r)| r.as_ref().ok().map(|r| r.price));

            println!("{:<20} {:>16} {:>14} {:>12}", "engine", "price", "diff", "std err");
            println!("{}", "-".repeat(66));
            for (engine, res) in &results {
                match res {
                    Ok(r) => {
                        let diff = reference.map(|b| r.price - b).unwrap_or(f64::NAN);
                        let se = r.std_error.map(|s| format!("{s:.3e}")).unwrap_or_else(|| "-".into());
                        println!("{:<20} {:>16.10} {:>14.2e} {:>12}", engine.name(), r.price, diff, se);
                    }
                    Err(e) => println!("{:<20} {:>16} ({e})", engine.name(), "-"),
                }
            }
            Ok(())
        }
    }
}
