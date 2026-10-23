use crate::config::store::lock_config_dir_env;
use crate::core::engine::command_test_engine;
use crate::core::models::RuntimeGraph;
use crate::AppState;
use std::ffi::OsString;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Listener, Manager};
use tokio::sync::RwLock;

pub(super) struct CommandFixture {
    pub app: App<MockRuntime>,
    graph_updates: Arc<Mutex<Vec<RuntimeGraph>>>,
    config_dir: std::path::PathBuf,
    previous_env: Vec<(&'static str, Option<OsString>)>,
    _guard: MutexGuard<'static, ()>,
}

impl CommandFixture {
    pub fn new() -> Self {
        let guard = lock_config_dir_env();
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let config_dir = std::env::temp_dir()
            .join(format!(
                "pipe-deck-command-test-config-{}",
                std::process::id()
            ))
            .join(COUNTER.fetch_add(1, Ordering::SeqCst).to_string());
        std::fs::create_dir_all(&config_dir).unwrap();
        let previous_env = ["PIPE_DECK_CONFIG_DIR", "PIPE_DECK_USE_MOCK"]
            .map(|name| (name, std::env::var_os(name)))
            .to_vec();
        std::env::set_var("PIPE_DECK_CONFIG_DIR", &config_dir);
        std::env::set_var("PIPE_DECK_USE_MOCK", "1");

        let mut engine = command_test_engine();
        engine.refresh_graph().expect("initial mock graph refresh");
        let app = mock_builder()
            .manage(AppState {
                engine: Arc::new(RwLock::new(engine)),
            })
            .build(mock_context(noop_assets()))
            .expect("build command test app");
        let graph_updates = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&graph_updates);
        // Rust listeners run inline, so completed commands have already recorded their events.
        app.listen("graph-updated", move |event| {
            let graph = serde_json::from_str(event.payload()).expect("graph-updated payload");
            recorded.lock().unwrap().push(graph);
        });
        Self {
            app,
            graph_updates,
            config_dir,
            previous_env,
            _guard: guard,
        }
    }

    pub fn isolate_plugins(&mut self) {
        self.previous_env.push((
            "PIPE_DECK_BUNDLED_PLUGINS",
            std::env::var_os("PIPE_DECK_BUNDLED_PLUGINS"),
        ));
        std::env::set_var("PIPE_DECK_BUNDLED_PLUGINS", self.config_dir.join("bundled"));
    }

    pub fn take_graph_updates(&self) -> Vec<RuntimeGraph> {
        std::mem::take(&mut *self.graph_updates.lock().unwrap())
    }

    pub fn use_backend(&self, backend: Box<dyn crate::backend::AudioBackend>) {
        crate::core::engine::command_test_set_backend(
            &mut self.app.state::<AppState>().engine.blocking_write(),
            backend,
        );
    }

    pub fn assert_graph_update(&self, expected: &RuntimeGraph) {
        let updates = self.take_graph_updates();
        assert_eq!(updates.len(), 1, "one graph-updated event per operation");
        assert_eq!(
            serde_json::to_value(&updates[0]).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    pub fn fail_config_writes(&self) {
        // Exercise persistence with the mock backend still handling all audio operations.
        let state = self.app.state::<AppState>();
        let mut engine = state.engine.blocking_write();
        let mut graph = engine.runtime_graph().clone();
        graph.data_source = "persisted-config-test".into();
        engine.apply_graph_update(graph);
        let blocker = self.config_dir.join("blocked");
        std::fs::write(&blocker, b"config directory blocker").unwrap();
        std::env::set_var("PIPE_DECK_CONFIG_DIR", blocker);
    }
}

impl Drop for CommandFixture {
    fn drop(&mut self) {
        for (name, value) in &self.previous_env {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}
