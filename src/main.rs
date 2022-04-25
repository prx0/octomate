use octocrab::{models::Label, Octocrab};
use serde::Deserialize;
use snafu::{ResultExt, Snafu};
use std::{path::Path, sync::Arc};
use tokio::fs;
use tracing::{debug, info};
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Could not read file {path}"))]
    IO {
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

async fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
    let bytes = fs::read(&path).await.context(IOSnafu {
        path: String::from(path.as_ref().to_str().unwrap_or("")),
    })?;
    Ok(bytes)
}

#[derive(Debug)]
struct Context<'a> {
    batch: &'a Batch,
    job: Option<&'a Job>,
    step: Option<&'a Step>,
}

impl<'a> Context<'a> {
    pub fn new(batch: &'a Batch, job: Option<&'a Job>, step: Option<&'a Step>) -> Self {
        Self { batch, job, step }
    }

    pub fn update_from_job(&self, job: &'a Job) -> Self {
        Self {
            batch: self.batch,
            job: Some(job),
            step: self.step,
        }
    }

    pub fn update_from_step(&self, step: &'a Step) -> Self {
        Self {
            batch: self.batch,
            job: self.job,
            step: Some(step),
        }
    }
}

impl<'a> From<&Context<'a>> for Context<'a> {
    fn from(ctx: &Context<'a>) -> Self {
        Self {
            batch: ctx.batch,
            job: ctx.job,
            step: ctx.step,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct Batch {
    version: String,
    name: Option<String>,
    jobs: Vec<Job>,
}

type BatchResult<Output, Err> = Vec<Vec<StepResult<Output, Err>>>;

impl Batch {
    pub async fn run(&self, octocrab: &Octocrab) -> BatchResult<OctocrabResult, Error> {
        info!(
            "Running batch: {} with version specs: {}",
            &self.name.clone().unwrap_or("UNAMED".to_string()),
            &self.version,
        );
        let jobs = &self.jobs;
        let jobs_iter = jobs
            .iter()
            .map(|job| async move { job.run(octocrab, &Context::new(self, None, None)).await });
        futures::future::join_all(jobs_iter).await
    }
}

impl TryFrom<&[u8]> for Batch {
    type Error = Error;

    fn try_from(batch_file: &[u8]) -> Result<Self, Self::Error> {
        let batch = serde_yaml::from_slice(&batch_file).context(SerdeYamlSnafu)?;
        Ok(batch)
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case")]
struct Job {
    name: Option<String>,
    on_repositories: Vec<Repository>,
    steps: Vec<Step>,
}

impl Job {
    pub async fn run(
        &self,
        octocrab: &Octocrab,
        ctx: &Context<'_>,
    ) -> Vec<StepResult<OctocrabResult, Error>> {
        debug!("{:?}", ctx);
        info!(
            "job: {}",
            &self.name.clone().unwrap_or("UNAMED".to_string())
        );
        let on_repositories = &self.on_repositories;
        let steps = &self.steps;
        let steps_iter = steps.iter().map(|step| async move {
            step.run(octocrab, on_repositories, &ctx.update_from_job(self))
                .await
        });
        futures::future::join_all(steps_iter).await
    }
}

#[derive(Deserialize, Debug, Clone)]
struct Repository {
    owner: String,
    name: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct Step {
    name: Option<String>,
    runs: Vec<Command>,
}

type StepResult<Output, Err> = Vec<Vec<Result<Output, Err>>>;

impl Step {
    pub async fn run(
        &self,
        octocrab: &Octocrab,
        repositories: &[Repository],
        ctx: &Context<'_>,
    ) -> StepResult<OctocrabResult, Error> {
        info!(
            "step: {}",
            &self.name.clone().unwrap_or("UNAMED".to_string())
        );
        let runs = &self.runs;
        let runs_iter = runs.iter().map(|command| async move {
            let on_repositories_iter = repositories.iter().map(|repository| async move {
                command
                    .run(
                        octocrab,
                        &repository.owner,
                        &repository.name,
                        &ctx.update_from_step(self),
                    )
                    .await
            });
            futures::future::join_all(on_repositories_iter).await
        });
        futures::future::join_all(runs_iter).await
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
enum Command {
    CreateLabel(CreateLabelOptions),
}

impl Command {
    pub async fn run(
        &self,
        octocrab: &Octocrab,
        owner: impl Into<String>,
        repo: impl Into<String>,
        ctx: &Context<'_>,
    ) -> Result<OctocrabResult, Error> {
        match self {
            Self::CreateLabel(options) => options.run(octocrab, owner, repo, ctx).await,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct CreateLabelOptions {
    name: String,
    color: String,
    description: String,
}

impl CreateLabelOptions {
    pub async fn run(
        &self,
        octocrab: &Octocrab,
        owner: impl Into<String>,
        repo: impl Into<String>,
        ctx: &Context<'_>,
    ) -> Result<OctocrabResult, Error> {
        debug!("{:?}", ctx);
        let label = octocrab
            .issues(owner, repo)
            .create_label(&self.name, &self.color, &self.description)
            .await
            .context(OctocrabSnafu)?;
        Ok(OctocrabResult::CreateLabel(label))
    }
}

enum OctocrabResult {
    CreateLabel(Label),
}

struct Octomate {
    octocrab: Arc<Octocrab>,
}

impl Octomate {
    async fn new(personal_token: String) -> Result<Self, Error> {
        let octocrab = octocrab::Octocrab::builder()
            .personal_token(personal_token)
            .build()
            .context(OctocrabSnafu)?;
        Ok(Self {
            octocrab: Arc::new(octocrab),
        })
    }

    async fn run_batch(&self, batch: &Batch) -> BatchResult<OctocrabResult, Error> {
        batch.run(&self.octocrab).await
    }

    async fn run_batch_from_file(
        &self,
        filepath: impl AsRef<Path>,
    ) -> Result<BatchResult<OctocrabResult, Error>, Error> {
        let bytes = read_file(filepath).await?;
        let batch = Batch::try_from(bytes.as_slice())?;
        Ok(self.run_batch(&batch).await)
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
        .run_batch_from_file("batch.yml")
        .await
        .expect("Unable to run batch from file");
}
