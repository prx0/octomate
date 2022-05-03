# Octomate

> Automate things using the Github API, run batch of tasks to manage Github related things.

Example of batch file:

```yml
version: "1.0"

name: My first batch # Optional

jobs:
  - name: "Perform some basics things for some repos" # Optional field
    on-repositories:
      - owner: me
        name: repo1
      - owner: me
        name: repo2

    steps:
      - name: Init labels and create a team # Optional field
        runs:
          - create-label:
              name: test # Optional field
              color: "000000"
              description: "label created by octomate!" # Optional field

          - create-team:
              name: Avengers # Optional field
              description: "My super team of heroes" # Optional field
              owner: me
              maintainers:
                - thor
                - ironman
                - captain
```

## Batch specifications

[version 1.0](specs/1.0.md)

## Commands

### create-issue

```yml
          - create-issue:
              title: "A title for my gist"
              body: >
                Description of the issue,
                blablabla
              milestone: 1 # number, id of the milestone, optional field
              assignees: # List of string, optional field
                - "John doe"
              labels: # List of string, optional field
                - "my-label" # name of the label
```

### create-gist

```yml
          - create-gist:
              title: "A title for my gist"
              content: >
                A content for my gist,
                blablabla
              description: "Example of gist" # Optional field
              public: true # Optional field
```

### create-team

```yml
          - create-team:
              name: My Team
              description: "A description for the team" # Optional field
              owner: myusername # in string
              maintainers: # a list of string, optional field
                - thor
                - ironman
                - captain
```

### create-label

```yml
          - create-label:
              name: "my label"
              color: "000000" # hex color code without the # sign
              description: "A description"
```
