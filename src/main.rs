use clap::Parser;
use octocrab::Octocrab;
use serde::Deserialize;
use snafu::{ResultExt, Snafu};
use std::{path::Path, sync::Arc};
use tokio::fs;
use tracing::{debug, info};
use tracing_subscriber::{
    filter::{EnvFilter, LevelFilter},
    fmt::format::FmtSpan,
};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not read file {path}"))]
    IO {
        source: tokio::io::Error,
        path: String,
    },
    #[snafu(display("SerdeJson error: {source}"))]
    SerdeJson { source: serde_json::Error },
    #[snafu(display("SerdeYaml error: {source}"))]
    SerdeYaml { source: serde_yaml::Error },
    #[snafu(display("Octocrab error: {source}"))]
    Octocrab { source: octocrab::Error },
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Options {
    #[clap(long, help = "The batch file to run")]
    pub batch_file: String,
}

async fn read_file(path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
    let bytes = fs::read(&path).await.context(IOSnafu {
        path: String::from(path.as_ref().to_str().unwrap_or_default()),
    })?;
    Ok(bytes)
}

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
        debug!("{:?}", ctx);
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

pub mod command {
    use super::Context;
    use super::Error;
    use super::Octocrab;
    use crate::OctocrabSnafu;
    use octocrab::models::{gists::Gist, issues::Issue, teams::Team, Label};
    use serde::Deserialize;
    use snafu::ResultExt;
    use tracing::{debug, info};

    #[derive(Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    pub enum Command {
        CreateLabel(CreateLabelOptions),
        CreateIssue(CreateIssueOptions),
        CreateTeam(CreateTeamOptions),
        CreateGist(CreateGistOptions),
    }

    impl Command {
        pub async fn run(
            &self,
            octocrab: &Octocrab,
            ctx: &Context<'_>,
        ) -> Vec<Result<Response, Error>> {
            match self {
                Self::CreateLabel(options) => options.run(octocrab, ctx).await,
                Self::CreateIssue(options) => options.run(octocrab, ctx).await,
                Self::CreateTeam(options) => options.run(octocrab, ctx).await,
                Self::CreateGist(options) => options.run(octocrab, ctx).await,
            }
        }
    }

    #[derive(Deserialize, Debug, Clone, PartialEq)]
    pub struct CreateGistOptions {
        pub title: String,
        pub content: String,
        pub description: Option<String>,
        pub public: Option<bool>,
    }

    impl CreateGistOptions {
        pub async fn run(
            &self,
            octocrab: &Octocrab,
            ctx: &Context<'_>,
        ) -> Vec<Result<Response, Error>> {
            debug!("{:?}", ctx);
            let gist_res = octocrab
                .gists()
                .create()
                .file(&self.title, &self.content)
                .description(&self.description.clone().unwrap_or_default())
                .public(self.public.unwrap_or(false))
                .send()
                .await
                .context(OctocrabSnafu);

            match gist_res {
                Ok(gist) => vec![Ok(Response::CreateGist(gist))],
                Err(err) => vec![Err(err)],
            }
        }
    }

    #[derive(Deserialize, Debug, Clone, PartialEq)]
    pub struct CreateTeamOptions {
        pub name: String,
        pub description: Option<String>,
        pub owner: String,
        pub maintainers: Option<Vec<String>>,
    }

    impl CreateTeamOptions {
        pub async fn run(
            &self,
            octocrab: &Octocrab,
            ctx: &Context<'_>,
        ) -> Vec<Result<Response, Error>> {
            debug!("{:?}", ctx);
            let team = match ctx.job {
                None => Ok(Response::None),
                Some(job) => {
                    let on_repositories = &job.on_repositories;
                    let repo_names: &Vec<String> = &on_repositories
                        .iter()
                        .map(|repository| repository.name.clone())
                        .collect();

                    let description = self.description.clone().unwrap_or_default();
                    let maintainers = self.maintainers.clone().unwrap_or_default();

                    let team_res = octocrab
                        .teams(&self.owner)
                        .create(&self.name)
                        .description(&description)
                        .maintainers(&maintainers)
                        .repo_names(&repo_names)
                        .send()
                        .await
                        .context(OctocrabSnafu);

                    match team_res {
                        Ok(team) => Ok(Response::CreateTeam(team)),
                        Err(err) => Err(err),
                    }
                }
            };
            vec![team]
        }
    }

    #[derive(Deserialize, Debug, Clone, PartialEq)]
    pub struct CreateIssueOptions {
        pub title: String,
        pub body: String,
        pub milestone: Option<u64>,
        pub assignees: Option<Vec<String>>,
        pub labels: Option<Vec<String>>,
    }

    impl CreateIssueOptions {
        pub async fn run(
            &self,
            octocrab: &Octocrab,
            ctx: &Context<'_>,
        ) -> Vec<Result<Response, Error>> {
            debug!("{:?}", ctx);
            match ctx.job {
                None => vec![Ok(Response::None)],
                Some(job) => {
                    let on_repositories = &job.on_repositories;
                    let statements = on_repositories.iter().map(|repository| async move {
                        let milestone = self.milestone.unwrap_or_default();
                        let assignees = self.assignees.clone().unwrap_or_default();
                        let labels = self.labels.clone().unwrap_or_default();

                        let issue = octocrab
                            .issues(&repository.owner, &repository.name)
                            .create(&self.title)
                            .body(&self.body)
                            .milestone(milestone)
                            .assignees(assignees)
                            .labels(labels)
                            .send()
                            .await
                            .context(OctocrabSnafu)?;

                        Ok(Response::CreateIssue(issue))
                    });

                    let issues: Vec<Result<Response, Error>> =
                        futures::future::join_all(statements).await;
                    issues
                }
            }
        }
    }

    #[derive(Deserialize, Debug, Clone, PartialEq)]
    pub struct CreateLabelOptions {
        pub name: String,
        pub color: String,
        pub description: String,
    }

    impl CreateLabelOptions {
        pub async fn run(
            &self,
            octocrab: &Octocrab,
            ctx: &Context<'_>,
        ) -> Vec<Result<Response, Error>> {
            debug!("{:?}", ctx);
            match ctx.job {
                None => vec![Ok(Response::None)],
                Some(job) => {
                    let on_repositories = &job.on_repositories;
                    let statements = on_repositories.iter().map(|repository| async move {
                        let label = octocrab
                            .issues(&repository.owner, &repository.name)
                            .create_label(&self.name, &self.color, &self.description)
                            .await
                            .context(OctocrabSnafu)?;
                        Ok(Response::CreateLabel(label))
                    });
                    let labels: Vec<Result<Response, Error>> =
                        futures::future::join_all(statements).await;
                    labels
                }
            }
        }
    }

    pub enum Response {
        CreateLabel(Label),
        CreateIssue(Issue),
        CreateTeam(Team),
        CreateGist(Gist),
        None,
    }
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

    async fn run_batch(&self, batch: &Batch) -> BatchResult<command::Response, Error> {
        batch.run(&self.octocrab).await
    }

    async fn run_batch_from_file(
        &self,
        filepath: impl AsRef<Path>,
    ) -> Result<BatchResult<command::Response, Error>, Error> {
        let bytes = read_file(filepath).await?;
        let batch = Batch::try_from(bytes.as_slice())?;
        Ok(self.run_batch(&batch).await)
    }
}

#[tokio::main]
async fn main() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .with_env_var("OCTOMATE_LOG")
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .with_env_filter(env_filter)
        .init();

    let options = Options::parse();

    let personal_token =
        rpassword::prompt_password("Enter your personal access token (scope: repo): ")
            .expect("You need to enter a valid personal access token");

    let octomate = Octomate::new(personal_token)
        .await
        .expect("Unable to init octocrab");

    octomate
        .run_batch_from_file(options.batch_file)
        .await
        .expect("Unable to run batch from file");
}

#[cfg(test)]
mod test {
    use super::*;
    use mockito::mock;
    use octocrab::models::Label;

    #[tokio::test]
    async fn test_create_label() {
        let url = &mockito::server_url();
        let octocrab = octocrab::Octocrab::builder()
            .personal_token("test".to_owned())
            .base_url(url)
            .unwrap()
            .build()
            .unwrap();

        let _m = mock("POST", "/repos/owner1/repo1/labels")
            .with_status(201)
            .with_body(
                r#"
                {
                    "id": 208045946,
                    "node_id": "MDU6TGFiZWwyMDgwNDU5NDY=",
                    "url": "https://api.github.com/repos/octocat/Hello-World/labels/bug",
                    "name": "bug",
                    "description": "Something isn't working",
                    "color": "f29513",
                    "default": true
                }
            "#,
            )
            .create();

        let batch = r#"
version: "1.0"
name: Test
jobs:
  - name: "Perform some basics things for some repos"
    on-repositories:
      - owner: me
        name: repo1
    steps:
      - name: Hello world!
        runs:
          - create-label:
              name: "bug"
              color: "f29513"
              description: "Something isn't working"
       "#
        .as_bytes();

        let batch = Batch::try_from(batch).unwrap();

        assert_eq!(
            batch,
            Batch {
                version: "1.0".to_owned(),
                name: Some("Test".to_owned()),
                jobs: vec![Job {
                    name: Some("Perform some basics things for some repos".to_owned()),
                    on_repositories: vec![Repository {
                        owner: "me".to_owned(),
                        name: "repo1".to_owned(),
                    }],
                    steps: vec![Step {
                        name: Some("Hello world!".to_owned()),
                        runs: vec![command::Command::CreateLabel(command::CreateLabelOptions {
                            name: "bug".to_owned(),
                            color: "f29513".to_owned(),
                            description: "Something isn't working".to_owned(),
                        })]
                    }]
                }]
            }
        );
    }
}
