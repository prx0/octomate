use octocrab::{models::Label, Octocrab};
use serde::Deserialize;
use snafu::{ResultExt, Snafu};
use std::sync::Arc;
use tokio::fs;
use tracing::info;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Could not read file {path}"))]
    ConfigFile {
        source: tokio::io::Error,
        path: String,
    },
    #[snafu(display("SerdeJson error: {source}"))]
    SerdeJson { source: serde_json::Error },
    #[snafu(display("SerdeYaml error: {source}"))]
    SerdeYaml { source: serde_yaml::Error },
    #[snafu(display("Serde error: {source}"))]
    Hyper { source: hyper::http::Error },
    #[snafu(display("Octocrab error: {source}"))]
    Octocrab { source: octocrab::Error },
}

enum BatchFile<'a> {
    Yaml(&'a str),
    Json(&'a str),
}

#[derive(Deserialize, Debug, Clone)]
struct Batch {
    name: String,
    tasks: Vec<Task>,
}

#[derive(Deserialize, Debug, Clone)]
struct Task {
    command: Command,
}

#[derive(Deserialize, Debug, Clone)]
enum Command {
    CreateLabel(CreateLabelOptions),
}

enum OctocrabResult {
    CreateLabel(Label),
}

#[derive(Deserialize, Debug, Clone)]
struct CreateLabelOptions {
    owner: String,
    repo: String,
    name: String,
    color: String,
    description: String,
}

impl BatchFile<'_> {
    async fn read_batch(&self) -> Result<Batch, Error> {
        let batch = match self {
            Self::Json(path) => {
                let batch_file = fs::read(path).await.context(ConfigFileSnafu {
                    path: String::from(path.to_owned()),
                })?;
                serde_json::from_slice(&batch_file).context(SerdeJsonSnafu)?
            }
            Self::Yaml(path) => {
                let batch_file = fs::read(path).await.context(ConfigFileSnafu {
                    path: String::from(path.to_owned()),
                })?;
                serde_yaml::from_slice(&batch_file).context(SerdeYamlSnafu)?
            }
        };
        Ok(batch)
    }
}

struct Octomate {
    octocrab: Arc<Octocrab>,
}

impl Octomate {
    async fn new(personal_token: String) -> Result<Self, Error> {
        let octocrab_builder = octocrab::Octocrab::builder().personal_token(personal_token);
        let octocrab = octocrab::initialise(octocrab_builder).context(OctocrabSnafu)?;
        Ok(Self { octocrab })
    }

    async fn run_batch(&self, batch: &Batch) -> Result<Vec<Result<OctocrabResult, Error>>, Error> {
        info!("Running batch: {}", batch.name);
        let tasks = &batch.tasks;
        let command_iter = tasks.iter().map(|task| async move {
            match &task.command {
                Command::CreateLabel(options) => {
                    let CreateLabelOptions {
                        owner,
                        repo,
                        name,
                        color,
                        description,
                    } = options;
                    let label = self
                        .octocrab
                        .issues(owner, repo)
                        .create_label(name, color, description)
                        .await
                        .context(OctocrabSnafu)?;
                    Ok(OctocrabResult::CreateLabel(label))
                }
            }
        });
        let command_results: Vec<Result<OctocrabResult, Error>> =
            futures::future::join_all(command_iter).await;
        Ok(command_results)
    }

    async fn run_batch_from_file(
        &self,
        batch_file: &BatchFile<'_>,
    ) -> Result<Vec<Result<OctocrabResult, Error>>, Error> {
        let batch = batch_file.read_batch().await?;
        self.run_batch(&batch).await
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let personal_token =
        rpassword::prompt_password("Enter your personal access token (scope: repo): ")
            .expect("You need to enter a valid personal access token");
    let octomate = Octomate::new(personal_token)
        .await
        .expect("Unable to init octocrab");
    octomate
        .run_batch_from_file(&BatchFile::Yaml("batch.yml"))
        .await
        .expect("Unable to run batch from file");
}
