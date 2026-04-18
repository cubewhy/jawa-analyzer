use clap::Parser;

use crate::args::Cli;

mod args;
mod prepare;

fn main() {
    let args = Cli::parse();

    match args {
        Cli::Prepare { target } => {
            prepare::prepare(target);
        }
    }
}
