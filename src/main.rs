use octocrab::{models::Label, Octocrab};
use serde::Deserialize;
use snafu::{ResultExt, Snafu};
use std::sync::Arc;
use tokio::fs;
use tracing::{info, debug};
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
    name: Option<String>,
    jobs: Vec<Job>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
struct Job {
    name: Option<String>,
    on_repositories: Vec<Repository>,
    steps: Vec<Step>,
}

#[derive(Deserialize, Debug, Clone)]
struct Repository {
    owner: String,
    name: String,
}

#[derive(Deserialize, Debug, Clone)]
struct Step {
    name: Option<String>,
    runs: Vec<Command>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
enum Command {
    CreateLabel(CreateLabelOptions),
}

#[derive(Deserialize, Debug, Clone)]
struct CreateLabelOptions {
    name: String,
    color: String,
    description: String,
}

enum OctocrabResult {
    CreateLabel(Label),
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

type BatchRes = Vec<Vec<Vec<Vec<Result<OctocrabResult, Error>>>>>;

impl Octomate {
    async fn new(personal_token: String) -> Result<Self, Error> {
        let octocrab = octocrab::Octocrab::builder().personal_token(personal_token).build().context(OctocrabSnafu)?;
        Ok(Self { octocrab: Arc::new(octocrab) })
    }

    async fn run_batch(&self, batch: &Batch) -> Result<BatchRes, Error> {
        info!(
            "Running batch: {}",
            batch.name.clone().unwrap_or("UNAMED".to_string())
        );
        let jobs = &batch.jobs;

        let jobs_iter = jobs.iter().map(|job| async move {
            info!("job: {}", &job.name.clone().unwrap_or("UNAMED".to_string()));
            let on_repositories = &job.on_repositories;
            let steps = &job.steps;
            let steps_iter = steps.iter().map(|step| async move {
                info!(
                    "step: {}",
                    &step.name.clone().unwrap_or("UNAMED".to_string())
                );
                let runs = &step.runs;
                let runs_iter = runs.iter().map(|command| async move {
                    let repositories = &on_repositories;
                    let on_repositories_iter = repositories.iter().map(|repository| async move {
                        match command {
                            Command::CreateLabel(options) => {
                                debug!("options: {:?}", options);
                                let label = self
                                    .octocrab
                                    .issues(&repository.owner, &repository.name)
                                    .create_label(
                                        &options.name,
                                        &options.color,
                                        &options.description,
                                    )
                                    .await
                                    .context(OctocrabSnafu)?;
                                Ok(OctocrabResult::CreateLabel(label))
                            }
                        }
                    });
                    futures::future::join_all(on_repositories_iter).await
                });
                futures::future::join_all(runs_iter).await
            });
            futures::future::join_all(steps_iter).await
        });

        let job_results: BatchRes = futures::future::join_all(jobs_iter).await;

        Ok(job_results)
    }

    async fn run_batch_from_file(&self, batch_file: &BatchFile<'_>) -> Result<BatchRes, Error> {
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
