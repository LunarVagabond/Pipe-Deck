use crate::config::store::lock_config_dir_env;
use crate::core::engine::command_test_engine;
use crate::AppState;
use std::ffi::OsString;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, MutexGuard, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tokio::sync::RwLock;

pub(super) struct CommandFixture {
    pub app: &'static AppHandle,
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
        let app = test_app();
        *app.state::<AppState>().engine.blocking_write() = engine;
        Self {
            app,
            config_dir,
            previous_env,
            _guard: guard,
        }
    }

    pub fn state(&self) -> State<'_, AppState> {
        self.app.state()
    }

    pub fn fail_config_writes(&self) {
        // Exercise persistence with the mock backend still handling all audio operations.
        let state = self.state();
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

fn test_app() -> &'static AppHandle {
    static APP: OnceLock<AppHandle> = OnceLock::new();
    APP.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        // Concrete command signatures require Wry; share its single GTK event loop.
        std::thread::spawn(move || {
            let mut context = tauri::generate_context!(test = true);
            context.config_mut().app.windows.clear();
            let app = tauri::Builder::default()
                .any_thread()
                .manage(AppState {
                    engine: Arc::new(RwLock::new(command_test_engine())),
                })
                .build(context)
                .expect("build command test app (requires a display, e.g. xvfb-run)");
            app.run_return(move |app, event| match event {
                tauri::RunEvent::Ready => sender.send(app.clone()).unwrap(),
                tauri::RunEvent::ExitRequested { api, .. } => api.prevent_exit(),
                _ => {}
            });
        });
        receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("command test app must become ready")
    })
}
