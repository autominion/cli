use std::path::Path;

use url::Url;
use uuid::Uuid;

use crate::{
    context::{self, Context},
    runtime::ContainerConfig,
};

const AGENT_CONTAINER_IMAGE: &str = "ghcr.io/autominion/minion:x86-64-latest";
const TASK_DESCRIPTION: &str =
    "This is a test task. Don't do anything else, just use the end-task action immediately to mark the task as complete";

pub async fn run<P: AsRef<Path>>(openrouter_key: String, path: &P) -> anyhow::Result<()> {
    let rt = crate::runtime::LocalDockerRuntime::connect()?;
    let agent_api_host = rt.bridge_network_ip().await?;
    let listener = crate::util::listen_to_free_port(&agent_api_host);
    let agent_api_port = listener.local_addr().unwrap().port();
    let git_repo_url = Url::parse(&format!(
        "http://host.docker.internal:{}/api/agent/git",
        agent_api_port
    ))
    .expect("Failed to parse URL");
    let minion_api_base_url = format!("http://host.docker.internal:{}/api/", agent_api_port);
    let git_branch = Uuid::now_v7().to_string();
    let agent_api_key = context::random_key();
    let host_address = format!("http://{}:{}", agent_api_host, agent_api_port);

    create_git_branch(path, &git_branch)?;

    let ctx = Context {
        openrouter_key,
        agent_api_key: agent_api_key.clone(),
        task_description: TASK_DESCRIPTION.to_owned(),
        git_user_name: "minion[bot]".to_owned(),
        git_user_email: "minion@localhost".to_owned(),
        git_repo_url,
        git_branch,
        git_repo_path: path.as_ref().to_path_buf(),
    };

    let container_config = ContainerConfig {
        image: AGENT_CONTAINER_IMAGE.to_owned(),
        env_vars: vec![
            ("MINION_API_BASE_URL".to_owned(), minion_api_base_url),
            ("MINION_API_TOKEN".to_owned(), agent_api_key),
        ],
    };

    rt.pull_container_image(&container_config.image).await?;

    let server = tokio::spawn(crate::api::run_server(listener, ctx));

    // Wait for the server to be ready by polling the /ready endpoint
    crate::api::wait_until_ready(&host_address).await?;

    tokio::select! {
        res = server => {
            res.map_err(|e| anyhow::anyhow!(e))?.map_err(|e| anyhow::anyhow!(e))
        }
        res = rt.run_container(container_config) => {
            res.map_err(|e| anyhow::anyhow!(e))
        }
    }
}

/// Create a new git branch from the current HEAD.
fn create_git_branch<P: AsRef<Path>>(path: P, branch_name: &str) -> anyhow::Result<()> {
    let repo = git2::Repository::open(path)?;

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    repo.branch(branch_name, &commit, false)?;

    Ok(())
}
