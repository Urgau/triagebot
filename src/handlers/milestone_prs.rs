use crate::{
    github::{Event, GithubClient, IssuesAction},
    handlers::Context,
};
use anyhow::Context as _;
use regex::Regex;
use reqwest::StatusCode;
use tracing as log;

pub(super) async fn handle(ctx: &Context, event: &Event) -> anyhow::Result<()> {
    let e = if let Event::Issue(e) = event {
        e
    } else {
        return Ok(());
    };

    // Only trigger on closed issues
    if e.action != IssuesAction::Closed {
        return Ok(());
    }

    let repo = e.issue.repository();
    if !(repo.organization == "rust-lang" && repo.repository == "rust") {
        return Ok(());
    }

    if !e.issue.merged {
        log::trace!(
            "Ignoring closing of rust-lang/rust#{}: not merged",
            e.issue.number
        );
        return Ok(());
    }

    let merge_sha = if let Some(s) = &e.issue.merge_commit_sha {
        s
    } else {
        log::error!(
            "rust-lang/rust#{}: no merge_commit_sha in event",
            e.issue.number
        );
        return Ok(());
    };

    // Fetch the version from the upstream repository.
    let version = if let Some(version) = get_version_standalone(&ctx.github, merge_sha).await? {
        version
    } else {
        log::error!("could not find the version of {:?}", merge_sha);
        return Ok(());
    };

    if !version.starts_with("1.") && version.len() < 8 {
        log::error!("Weird version {:?} for {:?}", version, merge_sha);
        return Ok(());
    }

    // Associate this merged PR with the version it merged into.
    //
    // Note that this should work for rollup-merged PRs too. It will *not*
    // auto-update when merging a beta-backport, for example, but that seems
    // fine; we can manually update without too much trouble in that case, and
    // eventually automate it separately.
    e.issue.set_milestone(&ctx.github, &version).await?;

    let files = e.issue.diff(&ctx.github).await?;
    if let Some(files) = files {
        if let Some(cargo) = files.iter().find(|fd| fd.filename == "src/tools/cargo") {
            // The webhook timeout of 10 seconds can be too short, so process in
            // the background.
            let diff = cargo.patch.clone();
            tokio::task::spawn(async move {
                let gh = GithubClient::new_from_env();
                if let Err(e) = milestone_cargo(&gh, &version, &diff).await {
                    log::error!("failed to milestone cargo: {e:?}");
                }
            });
        }
    }

    Ok(())
}

async fn get_version_standalone(
    gh: &GithubClient,
    merge_sha: &str,
) -> anyhow::Result<Option<String>> {
    let resp = gh
        .raw()
        .get(&format!(
            "https://raw.githubusercontent.com/rust-lang/rust/{}/src/version",
            merge_sha
        ))
        .send()
        .await
        .with_context(|| format!("retrieving src/version for {}", merge_sha))?;

    match resp.status() {
        StatusCode::OK => {}
        // Don't treat a 404 as a failure, we'll try another way to retrieve the version.
        StatusCode::NOT_FOUND => return Ok(None),
        status => anyhow::bail!(
            "unexpected status code {} while retrieving src/version for {}",
            status,
            merge_sha
        ),
    }

    Ok(Some(
        resp.text()
            .await
            .with_context(|| format!("deserializing src/version for {}", merge_sha))?
            .trim()
            .to_string(),
    ))
}

/// Milestones all PRs in the cargo repo when the submodule is synced in
/// rust-lang/rust.
async fn milestone_cargo(
    gh: &GithubClient,
    release_version: &str,
    submodule_diff: &str,
) -> anyhow::Result<()> {
    // Determine the start/end range of commits in this submodule update by
    // looking at the diff content which indicates the old and new hash.
    let subproject_re = Regex::new("Subproject commit ([0-9a-f]+)").unwrap();
    let mut caps = subproject_re.captures_iter(submodule_diff);
    let cargo_start_hash = &caps.next().unwrap()[1];
    let cargo_end_hash = &caps.next().unwrap()[1];
    assert!(caps.next().is_none());

    // Get all of the git commits in the cargo repo.
    let cargo_repo = gh.repository("rust-lang/cargo").await?;
    log::info!("loading cargo changes {cargo_start_hash}...{cargo_end_hash}");
    let commits = cargo_repo
        .github_commits_in_range(gh, cargo_start_hash, cargo_end_hash)
        .await?;

    // For each commit, look for a message from bors that indicates which
    // PR was merged.
    //
    // GitHub has a specific API for this at
    // /repos/{owner}/{repo}/commits/{commit_sha}/pulls
    // <https://docs.github.com/en/rest/commits/commits?apiVersion=2022-11-28#list-pull-requests-associated-with-a-commit>,
    // but it is a little awkward to use, only works on the default branch,
    // and this is a bit simpler/faster. However, it is sensitive to the
    // specific messages generated by bors or GitHub merge queue, and won't
    // catch things merged beyond them.
    let merge_re =
        Regex::new(r"(?:Auto merge of|Merge pull request) #([0-9]+)|\(#([0-9]+)\)$").unwrap();

    let pr_nums = commits
        .iter()
        .filter(|commit|
            // Assumptions:
            // * A merge commit always has two parent commits.
            // * Cargo's PR never got merged as fast-forward / rebase / squash merge.
            commit.parents.len() == 2)
        .filter_map(|commit| {
            let first = commit.commit.message.lines().next().unwrap_or_default();
            merge_re.captures(first).map(|cap| {
                cap.get(1)
                    .or_else(|| cap.get(2))
                    .unwrap()
                    .as_str()
                    .parse::<u64>()
                    .expect("digits only")
            })
        });
    let milestone = cargo_repo
        .get_or_create_milestone(gh, release_version, "closed")
        .await?;
    for pr_num in pr_nums {
        log::info!("setting cargo milestone {milestone:?} for {pr_num}");
        cargo_repo.set_milestone(gh, &milestone, pr_num).await?;
    }

    Ok(())
}
