use std::{
    collections::HashSet,
    mem::MaybeUninit,
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use dashmap::DashMap;
use tower_lsp::{Client, lsp_types::Diagnostic};
use url::Url;

#[derive(Clone)]
pub struct DiagManager {
    worker: Arc<DiagWorker>,
}

static SINGLETON: RwLock<MaybeUninit<DiagManager>> = RwLock::new(MaybeUninit::uninit());
static INIT: AtomicBool = AtomicBool::new(false);

impl DiagManager {
    pub fn get() -> Option<Self> {
        unsafe {
            if INIT.load(std::sync::atomic::Ordering::SeqCst) {
                Some(
                    SINGLETON
                        .read()
                        .expect("DiagManager poisoned")
                        .assume_init_ref()
                        .clone(),
                )
            } else {
                None
            }
        }
    }

    pub fn init(client: Client) {
        if !INIT.swap(true, std::sync::atomic::Ordering::SeqCst) {
            *SINGLETON.write().expect("DiagManager poisoned") = MaybeUninit::new(Self {
                worker: Arc::new(DiagWorker {
                    client,
                    last_touched: tokio::sync::Mutex::new(HashSet::new()),
                    current: Arc::new(DashMap::new()),
                }),
            });
            INIT.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[allow(dead_code)]
    pub fn current(&self, scope: &str, url: &Url) -> Option<Vec<Diagnostic>> {
        self.worker.current(scope, url)
    }

    pub fn set_current(&self, scope: String, url: &Url, diags: Vec<Diagnostic>) {
        self.worker.set_current(scope, url, diags);
    }

    pub fn clear_current(&self, scope: &str) {
        self.worker.clear_current(scope);
    }

    pub async fn sync(&self, scope: &str) {
        self.worker.sync(scope).await;
    }
}

pub struct DiagWorker {
    client: Client,
    last_touched: tokio::sync::Mutex<HashSet<Url>>,
    current: Arc<DashMap<(String, Url), Vec<Diagnostic>>>,
}

impl DiagWorker {
    #[allow(dead_code)]
    pub fn current(&self, scope: &str, url: &Url) -> Option<Vec<Diagnostic>> {
        self.current
            .get(&(scope.to_string(), url.clone()))
            .map(|x| x.clone())
    }

    pub fn set_current(&self, scope: String, url: &Url, diags: Vec<Diagnostic>) {
        self.current.insert((scope, url.clone()), diags);
    }

    pub fn clear_current(&self, scope: &str) {
        self.current.retain(|k, _| k.0 != scope);
    }

    pub async fn sync(&self, _scope: &str) {
        let mut last_touched = self.last_touched.lock().await;
        let mut touched = HashSet::new();
        let diags = self
            .current
            .iter()
            .map(|x| (x.key().1.clone(), x.value().clone()))
            .collect::<Vec<_>>();
        let mut diags_by_file = std::collections::HashMap::new();
        for (url, diags) in diags {
            touched.insert(url.clone());
            diags_by_file
                .entry(url)
                .or_insert_with(Vec::new)
                .extend(diags);
        }
        let client = self.client.clone();
        let previous = std::mem::replace(&mut *last_touched, touched);
        {
            // clear files with previous diagnostics
            for url in previous {
                client
                    .publish_diagnostics(url.clone(), Vec::new(), None)
                    .await;
            }
            // publish new diagnostics
            for (url, diags) in diags_by_file {
                client.publish_diagnostics(url, diags, None).await;
            }
        }
    }
}
