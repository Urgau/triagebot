use crate::{
    github::{Event, IssuesAction, IssuesEvent},
    handlers::Context,
    interactions::EditIssueBody,
};

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
struct RenderedLinkData {
    rendered_link: String,
}

pub async fn handle(ctx: &Context, event: &Event) -> anyhow::Result<()> {
    let e = if let Event::Issue(e) = event {
        e
    } else {
        return Ok(());
    };

    if !e.issue.is_pr() {
        return Ok(());
    }

    let repo = e.issue.repository();
    let prefix = match (&*repo.organization, &*repo.repository) {
        ("rust-lang", "rfcs") => "text/",
        ("rust-lang", "blog.rust-lang.org") => "posts/",
        _ => return Ok(()),
    };

    if let Err(e) = add_rendered_link(&ctx, &e, prefix).await {
        tracing::error!("Error adding rendered link: {:?}", e);
    }

    Ok(())
}

async fn add_rendered_link(ctx: &Context, e: &IssuesEvent, prefix: &str) -> anyhow::Result<()> {
    if e.action == IssuesAction::Opened
        || e.action == IssuesAction::Closed
        || e.action == IssuesAction::Reopened
    {
        let edit = EditIssueBody::new(&e.issue, "RENDERED_LINK");
        let current_data: Option<RenderedLinkData> = edit.current_data();

        let files = e.issue.files(&ctx.github).await?;

        if let Some(file) = files.iter().find(|f| f.filename.starts_with(prefix)) {
            let mut current_data = current_data.unwrap_or_default();
            let head = e.issue.head.as_ref().unwrap();

            // This URL should be stable while the PR is open, even if the
            // user pushes new commits.
            //
            // It will go away if the user deletes their branch, or if
            // they reset it (such as if they created a PR from master).
            // That should usually only happen after the PR is closed
            // a which point we switch to a SHA-based url.
            current_data.rendered_link = format!(
                "https://github.com/{}/blob/{}/{}",
                head.repo.full_name,
                if e.action == IssuesAction::Closed {
                    &head.sha
                } else {
                    &head.git_ref
                },
                file.filename
            );

            edit.apply(
                &ctx.github,
                format!("[Rendered]({})", &current_data.rendered_link),
                current_data,
            )
            .await?;
        } else if let Some(mut current_data) = current_data {
            // No render link to show, but one previously, so remove it
            current_data.rendered_link = String::new();
            edit.apply(&ctx.github, String::new(), current_data).await?;
        }
    }

    Ok(())
}
