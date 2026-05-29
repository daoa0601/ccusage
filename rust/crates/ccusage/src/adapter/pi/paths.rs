use std::{collections::HashSet, env, path::PathBuf};

use crate::Result;

const PI_AGENT_DIR_ENV: &str = "PI_AGENT_DIR";
const PI_CODING_AGENT_DIR_ENV: &str = "PI_CODING_AGENT_DIR";
const PI_CONFIG_DIR_ENV: &str = "PI_CONFIG_DIR";

pub(super) fn paths(custom_path: Option<&str>) -> Result<Vec<PathBuf>> {
    if let Some(custom_path) = custom_path.filter(|path| !path.trim().is_empty()) {
        return Ok(existing_path_list(custom_path));
    }
    if let Ok(env_paths) = env::var(PI_AGENT_DIR_ENV) {
        if !env_paths.trim().is_empty() {
            return Ok(existing_path_list(&env_paths));
        }
    }

    let home =
        crate::home::home_dir().ok_or_else(|| crate::cli_error("home directory is not set"))?;
    Ok(default_pi_paths(&home))
}

fn existing_path_list(raw: &str) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    raw.split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .collect()
}

fn default_pi_paths(home: &std::path::Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    match env::var(PI_CODING_AGENT_DIR_ENV)
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
    {
        Some(agent_dir) => candidates.push(PathBuf::from(agent_dir).join("sessions")),
        None => {
            let config_dir = env::var(PI_CONFIG_DIR_ENV)
                .ok()
                .map(|path| path.trim().to_string())
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| ".omp".to_string());
            candidates.push(home.join(config_dir).join("agent/sessions"));
            if let Some(xdg_data_home) = env::var("XDG_DATA_HOME")
                .ok()
                .map(|path| path.trim().to_string())
                .filter(|path| !path.is_empty())
            {
                candidates.push(PathBuf::from(xdg_data_home).join("omp/sessions"));
            }
        }
    }
    candidates.push(home.join(".pi/agent/sessions"));

    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| path.is_dir() && seen.insert(path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccusage_test_support::fs_fixture;
    use std::env;

    fn with_env<T>(values: &[(&str, Option<&str>)], run: impl FnOnce() -> T) -> T {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in values {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }
        let result = run();
        for (key, value) in previous {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }
        result
    }

    #[test]
    fn finds_oh_my_pi_and_legacy_pi_directories() {
        let fixture = fs_fixture!({
            "home/.omp/agent/sessions/.keep": "",
            "home/.pi/agent/sessions/.keep": "",
        });
        with_env(
            &[
                (PI_CODING_AGENT_DIR_ENV, None::<&str>),
                (PI_CONFIG_DIR_ENV, None::<&str>),
                ("XDG_DATA_HOME", None::<&str>),
            ],
            || {
                assert_eq!(
                    default_pi_paths(&fixture.path("home")),
                    vec![
                        fixture.path("home/.omp/agent/sessions"),
                        fixture.path("home/.pi/agent/sessions"),
                    ]
                );
            },
        );
    }

    #[test]
    fn pi_coding_agent_dir_overrides_oh_my_pi_config_dir() {
        let fixture = fs_fixture!({
            "home/.pi/agent/sessions/.keep": "",
            "agent/sessions/.keep": "",
        });
        let agent_dir = fixture.path("agent").to_string_lossy().into_owned();
        with_env(
            &[
                (PI_CODING_AGENT_DIR_ENV, Some(agent_dir.as_str())),
                (PI_CONFIG_DIR_ENV, None::<&str>),
                ("XDG_DATA_HOME", None::<&str>),
            ],
            || {
                assert_eq!(
                    default_pi_paths(&fixture.path("home")),
                    vec![
                        fixture.path("agent/sessions"),
                        fixture.path("home/.pi/agent/sessions"),
                    ]
                );
            },
        );
    }
}
