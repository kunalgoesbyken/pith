use std::time::Duration;

use semver::Version;
use serde::Deserialize;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const RELEASES_PER_PAGE: u32 = 30;
const TIMEOUT: Duration = Duration::from_secs(10);
const USER_AGENT: &str = concat!("pith/", env!("CARGO_PKG_VERSION"));
const ACCEPT: &str = "application/vnd.github+json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateFailure {
    Offline,
    RateLimited,
    BadResponse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheck {
    UpToDate,
    Available(Release),
    Failed(UpdateFailure),
}

#[derive(Deserialize)]
struct Entry {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

fn running() -> Version {
    let part = |value: &str| value.parse().unwrap_or_default();
    Version::new(
        part(env!("CARGO_PKG_VERSION_MAJOR")),
        part(env!("CARGO_PKG_VERSION_MINOR")),
        part(env!("CARGO_PKG_VERSION_PATCH")),
    )
}

fn releases_url() -> String {
    let api = REPOSITORY.replacen("https://github.com/", "https://api.github.com/repos/", 1);
    format!("{api}/releases?per_page={RELEASES_PER_PAGE}")
}

fn failure(error: ureq::Error) -> UpdateFailure {
    match error {
        ureq::Error::StatusCode(403 | 429) => UpdateFailure::RateLimited,
        ureq::Error::StatusCode(_) | ureq::Error::Json(_) | ureq::Error::BodyExceedsLimit(_) => {
            UpdateFailure::BadResponse
        }
        _ => UpdateFailure::Offline,
    }
}

fn releases() -> Result<Vec<Entry>, UpdateFailure> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(USER_AGENT)
        .build()
        .into();
    match agent.get(releases_url()).header("Accept", ACCEPT).call() {
        Ok(mut response) => response.body_mut().read_json().map_err(failure),
        Err(ureq::Error::StatusCode(404)) => Ok(Vec::new()),
        Err(error) => Err(failure(error)),
    }
}

fn stable(entry: Entry) -> Option<(Version, String)> {
    if entry.draft || entry.prerelease {
        return None;
    }
    let digits = entry
        .tag_name
        .strip_prefix(['v', 'V'])
        .unwrap_or(&entry.tag_name);
    let version = Version::parse(digits).ok()?;
    version.pre.is_empty().then_some((version, entry.tag_name))
}

fn newest_above(running: &Version, entries: Vec<Entry>) -> Option<Release> {
    entries
        .into_iter()
        .filter_map(stable)
        .filter(|(version, _)| version > running)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(version, tag)| Release {
            version: version.to_string(),
            url: format!("{REPOSITORY}/releases/tag/{tag}"),
        })
}

pub fn check_for_update() -> UpdateCheck {
    match releases() {
        Ok(entries) => {
            newest_above(&running(), entries).map_or(UpdateCheck::UpToDate, UpdateCheck::Available)
        }
        Err(failure) => UpdateCheck::Failed(failure),
    }
}
