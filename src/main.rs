#![allow(clippy::new_without_default)]

use anyhow::Context as _;
use axum::body::Body;
use axum::error_handling::HandleErrorLayer;
use axum::http::HeaderName;
use axum::response::{Html, Response};
use axum::routing::{get, post};
use axum::{BoxError, Router};
use hyper::{Request, StatusCode};
use std::time::Duration;
use std::{env, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tokio::{task, time};
use tower::ServiceBuilder;
use tower::buffer::BufferLayer;
use tower::limit::RateLimitLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{self as log, info_span};
use triagebot::gha_logs::GitHubActionLogsCache;
use triagebot::handlers::Context;
use triagebot::handlers::pr_tracking::ReviewerWorkqueue;
use triagebot::handlers::pr_tracking::load_workqueue;
use triagebot::jobs::{
    JOB_PROCESSING_CADENCE_IN_SECS, JOB_SCHEDULING_CADENCE_IN_SECS, default_jobs,
};
use triagebot::team_data::TeamClient;
use triagebot::zulip::client::ZulipClient;
use triagebot::{db, github};

async fn run_server(addr: SocketAddr) -> anyhow::Result<()> {
    let gh = github::GithubClient::new_from_env();
    let zulip = ZulipClient::new_from_env();
    let team_api = TeamClient::new_from_env();
    let oc = octocrab::OctocrabBuilder::new()
        .personal_token(github::default_token_from_env())
        .build()
        .expect("Failed to build octocrab.");

    // Loading the workqueue takes ~10-15s, and it's annoying for local rebuilds.
    // Allow users to opt out of it.
    let skip_loading_workqueue = env::var("SKIP_WORKQUEUE")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    // Load the initial workqueue state from GitHub
    // In case this fails, we do not want to block triagebot, instead
    // we use an empty workqueue and let it be updated later through
    // webhooks and the `PullRequestAssignmentUpdate` cron job.
    let workqueue = if skip_loading_workqueue {
        tracing::warn!("Skipping workqueue loading");
        ReviewerWorkqueue::default()
    } else {
        tracing::info!("Loading reviewer workqueue for rust-lang/rust");
        let workqueue =
            match tokio::time::timeout(Duration::from_secs(60), load_workqueue(&oc)).await {
                Ok(Ok(workqueue)) => workqueue,
                Ok(Err(error)) => {
                    tracing::error!("Cannot load initial workqueue: {error:?}");
                    ReviewerWorkqueue::default()
                }
                Err(_) => {
                    tracing::error!("Cannot load initial workqueue, timeouted after a minute");
                    ReviewerWorkqueue::default()
                }
            };
        tracing::info!("Workqueue loaded");
        workqueue
    };

    // Only run the migrations after the workqueue has been loaded, immediately
    // before starting the HTTP server.
    // On AWS ECS, triagebot shortly runs in two instances at once.
    // We thus want to minimize the time where migrations have been executed
    // and the old instance potentially runs on an newer database schema.
    let db_url = std::env::var("DATABASE_URL").expect("needs DATABASE_URL");
    let pool = db::ClientPool::new(db_url.clone());
    if !std::env::var("SKIP_DB_MIGRATIONS").is_ok_and(|value| value == "1") {
        db::run_migrations(&mut *pool.get().await)
            .await
            .context("database migrations")?;
    }

    let ctx = Arc::new(Context {
        username: std::env::var("TRIAGEBOT_USERNAME").or_else(|err| match err {
            std::env::VarError::NotPresent => Ok("rustbot".to_owned()),
            err => Err(err),
        })?,
        db: pool,
        github: gh,
        team: team_api,
        octocrab: oc,
        workqueue: Arc::new(RwLock::new(workqueue)),
        gha_logs: Arc::new(RwLock::new(GitHubActionLogsCache::default())),
        zulip,
    });

    // Run all jobs that have a schedule (recurring jobs)
    if !is_scheduled_jobs_disabled() {
        spawn_job_scheduler(db_url);
        spawn_job_runner(ctx.clone());
    }

    const REQUEST_ID_HEADER: &str = "x-request-id";
    const X_REQUEST_ID: HeaderName = HeaderName::from_static(REQUEST_ID_HEADER);

    let middleware = ServiceBuilder::new()
        .layer(SetRequestIdLayer::new(
            X_REQUEST_ID.clone(),
            MakeRequestUuid,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    // Log the request id as generated.
                    let request_id = request.headers().get(REQUEST_ID_HEADER);

                    if let Some(request_id) = request_id {
                        info_span!(
                            "request",
                            request_id = ?request_id,
                        )
                    } else {
                        tracing::error!("could not extract request_id");
                        info_span!("request")
                    }
                })
                .on_request(|request: &Request<Body>, _span: &tracing::Span| {
                    tracing::info!(?request);
                })
                .on_response(|response: &Response<_>, dur, _span: &tracing::Span| {
                    tracing::info!("response={} in {dur:?}", response.status());
                }),
        )
        .layer(PropagateRequestIdLayer::new(X_REQUEST_ID))
        .layer(CompressionLayer::new())
        .layer(CatchPanicLayer::new());

    let agenda = Router::new()
        .route("/", get(|| async { Html(triagebot::agenda::INDEX) }))
        .route("/lang/triage", get(triagebot::agenda::lang_http))
        .route("/lang/planning", get(triagebot::agenda::lang_planning_http))
        .route(
            "/types/planning",
            get(triagebot::agenda::types_planning_http),
        )
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|err: BoxError| async move {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Unhandled error: {err}"),
                    )
                }))
                .layer(BufferLayer::new(5))
                .layer(RateLimitLayer::new(2, Duration::from_secs(60))),
        );

    let app = Router::new()
        .route("/", get(|| async { "Triagebot is awaiting triage." }))
        .route(
            "/robots.txt",
            get(|| async { "User-Agent: *\nDisallow: /\n" }),
        )
        .route("/triage", get(triagebot::triage::index))
        .route("/triage/{owner}/{repo}", get(triagebot::triage::pulls))
        .route(
            triagebot::gha_logs::ANSI_UP_URL,
            get(triagebot::gha_logs::ansi_up_min_js),
        )
        .route(
            triagebot::gha_logs::SUCCESS_URL,
            get(triagebot::gha_logs::success_svg),
        )
        .route(
            triagebot::gha_logs::FAILURE_URL,
            get(triagebot::gha_logs::failure_svg),
        )
        .route(
            "/gha-logs/{owner}/{repo}/{log-id}",
            get(triagebot::gha_logs::gha_logs),
        )
        .route(
            "/gh-range-diff/{owner}/{repo}/{basehead}",
            get(triagebot::gh_range_diff::gh_range_diff),
        )
        .route(
            "/gh-range-diff/{owner}/{repo}/{oldbasehead}/{newbasehead}",
            get(triagebot::gh_range_diff::gh_ranges_diff),
        )
        .route(
            "/gh-changes-since/{owner}/{repo}/{pr}/{oldbasehead}",
            get(triagebot::gh_changes_since::gh_changes_since),
        )
        .nest("/agenda", agenda)
        .route("/bors-commit-list", get(triagebot::bors::bors_commit_list))
        .route(
            "/notifications",
            get(triagebot::notification_listing::notifications),
        )
        .route("/zulip-hook", post(triagebot::zulip::webhook))
        .route("/github-hook", post(triagebot::github::webhook))
        .layer(middleware)
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    log::info!("Listening on http://{}", addr);

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

/// Spawns a background tokio task which runs continuously to queue up jobs
/// to be run by the job runner.
///
/// The scheduler wakes up every `JOB_SCHEDULING_CADENCE_IN_SECS` seconds to
/// check if there are any jobs ready to run. Jobs get inserted into the the
/// database which acts as a queue.
fn spawn_job_scheduler(db_url: String) {
    task::spawn(async move {
        loop {
            let db_url = db_url.clone();
            let res = task::spawn(async move {
                let pool = db::ClientPool::new(db_url);
                let mut interval =
                    time::interval(time::Duration::from_secs(JOB_SCHEDULING_CADENCE_IN_SECS));

                loop {
                    interval.tick().await;
                    db::schedule_jobs(&*pool.get().await, default_jobs())
                        .await
                        .context("database schedule jobs")
                        .unwrap();
                }
            });

            match res.await {
                Err(err) if err.is_panic() => {
                    /* handle panic in above task, re-launching */
                    tracing::error!("schedule_jobs task died (error={err})");
                    tokio::time::sleep(std::time::Duration::new(5, 0)).await;
                }
                _ => unreachable!(),
            }
        }
    });
}

/// Spawns a background tokio task which runs continuously to run scheduled
/// jobs.
///
/// The runner wakes up every `JOB_PROCESSING_CADENCE_IN_SECS` seconds to
/// check if any jobs have been put into the queue by the scheduler. They
/// will get popped off the queue and run if any are found.
fn spawn_job_runner(ctx: Arc<Context>) {
    task::spawn(async move {
        loop {
            let ctx = ctx.clone();
            let res = task::spawn(async move {
                let mut interval =
                    time::interval(time::Duration::from_secs(JOB_PROCESSING_CADENCE_IN_SECS));

                loop {
                    interval.tick().await;
                    db::run_scheduled_jobs(&ctx)
                        .await
                        .context("run database scheduled jobs")
                        .unwrap();
                }
            });

            match res.await {
                Err(err) if err.is_panic() => {
                    /* handle panic in above task, re-launching */
                    tracing::error!("run_scheduled_jobs task died (error={err})");
                    tokio::time::sleep(std::time::Duration::new(5, 0)).await;
                }
                _ => unreachable!(),
            }
        }
    });
}

/// Determines whether or not background scheduled jobs should be disabled for
/// the purpose of testing.
///
/// This helps avoid having random jobs run while testing other things.
fn is_scheduled_jobs_disabled() -> bool {
    env::var_os("TRIAGEBOT_TEST_DISABLE_JOBS").is_some()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::Subscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(std::env::var_os("DISABLE_COLOR").is_none())
        .try_init()
        .unwrap();

    let port = env::var("PORT")
        .ok()
        .map(|p| p.parse::<u16>().expect("parsed PORT"))
        .unwrap_or(8000);
    let addr = ([0, 0, 0, 0], port).into();
    run_server(addr).await.context("Failed to run the server")?;
    Ok(())
}
