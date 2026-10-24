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
        // One flat root per fixture instance: Drop removes exactly this path,
        // so cleanup never needs to know how many fixtures share the parent.
        let config_dir = std::env::temp_dir().join(format!(
            "pipe-deck-command-test-config-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
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

    pub fn isolate_daemon(&mut self) {
        for name in [
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_STATE_HOME",
            "XDG_CONFIG_HOME",
            "PIPE_DECK_DAEMON_PATH",
        ] {
            self.previous_env.push((name, std::env::var_os(name)));
        }
        // Reject the transport before any session-bus connection can be attempted.
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "invalid:");
        std::env::set_var("XDG_STATE_HOME", self.config_dir.join("state"));
        std::env::set_var("XDG_CONFIG_HOME", self.config_dir.join("service-config"));
        std::env::set_var("PIPE_DECK_DAEMON_PATH", self.config_dir.join("daemon"));
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
        let _ = std::fs::remove_dir_all(&self.config_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::CommandFixture;

    #[test]
    fn drop_restores_env_and_removes_its_config_directory() {
        // Record the baseline while holding the crate-wide env lock so the
        // values below are a quiesced state rather than a live fixture's
        // transient ones; the lock is released before the fixture takes it.
        let guard = crate::config::store::lock_config_dir_env();
        let previous_config_dir = std::env::var_os("PIPE_DECK_CONFIG_DIR");
        let previous_use_mock = std::env::var_os("PIPE_DECK_USE_MOCK");
        drop(guard);
        let fixture = CommandFixture::new();
        let config_dir = fixture.config_dir.clone();
        assert!(
            config_dir.is_dir(),
            "fixture must create its config directory, or the absence check below proves nothing"
        );
        drop(fixture);
        assert!(
            !config_dir.exists(),
            "dropping the fixture must remove its config directory"
        );
        // Every writer of these two variables holds the crate-wide
        // `lock_config_dir_env` mutex for the whole mutation window, so
        // re-acquiring it here freezes the environment for these reads.
        let _guard = crate::config::store::lock_config_dir_env();
        assert_eq!(
            std::env::var_os("PIPE_DECK_CONFIG_DIR"),
            previous_config_dir,
            "drop must restore PIPE_DECK_CONFIG_DIR"
        );
        assert_eq!(
            std::env::var_os("PIPE_DECK_USE_MOCK"),
            previous_use_mock,
            "drop must restore PIPE_DECK_USE_MOCK"
        );
    }

    #[test]
    fn drop_removes_the_config_directory_even_when_the_fixture_unwinds() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let fixture = CommandFixture::new();
            tx.send(fixture.config_dir.clone())
                .expect("test still waiting for the fixture path");
            panic!("drop the fixture during unwind");
        });
        let config_dir = rx.recv().expect("fixture path before the panic");
        assert!(
            handle.join().is_err(),
            "the spawned thread should have panicked while holding the fixture"
        );
        assert!(
            !config_dir.exists(),
            "unwinding out of the fixture must still remove its config directory"
        );
    }
}
