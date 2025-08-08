use actix_web::Scope;
use actix_web::{get, post, web, HttpResponse};
use tokio::sync::{oneshot, Mutex};

use agent_api::types::task::*;
use serde::Deserialize;

use crate::api::TaskOutcome;
use crate::context::Context;

pub struct Inquiry {
    pub sender: oneshot::Sender<String>,
    pub question: String,
}

pub struct InquiryState {
    pub pending: Mutex<Option<Inquiry>>,
}
#[derive(Deserialize)]
pub struct InquiryPayload {
    pub inquiry: String,
}

pub fn scope() -> Scope {
    Scope::new("/agent")
        .service(task_info)
        .service(task_complete)
        .service(task_fail)
        .service(inquiry)
        .service(get_inquiry)
        .service(inquiry_response)
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
    shutdown_tx: web::Data<Mutex<Option<oneshot::Sender<TaskOutcome>>>>,
) -> HttpResponse {
    let body = body.into_inner();
    println!("Task completed");
    println!("{}", body.description);

    let tx = shutdown_tx
        .lock()
        .await
        .take()
        .expect("Failed to acquire lock for shutdown signal");
    tx.send(TaskOutcome::Completed)
        .expect("Failed to send shutdown signal");

    HttpResponse::Ok().finish()
}

#[post("/task/fail")]
pub async fn task_fail(
    body: web::Json<TaskFailure>,
    shutdown_tx: web::Data<Mutex<Option<oneshot::Sender<TaskOutcome>>>>,
) -> HttpResponse {
    println!("Task failed");
    println!("{}", body.description);

    let tx = shutdown_tx
        .lock()
        .await
        .take()
        .expect("Failed to acquire lock for shutdown signal");
    tx.send(TaskOutcome::Failure)
        .expect("Failed to send shutdown signal");

    HttpResponse::Ok().finish()
}
/// Send an inquiry to the user and await its answer.
/// Agents use this endpoint to request clarification on their tasks.
#[post("/inquiry")]
pub async fn inquiry(
    request: web::Json<InquiryPayload>,
    inquiry_state: web::Data<InquiryState>,
) -> HttpResponse {
    let (tx, rx) = oneshot::channel();
    {
        let mut guard = inquiry_state.pending.lock().await;
        *guard = Some(Inquiry {
            sender: tx,
            question: request.inquiry.clone(),
        });
    }
    match rx.await {
        Ok(answer) => HttpResponse::Ok().json(answer),
        Err(_) => HttpResponse::InternalServerError().body("No answer received"),
    }
}

/// This endpoint lets the CLI check if there is a pending inquiry from the agent.
/// If there is a question it returns it as a string in the response body.
/// If there is no question it returns an empty string.
/// CLI is constantly checking
#[get("/inquiry_request")]
pub async fn get_inquiry(inquiry_state: web::Data<InquiryState>) -> HttpResponse {
    let guard = inquiry_state.pending.lock().await;
    if let Some(ref pending_inquiry) = *guard {
        HttpResponse::Ok().body(pending_inquiry.question.clone())
    } else {
        HttpResponse::Ok().body("")
    }
}

/// This endpoint lets the CLI provide an answer to the pending inquiry.
/// It takes a string as input and delivers it to the waiting agent (via the stored oneshot sender).
/// If there is no pending inquiry, it returns a BadRequest.
/// Once there is an answer its send back
#[post("/inquiry_response")]
pub async fn inquiry_response(
    answer: web::Json<String>,
    inquiry_state: web::Data<InquiryState>,
) -> HttpResponse {
    let maybe_inquiry = {
        let mut guard = inquiry_state.pending.lock().await;
        guard.take()
    };
    if let Some(pending_inquiry) = maybe_inquiry {
        let _ = pending_inquiry.sender.send(answer.into_inner());
        HttpResponse::Ok().body("OK")
    } else {
        HttpResponse::BadRequest().body("No pending inquiry")
    }
}
