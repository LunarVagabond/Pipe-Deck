#[path = "paths.rs"]
mod paths;

#[test]
fn windows_public_wrappers_use_appdata() {
    use std::ffi::OsStr;

    let _env = paths::tests::PathEnv::new(&[("APPDATA", r"C:\Users\u\AppData\Roaming")]);
    assert_eq!(
        paths::config_dir().as_os_str(),
        OsStr::new(r"C:\Users\u\AppData\Roaming\pipe-deck")
    );
    assert_eq!(
        paths::state_dir().as_os_str(),
        OsStr::new(r"C:\Users\u\AppData\Roaming\pipe-deck")
    );
    assert_eq!(
        paths::user_systemd_dir().as_os_str(),
        OsStr::new(r"C:\Users\u\AppData\Roaming\pipe-deck")
    );
    assert_eq!(
        paths::audit_log_path().as_os_str(),
        OsStr::new(r"C:\Users\u\AppData\Roaming\pipe-deck\plugin-audit.jsonl")
    );
    assert_eq!(paths::runtime_dir(), None);
}
