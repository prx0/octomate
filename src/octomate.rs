use crate::command;
use crate::error::Error;
use crate::io;
use octocrab::Octocrab;
use paris::info;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct Context<'a> {
    pub batch: &'a Batch,
    pub job: Option<&'a Job>,
    pub step: Option<&'a Step>,
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

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Batch {
    pub version: String,
    pub name: Option<String>,
    pub jobs: Vec<Job>,
}

type BatchResult<Output, Err> = Vec<Vec<StepResult<Output, Err>>>;

impl Batch {
    pub async fn run(&self, octocrab: &Octocrab) -> BatchResult<command::Response, Error> {
        println!();
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
        let batch = serde_yaml::from_slice(&batch_file)?;
        Ok(batch)
    }
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Job {
    pub name: Option<String>,
    pub on_repositories: Vec<Repository>,
    pub steps: Vec<Step>,
}

impl Job {
    pub async fn run(
        &self,
        octocrab: &Octocrab,
        ctx: &Context<'_>,
    ) -> Vec<StepResult<command::Response, Error>> {
        info!(
            "job: {}",
            &self.name.clone().unwrap_or("UNAMED".to_string())
        );
        let steps = &self.steps;
        let steps_iter = steps
            .iter()
            .map(|step| async move { step.run(octocrab, &ctx.update_from_job(self)).await });
        futures::future::join_all(steps_iter).await
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}

#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Step {
    pub name: Option<String>,
    pub runs: Vec<command::Command>,
}

type StepResult<Output, Err> = Vec<Vec<Result<Output, Err>>>;

impl Step {
    pub async fn run(
        &self,
        octocrab: &Octocrab,
        ctx: &Context<'_>,
    ) -> StepResult<command::Response, Error> {
        info!(
            "step: {}",
            &self.name.clone().unwrap_or("UNAMED".to_string())
        );
        let runs = &self.runs;
        let runs_iter = runs
            .iter()
            .map(|command| async move { command.run(octocrab, &ctx.update_from_step(self)).await });
        futures::future::join_all(runs_iter).await
    }
}

pub struct Octomate {
    octocrab: Arc<Octocrab>,
}

impl Octomate {
    pub async fn new(personal_token: impl Into<String>) -> Result<Self, Error> {
        let octocrab = octocrab::Octocrab::builder()
            .personal_token(personal_token.into())
            .build()?;
        Ok(Self {
            octocrab: Arc::new(octocrab),
        })
    }

    pub async fn run_batch(&self, batch: &Batch) -> BatchResult<command::Response, Error> {
        batch.run(&self.octocrab).await
    }

    pub async fn run_batch_from_file(
        &self,
        filepath: impl AsRef<Path>,
    ) -> Result<BatchResult<command::Response, Error>, Error> {
        let bytes = io::read_file(filepath).await?;
        let batch = Batch::try_from(bytes.as_slice())?;
        Ok(self.run_batch(&batch).await)
    }
}
