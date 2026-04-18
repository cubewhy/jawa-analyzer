#[derive(Debug, clap::Parser)]
pub enum Cli {
    Prepare {
        #[arg(value_enum)]
        target: Option<PrepareTarget>,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
pub enum PrepareTarget {
    Cfr,
    Vineflower,
}
