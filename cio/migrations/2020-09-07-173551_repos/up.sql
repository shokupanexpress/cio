 CREATE TABLE repos (
    id SERIAL PRIMARY KEY,
    github_id INTEGER NOT NULL UNIQUE,
    owner JSONB NOT NULL,
    name VARCHAR NOT NULL UNIQUE,
    full_name VARCHAR NOT NULL UNIQUE,
    description VARCHAR NOT NULL,
    private BOOLEAN NOT NULL DEFAULT 'f',
    fork BOOLEAN NOT NULL DEFAULT 'f',
    url VARCHAR NOT NULL,
    html_url VARCHAR NOT NULL,
    archive_url VARCHAR NOT NULL,
    assignees_url VARCHAR NOT NULL,
    blobs_url VARCHAR NOT NULL,
    branches_url VARCHAR NOT NULL,
    clone_url VARCHAR NOT NULL,
    collaborators_url VARCHAR NOT NULL,
    comments_url VARCHAR NOT NULL,
    commits_url VARCHAR NOT NULL,
    compare_url VARCHAR NOT NULL,
    contents_url VARCHAR NOT NULL,
    contributors_url VARCHAR NOT NULL,
    deployments_url VARCHAR NOT NULL,
    downloads_url VARCHAR NOT NULL,
    events_url VARCHAR NOT NULL,
    forks_url VARCHAR NOT NULL,
    git_commits_url VARCHAR NOT NULL,
    git_refs_url VARCHAR NOT NULL,
    git_tags_url VARCHAR NOT NULL,
    git_url VARCHAR NOT NULL,
    hooks_url VARCHAR NOT NULL,
    issue_comment_url VARCHAR NOT NULL,
    issue_events_url VARCHAR NOT NULL,
    issues_url VARCHAR NOT NULL,
    keys_url VARCHAR NOT NULL,
    labels_url VARCHAR NOT NULL,
    languages_url VARCHAR NOT NULL,
    merges_url VARCHAR NOT NULL,
    milestones_url VARCHAR NOT NULL,
    mirror_url VARCHAR NOT NULL,
    notifications_url VARCHAR NOT NULL,
    pulls_url VARCHAR NOT NULL,
    releases_url VARCHAR NOT NULL,
    ssh_url VARCHAR NOT NULL,
    stargazers_url VARCHAR NOT NULL,
    statuses_url VARCHAR NOT NULL,
    subscribers_url VARCHAR NOT NULL,
    subscription_url VARCHAR NOT NULL,
    svn_url VARCHAR NOT NULL,
    tags_url VARCHAR NOT NULL,
    teams_url VARCHAR NOT NULL,
    trees_url VARCHAR NOT NULL,
    homepage VARCHAR NOT NULL,
    language VARCHAR NOT NULL,
    forks_count INTEGER NOT NULL,
    stargazers_count INTEGER NOT NULL,
    watchers_count INTEGER NOT NULL,
    size INTEGER NOT NULL,
    default_branch VARCHAR NOT NULL,
    open_issues_count INTEGER NOT NULL,
    has_issues BOOLEAN NOT NULL DEFAULT 'f',
    has_wiki BOOLEAN NOT NULL DEFAULT 'f',
    has_pages BOOLEAN NOT NULL DEFAULT 'f',
    has_downloads BOOLEAN NOT NULL DEFAULT 'f',
    archived BOOLEAN NOT NULL DEFAULT 'f',
    pushed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
)
