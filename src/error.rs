#[derive(Debug)]
pub enum Error {
    IO(tokio::io::Error),
    SerdeJson(serde_json::Error),
    SerdeYaml(serde_yaml::Error),
    Octocrab(octocrab::Error),
}

impl From<tokio::io::Error> for Error {
    fn from(err: tokio::io::Error) -> Self {
        Error::IO(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerdeJson(err)
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Self {
        Error::SerdeYaml(err)
    }
}

impl From<octocrab::Error> for Error {
    fn from(err: octocrab::Error) -> Self {
        Error::Octocrab(err)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Error {}", self))
    }
}

impl std::error::Error for Error {}
