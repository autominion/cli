use actix_web::Scope;
use actix_web::{get, post, web, HttpResponse};
use tokio::sync::{oneshot, Mutex};

use agent_api::types::task::*;

use crate::context::Context;

pub fn scope() -> Scope {
    Scope::new("/agent")
        .service(task_info)
        .service(task_complete)
        .service(task_fail)
}

#[get("/task")]
pub async fn task_info(ctx: web::Data<Context>) -> HttpResponse {
    let response = Task {
        status: TaskStatus::Running,
        description: ctx.task_description.clone(),
        git_user_name: ctx.git_user_name.clone(),
        git_user_email: ctx.git_user_email.clone(),
        git_repo_url: ctx.git_repo_url.clone(),
        git_branch: ctx.git_branch.clone(),
    };

    HttpResponse::Ok().json(response)
}

#[post("/task/complete")]
pub async fn task_complete(
    body: web::Json<TaskComplete>,
    shutdown_tx: web::Data<Mutex<Option<oneshot::Sender<()>>>>,
) -> HttpResponse {
    let body = body.into_inner();
    println!("Task completed");
    println!("{}", body.description);

    let tx = shutdown_tx
        .lock()
        .await
        .take()
        .expect("Failed to acquire lock for shutdown signal");
    tx.send(()).expect("Failed to send shutdown signal");

    HttpResponse::Ok().finish()
}

#[post("/task/fail")]
pub async fn task_fail(
    body: web::Json<TaskFailure>,
    shutdown_tx: web::Data<Mutex<Option<oneshot::Sender<()>>>>,
) -> HttpResponse {
    println!("Task failed");
    println!("{}", body.description);

    let tx = shutdown_tx
        .lock()
        .await
        .take()
        .expect("Failed to acquire lock for shutdown signal");
    tx.send(()).expect("Failed to send shutdown signal");

    HttpResponse::Ok().finish()
}
