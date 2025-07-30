use anyhow::anyhow;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Notify;
use url::Url;
use uuid::Uuid;

use crate::{
    api::TaskOutcome,
    config::LLMRouterTable,
    context::{self, Context},
    runtime::ContainerConfig,
};

const AGENT_CONTAINER_IMAGE: &str = "ghcr.io/autominion/default-minion:x86-64-latest";

pub async fn run<P: AsRef<Path>>(
    llm_router_table: LLMRouterTable,
    containerfile: &Option<P>,
    nested: bool,
    path: &P,
    task_description: String,
) -> anyhow::Result<()> {
    let rt = crate::runtime::LocalDockerRuntime::connect()?;
    let agent_api_host = rt.bridge_network_ip().await?;
    let listener = crate::util::listen_to_free_port(&agent_api_host);
    let agent_api_port = listener.local_addr().unwrap().port();
    let git_repo_url = Url::parse(&format!(
        "http://host.docker.internal:{agent_api_port}/api/agent/git"
    ))
    .expect("Failed to parse URL");
    let minion_api_base_url = format!("http://host.docker.internal:{agent_api_port}/api/");
    let fork_branch = Uuid::now_v7().to_string();
    let agent_api_key = context::random_key();
    let host_address = format!("http://{agent_api_host}:{agent_api_port}");

    let base_branch = current_branch_name(path)?;

    create_git_branch(path, &fork_branch)?;

    let ctx = Context {
        llm_router_table,
        agent_api_key: agent_api_key.clone(),
        task_description,
        git_user_name: "minion[bot]".to_owned(),
        git_user_email: "minion@localhost".to_owned(),
        git_repo_url,
        git_branch: fork_branch.clone(),
        git_repo_path: path.as_ref().to_path_buf(),
    };

    let image = if let Some(containerfile) = containerfile {
        rt.build_container_image(containerfile).await?
    } else {
        rt.pull_container_image(AGENT_CONTAINER_IMAGE).await?;
        AGENT_CONTAINER_IMAGE.to_owned()
    };
    let stop_notify = Arc::new(Notify::new());
    let agent_base_url = host_address.clone();

    let inquiry_handle = tokio::spawn(handle_inquiries(
        agent_base_url.clone(),
        agent_api_key.clone(),
        stop_notify.clone(),
    ));
    let container_config = ContainerConfig {
        image,
        env_vars: vec![
            ("MINION_API_BASE_URL".to_owned(), minion_api_base_url),
            ("MINION_API_TOKEN".to_owned(), agent_api_key),
        ],
        nested,
    };

    let server = tokio::spawn(crate::api::run_server(listener, ctx));
    // Wait for the server to be ready by polling the /ready endpoint
    crate::api::wait_until_ready(&host_address).await?;

    let (task_outcome, _) = tokio::try_join!(
        async {
            server
                .await
                .map_err(|e| anyhow!(e))?
                .map_err(|e| anyhow!(e))
        },
        async {
            rt.run_container(container_config)
                .await
                .map_err(|e| anyhow!(e))
        }
    )?;

    if task_outcome == TaskOutcome::Failure {
        stop_notify.notify_one();
        let _ = inquiry_handle.await;
        return Ok(());
    }

    squash_merge_branch(path, &base_branch, &fork_branch)?;
    stop_notify.notify_one();
    let _ = inquiry_handle.await;
    Ok(())
}

async fn handle_inquiries(agent_base_url: String, agent_api_key: String, stop_notify: Arc<Notify>) {
    let client = reqwest::Client::new();

    loop {
        // Check stop signal
        let stop_future = stop_notify.notified();
        let sleep_future = tokio::time::sleep(std::time::Duration::from_secs(1));
        tokio::select! {
            _ = stop_future => {
                println!("[handle_inquiries] Received stop_notify. Shutting down handler!");
                break;
            }
            _ = sleep_future => {
                let url = format!("{agent_base_url}/api/agent/inquiry_request");
                let resp_result = client.get(&url).bearer_auth(&agent_api_key).send().await;
                match resp_result {
                    Ok(resp) => {
                        let text = resp.text().await.unwrap_or_else(|e| {
                            println!("[handle_inquiries] ERROR reading response text: {e}");
                            "".to_owned()
                        });
                        let question = text.trim().to_string();

                        if !question.is_empty() {
                            // Fancy ORCA inquiry request
                            let width = term_size::dimensions().map(|(w, _)| w).unwrap_or(80);

                            let banner = "🐋=== ORCA WANTS INPUT ===🐋";
                            let question_banner = "🐋 ORCA is asking:".to_string();
                            let separator = "─".repeat(width);

                            // Helper to center text
                            fn center(text: &str, width: usize) -> String {
                                let pad = width.saturating_sub(text.len()) / 2;
                                format!("{:pad$}{}", "", text, pad = pad)
                            }

                            println!("\n\n{separator}");
                            println!("{}", center(banner, width));
                            println!("{}", center(&question_banner, width));
                            println!("{}", center(&question, width));
                            println!("{separator}");
                            println!();

                            // Read Blocking
                            let answer = tokio::task::spawn_blocking(|| {
                                use std::io::{self, Write};
                                print!("Your answer: ");
                                io::stdout().flush().unwrap();

                                let mut input = String::new();
                                match io::stdin().read_line(&mut input) {
                                    Ok(_bytes) => {
                                        input
                                    }
                                    Err(e) => {
                                        println!("[handle_inquiries] ERROR reading stdin: {e}");
                                        String::new()
                                    }
                                }
                            }).await.unwrap();

                            println!(" User entered: {answer:?}");

                            let post_url = format!("{agent_base_url}/api/agent/inquiry_response");
                            println!("[handle_inquiries] POSTing answer to: {post_url}");
                            let post_resp = client
                                .post(&post_url)
                                .bearer_auth(&agent_api_key)
                                .json(&answer)
                                .send()
                                .await;

                            match post_resp {
                                Ok(r) => println!("[handle_inquiries] POST status: {}", r.status()),
                                Err(e) => println!("[handle_inquiries] ERROR posting answer: {e}"),
                            }
                        }
                    }
                    Err(e) => {
                        println!("[handle_inquiries] ERROR sending GET: {e}");
                    }
                }
            }
        }
    }
    println!("[handle_inquiries] Handler EXITED.");
}

/// Create a new git branch from the current HEAD.
fn create_git_branch<P: AsRef<Path>>(path: P, branch_name: &str) -> anyhow::Result<()> {
    let repo = git2::Repository::open(path)?;

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    repo.branch(branch_name, &commit, false)?;

    Ok(())
}

/// Add the changes from the fork branch to the base branch, leaving the changes
/// unstaged on the base branch.
fn squash_merge_branch<P: AsRef<Path>>(path: P, base: &str, fork: &str) -> anyhow::Result<()> {
    let repo = git2::Repository::open(path)?;

    // Ensure the working directory is clean.
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(false);
    let statuses = repo.statuses(Some(&mut status_opts))?;
    if statuses.iter().any(|entry| {
        let s = entry.status();
        s.contains(git2::Status::WT_NEW)
            || s.contains(git2::Status::WT_MODIFIED)
            || s.contains(git2::Status::WT_DELETED)
            || s.contains(git2::Status::WT_RENAMED)
            || s.contains(git2::Status::WT_TYPECHANGE)
    }) {
        return Err(anyhow!("Working directory has unstaged changes; aborting."));
    }

    // Verify the current branch is the base branch. If not, check it out.
    let head = repo.head()?;
    let head_name = head
        .shorthand()
        .ok_or_else(|| anyhow!("Cannot determine current branch name"))?;
    if head_name != base {
        repo.set_head(&format!("refs/heads/{base}"))?;
        repo.checkout_head(None)?;
    }

    let head = repo.head()?;
    let base_commit = head.peel_to_commit()?;

    let fork_branch = repo.find_branch(fork, git2::BranchType::Local)?;
    let fork_commit = fork_branch.get().peel_to_commit()?;

    // Compute the merge base between the base and fork commits.
    let merge_base_oid = repo.merge_base(base_commit.id(), fork_commit.id())?;
    let merge_base_commit = repo.find_commit(merge_base_oid)?;
    let base_tree = merge_base_commit.tree()?;

    // Compute the diff from the merge base to the fork commit.
    let fork_tree = fork_commit.tree()?;
    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&fork_tree), None)?;

    // Apply the diff to the working directory (squash merge),
    // leaving the changes unstaged on the base branch.
    let mut apply_opts = git2::ApplyOptions::new();
    repo.apply(&diff, git2::ApplyLocation::WorkDir, Some(&mut apply_opts))
        .map_err(|_| anyhow!("Merge conflict encountered; aborting."))?;

    Ok(())
}

fn current_branch_name<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let repo = git2::Repository::open(path)?;

    let head = repo.head()?;
    if !head.is_branch() {
        return Err(anyhow!("HEAD is not pointing to a branch"));
    }
    let branch_name = head
        .shorthand()
        .ok_or_else(|| anyhow!("Cannot determine current branch name"))?;
    Ok(branch_name.to_string())
}
