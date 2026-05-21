use std::time::Duration;

use semver::Version;

use crate::error::{Error, Result};

pub const RELEASES_PAGE_URL: &str = "https://github.com/HugoSart/color-lut-tweaks/releases";
const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/HugoSart/color-lut-tweaks/releases/latest";
const USER_AGENT: &str = concat!("color-lut-tweaks/", env!("CARGO_PKG_VERSION"));
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateCheck {
    Latest,
    Available { version: String, url: String },
}

#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

pub fn check_latest() -> Result<UpdateCheck> {
    let release = reqwest::blocking::Client::builder()
        .timeout(UPDATE_CHECK_TIMEOUT)
        .build()
        .map_err(|source| {
            Error::platform(format!("failed to create update checker client: {source}"))
        })?
        .get(LATEST_RELEASE_API_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .map_err(|source| Error::platform(format!("failed to check for updates: {source}")))?
        .error_for_status()
        .map_err(|source| Error::platform(format!("failed to check for updates: {source}")))?
        .json::<GitHubRelease>()
        .map_err(|source| Error::platform(format!("failed to read update response: {source}")))?;

    if is_newer_version(&release.tag_name, env!("CARGO_PKG_VERSION")) {
        Ok(UpdateCheck::Available {
            version: release.tag_name,
            url: RELEASES_PAGE_URL.to_string(),
        })
    } else {
        Ok(UpdateCheck::Latest)
    }
}

pub fn is_newer_version(candidate: &str, current: &str) -> bool {
    let Ok(candidate) = parse_version(candidate) else {
        return false;
    };
    let Ok(current) = parse_version(current) else {
        return false;
    };

    candidate > current
}

fn parse_version(value: &str) -> std::result::Result<Version, semver::Error> {
    Version::parse(value.trim().trim_start_matches('v'))
}
