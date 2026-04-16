use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub schema_version: u32,
    pub active_profile_id: Option<String>,
    #[serde(default)]
    pub clash_api_addr: Option<String>,
    pub profiles: Vec<Profile>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            schema_version: 2,
            active_profile_id: None,
            clash_api_addr: None,
            profiles: Vec::new(),
        }
    }
}

impl AppState {
    pub fn normalize_active_profile(&mut self) -> bool {
        let mut changed = false;

        if self.profiles.is_empty() {
            if self.active_profile_id.take().is_some() {
                changed = true;
            }
            if self.clash_api_addr.take().is_some() {
                changed = true;
            }
            return changed;
        }

        let active_exists = self
            .active_profile_id
            .as_ref()
            .is_some_and(|id| self.profiles.iter().any(|p| &p.id == id));
        if !active_exists {
            self.active_profile_id = self.profiles.first().map(|p| p.id.clone());
            changed = true;
        }

        changed
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        self.active_profile_id
            .as_ref()
            .and_then(|id| self.profiles.iter().find(|p| &p.id == id))
    }

    pub fn find_profile_mut_by_normalized_url(&mut self, url: &str) -> Option<&mut Profile> {
        self.profiles
            .iter_mut()
            .find(|p| p.subscription_url_normalized == url)
    }

    /// Resolve a target string to a profile index.
    /// Resolution order: id -> remark -> numeric index.
    pub fn resolve_target(&self, target: &str) -> Option<usize> {
        if let Some(idx) = self.profiles.iter().position(|p| p.id == target) {
            return Some(idx);
        }
        if let Some(idx) = self.profiles.iter().position(|p| p.remark == target) {
            return Some(idx);
        }
        if let Ok(i) = target.parse::<usize>() {
            if i < self.profiles.len() {
                return Some(i);
            }
        }
        None
    }

    pub fn remark_exists(&self, remark: &str) -> bool {
        self.profiles.iter().any(|p| p.remark == remark)
    }

    pub fn generate_unique_remark(&self, base: &str) -> String {
        if !self.remark_exists(base) {
            return base.to_string();
        }
        let mut suffix = 2;
        loop {
            let candidate = format!("{base}-{suffix}");
            if !self.remark_exists(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub subscription_url: String,
    pub subscription_url_normalized: String,
    pub remark: String,
    pub remark_source: RemarkSource,
    pub last_update_at: Option<DateTime<Utc>>,
    pub last_update_status: Option<UpdateStatus>,
    pub last_update_error: Option<String>,
    pub node_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemarkSource {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    Success,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(id: &str, remark: &str, url: &str) -> Profile {
        Profile {
            id: id.to_string(),
            subscription_url: url.to_string(),
            subscription_url_normalized: url.to_lowercase(),
            remark: remark.to_string(),
            remark_source: RemarkSource::Auto,
            last_update_at: None,
            last_update_status: None,
            last_update_error: None,
            node_count: 0,
        }
    }

    #[test]
    fn test_generate_unique_remark_no_conflict() {
        let state = AppState::default();
        assert_eq!(state.generate_unique_remark("aaa"), "aaa");
    }

    #[test]
    fn test_generate_unique_remark_with_conflict() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("1", "aaa", "https://aaa.com/1"));
        assert_eq!(state.generate_unique_remark("aaa"), "aaa-2");
    }

    #[test]
    fn test_generate_unique_remark_with_multiple_conflicts() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("1", "aaa", "https://aaa.com/1"));
        state
            .profiles
            .push(make_profile("2", "aaa-2", "https://aaa.com/2"));
        assert_eq!(state.generate_unique_remark("aaa"), "aaa-3");
    }

    #[test]
    fn test_resolve_target_by_id() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "test", "https://test.com"));
        assert_eq!(state.resolve_target("prof_01"), Some(0));
    }

    #[test]
    fn test_resolve_target_by_remark() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "test", "https://test.com"));
        assert_eq!(state.resolve_target("test"), Some(0));
    }

    #[test]
    fn test_resolve_target_by_index() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "test", "https://test.com"));
        assert_eq!(state.resolve_target("0"), Some(0));
    }

    #[test]
    fn test_resolve_target_not_found() {
        let state = AppState::default();
        assert_eq!(state.resolve_target("nonexistent"), None);
    }

    #[test]
    fn test_active_profile() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "test", "https://test.com"));
        state.active_profile_id = Some("prof_01".to_string());
        assert!(state.active_profile().is_some());
        assert_eq!(state.active_profile().unwrap().remark, "test");
    }

    #[test]
    fn test_normalize_active_profile_sets_first_when_missing() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "first", "https://test.com/1"));
        state
            .profiles
            .push(make_profile("prof_02", "second", "https://test.com/2"));
        state.active_profile_id = Some("missing".to_string());

        assert!(state.normalize_active_profile());
        assert_eq!(state.active_profile_id.as_deref(), Some("prof_01"));
    }

    #[test]
    fn test_normalize_active_profile_clears_orphaned_state() {
        let mut state = AppState {
            active_profile_id: Some("missing".to_string()),
            clash_api_addr: Some("127.0.0.1:9090".to_string()),
            ..AppState::default()
        };

        assert!(state.normalize_active_profile());
        assert_eq!(state.active_profile_id, None);
        assert_eq!(state.clash_api_addr, None);
    }

    #[test]
    fn test_find_profile_by_normalized_url() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "test", "https://test.com/sub"));
        let found = state.find_profile_mut_by_normalized_url("https://test.com/sub");
        assert!(found.is_some());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut state = AppState::default();
        state
            .profiles
            .push(make_profile("prof_01", "test", "https://test.com"));
        state.active_profile_id = Some("prof_01".to_string());
        state.clash_api_addr = Some("127.0.0.1:9090".to_string());

        let json = serde_json::to_string_pretty(&state).unwrap();
        let deserialized: AppState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.schema_version, 2);
        assert_eq!(deserialized.profiles.len(), 1);
        assert_eq!(deserialized.active_profile_id, Some("prof_01".to_string()));
        assert_eq!(
            deserialized.clash_api_addr,
            Some("127.0.0.1:9090".to_string())
        );
    }
}
