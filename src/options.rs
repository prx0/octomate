use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Options {
    #[clap(long, help = "The batch file to run")]
    pub batch_file: String,
}

impl Options {
    pub fn from_cli() -> Self {
        Options::parse()
    }
}
