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
