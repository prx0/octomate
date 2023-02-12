use crate::error::Error;
use crate::octomate::Context;
use octocrab::models::{gists::Gist, issues::Issue, teams::Team, Label};
use octocrab::Octocrab;
use paris::info;
use serde::Deserialize;
use async_trait::async_trait;

#[async_trait]
pub trait Command<'a> {
    type Error;
    async fn run(&'a self, octocrab: &'a Octocrab, ctx: &'a Context<'a>) -> Vec<Result<Response, Self::Error>>;
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct CreateGistOptions {
    pub title: String,
    pub content: String,
    pub description: Option<String>,
    pub public: Option<bool>,
}

#[async_trait]
impl<'a> Command<'a> for CreateGistOptions {
    type Error = Error;

    async fn run(
        &'a self,
        octocrab: &'a Octocrab,
        _ctx: &'a Context<'a>,
    ) -> Vec<Result<Response, Error>> {
        let gist_res = octocrab
            .gists()
            .create()
            .file(&self.title, &self.content)
            .description(&self.description.clone().unwrap_or_default())
            .public(self.public.unwrap_or(false))
            .send()
            .await;

        match gist_res {
            Ok(gist) => vec![Ok(Response::CreateGist(gist))],
            Err(err) => vec![Err(Error::from(err))],
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
                    .await;

                match team_res {
                    Ok(team) => Ok(Response::CreateTeam(team)),
                    Err(err) => Err(Error::from(err)),
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
                        .await?;

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
        match ctx.job {
            None => vec![Ok(Response::None)],
            Some(job) => {
                let on_repositories = &job.on_repositories;
                let statements = on_repositories.iter().map(|repository| async move {
                    let label = octocrab
                        .issues(&repository.owner, &repository.name)
                        .create_label(&self.name, &self.color, &self.description)
                        .await?;
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
