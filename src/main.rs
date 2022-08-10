pub mod command;
pub mod error;
pub mod io;
pub mod octomate;
pub mod options;

use crate::options::Options;
use paris::Logger;

#[tokio::main]
async fn main() {
    let options = Options::from_cli();
    let mut logger = Logger::new();

    let personal_token =
        rpassword::prompt_password("Enter your personal access token (scope: repo): ")
            .expect("You need to enter a valid personal access token");

    logger.loading("Authenticate to github in progress");
    let octomate = octomate::Octomate::new(personal_token)
        .await
        .expect("Unable to init octocrab");
    logger
        .done()
        .success("Authenticated successfully to github");

    logger.loading(format!("Read batch file {:?}", options.batch_file));
    octomate
        .run_batch_from_file(options.batch_file)
        .await
        .expect("Unable to run batch from file");
    logger.done().success("Batch processing terminated");
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

        let batch = octomate::Batch::try_from(batch).unwrap();

        assert_eq!(
            batch,
            octomate::Batch {
                version: "1.0".to_owned(),
                name: Some("Test".to_owned()),
                jobs: vec![octomate::Job {
                    name: Some("Perform some basics things for some repos".to_owned()),
                    on_repositories: vec![octomate::Repository {
                        owner: "me".to_owned(),
                        name: "repo1".to_owned(),
                    }],
                    steps: vec![octomate::Step {
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
