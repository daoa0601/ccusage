use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[cfg(test)]
pub(crate) static XDG_CACHE_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const CACHE_DIRECTORY_NAME: &str = "ccusage";
const PRICING_CACHE_SUBDIR: &str = "pricing";
const PRICING_CACHE_FILE_NAME: &str = "litellm-pricing.json";
const PRICING_CACHE_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PricingCacheEnvelope {
    schema_version: u64,
    fetched_at: String,
    payload: String,
}

pub(crate) fn fresh_pricing_cache_fetched_at() -> Option<String> {
    let envelope = read_pricing_cache_envelope()?;
    is_pricing_cache_fresh(&envelope.fetched_at).then_some(envelope.fetched_at)
}

pub(crate) fn read_fresh_pricing_cache() -> Option<String> {
    let envelope = read_pricing_cache_envelope()?;
    is_pricing_cache_fresh(&envelope.fetched_at).then_some(envelope.payload)
}

pub(crate) fn read_pricing_cache() -> Option<String> {
    Some(read_pricing_cache_envelope()?.payload)
}

pub(crate) fn write_pricing_cache(payload: &str) {
    let Some(path) = pricing_cache_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let envelope = PricingCacheEnvelope {
        schema_version: PRICING_CACHE_SCHEMA_VERSION,
        fetched_at: crate::format_rfc3339_millis(crate::utc_now()),
        payload: payload.to_string(),
    };
    let Ok(bytes) = serde_json::to_vec(&envelope) else {
        return;
    };

    write_atomic(&path, &bytes);
}

fn read_pricing_cache_envelope() -> Option<PricingCacheEnvelope> {
    let bytes = fs::read(pricing_cache_path()?).ok()?;
    let envelope = serde_json::from_slice::<PricingCacheEnvelope>(&bytes).ok()?;
    (envelope.schema_version == PRICING_CACHE_SCHEMA_VERSION).then_some(envelope)
}

fn is_pricing_cache_fresh(fetched_at: &str) -> bool {
    fetched_at.get(..10) == crate::format_rfc3339_millis(crate::utc_now()).get(..10)
}

fn pricing_cache_path() -> Option<PathBuf> {
    let cache_home = match env::var_os("XDG_CACHE_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ if cfg!(test) => return None,
        _ => crate::home::home_dir()?.join(".cache"),
    };
    Some(
        cache_home
            .join(CACHE_DIRECTORY_NAME)
            .join(PRICING_CACHE_SUBDIR)
            .join(PRICING_CACHE_FILE_NAME),
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) {
    let temp_path = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if fs::write(&temp_path, bytes).is_ok() {
        let _ = fs::rename(&temp_path, path);
    }
    let _ = fs::remove_file(temp_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccusage_test_support::fs_fixture;

    struct EnvRestore {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvRestore {
        fn set_path(key: &'static str, value: &Path) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                env::set_var(self.key, value);
            } else {
                env::remove_var(self.key);
            }
        }
    }

    #[test]
    fn round_trips_fresh_pricing_payload() {
        let _guard = XDG_CACHE_HOME_LOCK.lock().unwrap();
        let fixture = fs_fixture!({});
        let _env = EnvRestore::set_path("XDG_CACHE_HOME", fixture.root());

        write_pricing_cache(r#"{"gpt-test":{}}"#);

        assert_eq!(
            read_fresh_pricing_cache(),
            Some(r#"{"gpt-test":{}}"#.to_string())
        );
    }

    #[test]
    fn ignores_stale_pricing_for_fresh_reads() {
        let _guard = XDG_CACHE_HOME_LOCK.lock().unwrap();
        let fixture = fs_fixture!({
            "ccusage/pricing/litellm-pricing.json": r#"{"schemaVersion":1,"fetchedAt":"2000-01-01T00:00:00.000Z","payload":"{}"}"#,
        });
        let _env = EnvRestore::set_path("XDG_CACHE_HOME", fixture.root());

        assert_eq!(read_fresh_pricing_cache(), None);
        assert_eq!(read_pricing_cache(), Some("{}".to_string()));
    }
}
