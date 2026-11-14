//! Recognises Jira / GitLab / GitHub / Confluence URLs so links show as `PROJ-123`, `!412`, ….

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Jira,
    MergeRequest,
    Issue,
    PullRequest,
    Docs,
    Web,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    pub kind: LinkKind,
    /// Short name: `PAY-88`, `!412`, `#12`, page title, or host.
    pub label: String,
    /// Context: project path, repo, or host.
    pub detail: String,
}

pub fn classify(url: &str) -> LinkInfo {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let path = path.split('#').next().unwrap_or("").trim_end_matches('/');
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let info = |kind, label: String, detail: String| LinkInfo {
        kind,
        label,
        detail,
    };

    // Jira: /browse/KEY-1 or ?selectedIssue=KEY-1
    if let Some(ix) = segments.iter().position(|s| *s == "browse")
        && let Some(key) = segments.get(ix + 1)
    {
        return info(LinkKind::Jira, key.to_string(), host.to_string());
    }
    if let Some(key) = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("selectedIssue="))
    {
        return info(LinkKind::Jira, key.to_string(), host.to_string());
    }

    // GitLab: <project>/-/merge_requests/N, <project>/-/issues/N
    if let Some(dash) = segments.iter().position(|s| *s == "-") {
        let project = segments[..dash].last().copied().unwrap_or(host);
        match (segments.get(dash + 1), segments.get(dash + 2)) {
            (Some(&"merge_requests"), Some(n)) => {
                return info(LinkKind::MergeRequest, format!("!{n}"), project.to_string());
            }
            (Some(&"issues"), Some(n)) => {
                return info(LinkKind::Issue, format!("#{n}"), project.to_string());
            }
            _ => {}
        }
    }

    // GitHub: owner/repo/pull/N, owner/repo/issues/N
    if host.ends_with("github.com")
        && let [owner, repo, kind, n, ..] = segments.as_slice()
    {
        let repo = format!("{owner}/{repo}");
        match *kind {
            "pull" => return info(LinkKind::PullRequest, format!("#{n}"), repo),
            "issues" => return info(LinkKind::Issue, format!("#{n}"), repo),
            _ => {}
        }
    }

    // Confluence / docs: last path segment as a readable title.
    if segments.first() == Some(&"wiki") || host.starts_with("docs.") {
        let title = segments
            .last()
            .map(|s| s.replace(['+', '-', '_'], " "))
            .filter(|s| !s.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or_else(|| host.to_string());
        return info(LinkKind::Docs, title, host.to_string());
    }

    let host = host.trim_start_matches("www.").to_string();
    let detail = if segments.is_empty() {
        String::new()
    } else {
        format!("/{}", segments.join("/"))
    };
    info(LinkKind::Web, host, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jira() {
        let i = classify("https://acme.atlassian.net/browse/PAY-88");
        assert_eq!((i.kind, i.label.as_str()), (LinkKind::Jira, "PAY-88"));
        let i = classify(
            "https://acme.atlassian.net/jira/software/projects/PAY/boards/1?selectedIssue=PAY-9",
        );
        assert_eq!(i.label, "PAY-9");
    }

    #[test]
    fn gitlab() {
        let i = classify("https://gitlab.com/grp/sub/app/-/merge_requests/412/diffs");
        assert_eq!(i.kind, LinkKind::MergeRequest);
        assert_eq!((i.label.as_str(), i.detail.as_str()), ("!412", "app"));
        let i = classify("https://gitlab.example.com/grp/app/-/issues/7");
        assert_eq!((i.kind, i.label.as_str()), (LinkKind::Issue, "#7"));
    }

    #[test]
    fn github_and_web() {
        let i = classify("https://github.com/zed-industries/zed/pull/123");
        assert_eq!(i.kind, LinkKind::PullRequest);
        assert_eq!(i.detail, "zed-industries/zed");
        let i = classify("https://www.example.com/a/b");
        assert_eq!((i.kind, i.label.as_str()), (LinkKind::Web, "example.com"));
    }

    #[test]
    fn confluence() {
        let i = classify("https://acme.atlassian.net/wiki/spaces/ENG/pages/123/Release+Plan");
        assert_eq!((i.kind, i.label.as_str()), (LinkKind::Docs, "Release Plan"));
    }
}
