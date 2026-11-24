use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rw_contract::commands::Hashes;
use rw_hashed_set::HashedSet;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

#[derive(Clone, Default)]
pub struct InternalService {
    inner: Arc<Mutex<Inner>>,
    write_lock: Arc<Mutex<()>>,
}

#[derive(Default)]
struct Inner {
    xray_config: Option<Value>,
    empty_config_hash: Option<String>,
    inbounds_hashmap: HashMap<String, HashedSet>,
    xtls_config_inbounds: HashSet<String>,
}

impl InternalService {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_xray_config(&self) -> Value {
        let inner = self.inner.lock().await;
        inner.xray_config.clone().unwrap_or_else(|| Value::Object(Default::default()))
    }

    pub async fn set_xray_config(&self, config: Value) {
        self.inner.lock().await.xray_config = Some(config);
    }

    pub async fn extract_users_from_config(&self, hashes: &Hashes, new_config: &Value) {
        self.cleanup().await;

        let mut inner = self.inner.lock().await;
        inner.empty_config_hash = Some(hashes.empty_config.clone());
        inner.xray_config = Some(new_config.clone());

        info!("Starting user extraction from inbounds...");
        info!("▸ Empty Config Hash: {}", hashes.empty_config);

        let valid_tags: HashSet<&str> = hashes.inbounds.iter().map(|i| i.tag.as_str()).collect();
        let remote_hash_by_tag: HashMap<&str, &str> =
            hashes.inbounds.iter().map(|i| (i.tag.as_str(), i.hash.as_str())).collect();

        if let Some(inbounds) = new_config.get("inbounds").and_then(Value::as_array) {
            for inbound in inbounds {
                let Some(tag) = inbound.get("tag").and_then(Value::as_str) else {
                    continue;
                };
                if tag.is_empty() || !valid_tags.contains(tag) {
                    continue;
                }

                let mut users = HashedSet::new();
                if let Some(clients) =
                    inbound.get("settings").and_then(|s| s.get("clients")).and_then(Value::as_array)
                {
                    for client in clients {
                        if let Some(id) = client.get("id").and_then(Value::as_str) {
                            users.add(id);
                        }
                    }
                }
                inner.inbounds_hashmap.insert(tag.to_string(), users);
            }

            let tags: Vec<String> = inner.inbounds_hashmap.keys().cloned().collect();
            for tag in tags {
                inner.xtls_config_inbounds.insert(tag);
            }
            let mut lines = Vec::new();
            for (tag, users) in &inner.inbounds_hashmap {
                lines.push(format!(
                    "▸ {tag} · {} users · {} ({})",
                    users.len(),
                    users.hash64_string(),
                    remote_hash_by_tag.get(tag.as_str()).copied().unwrap_or("N/A")
                ));
            }
            for line in lines {
                info!("{line}");
            }
        }

        info!("User extraction completed");
    }

    fn is_need_restart_core(&self, incoming: &Hashes, inner: &Inner) -> bool {
        let Some(empty_hash) = inner.empty_config_hash.as_ref() else {
            return true;
        };
        if &incoming.empty_config != empty_hash {
            warn!("Detected changes in Xray Core base configuration");
            return true;
        }
        if incoming.inbounds.len() != inner.inbounds_hashmap.len() {
            warn!("Number of Xray Core inbounds has changed");
            return true;
        }
        for (tag, users) in &inner.inbounds_hashmap {
            let Some(inbound) = incoming.inbounds.iter().find(|i| &i.tag == tag) else {
                warn!("Inbound {tag} no longer exists in Xray Core configuration");
                return true;
            };
            if users.hash64_string() != inbound.hash {
                warn!(
                    "User configuration changed for inbound {tag} ({} → {})",
                    users.hash64_string(),
                    inbound.hash
                );
                return true;
            }
        }
        info!("Xray Core configuration is up-to-date - no restart required");
        false
    }

    pub async fn needs_restart(&self, incoming: &Hashes) -> bool {
        let inner = self.inner.lock().await;
        self.is_need_restart_core(incoming, &inner)
    }

    pub async fn add_user_to_inbound(&self, inbound_tag: &str, user: &str) {
        let _guard = self.write_lock.lock().await;
        let mut inner = self.inner.lock().await;
        match inner.inbounds_hashmap.get_mut(inbound_tag) {
            Some(users) => users.add(user),
            None => {
                warn!("Inbound {inbound_tag} not found in inboundsHashMap, creating new one");
                let mut set = HashedSet::new();
                set.add(user);
                inner.inbounds_hashmap.insert(inbound_tag.to_string(), set);
            }
        }
    }

    pub async fn remove_user_from_inbound(&self, inbound_tag: &str, user: &str) {
        let _guard = self.write_lock.lock().await;
        let mut inner = self.inner.lock().await;
        let Some(users) = inner.inbounds_hashmap.get_mut(inbound_tag) else {
            return;
        };
        users.delete(user);
        if users.is_empty() {
            inner.xtls_config_inbounds.remove(inbound_tag);
            inner.inbounds_hashmap.remove(inbound_tag);
            warn!("Inbound {inbound_tag} has no users, clearing inboundsHashMap.");
        }
    }

    pub async fn get_xtls_config_inbounds(&self) -> HashSet<String> {
        self.inner.lock().await.xtls_config_inbounds.clone()
    }

    pub async fn add_xtls_config_inbound(&self, inbound_tag: &str) {
        self.inner.lock().await.xtls_config_inbounds.insert(inbound_tag.to_string());
    }

    pub async fn cleanup(&self) {
        let mut inner = self.inner.lock().await;
        if inner.xray_config.is_some() || !inner.inbounds_hashmap.is_empty() {
            info!("Cleaning up internal service.");
        }
        inner.inbounds_hashmap.clear();
        inner.xtls_config_inbounds.clear();
        inner.xray_config = None;
        inner.empty_config_hash = None;
    }

    pub async fn debug_hashes(&self, hashes: &Hashes) {
        debug!("{}", serde_json::to_string(hashes).unwrap_or_default());
    }
}
