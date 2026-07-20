fn main() {
    let exit_code = if std::env::var("PIPE_DECK_DAEMON_EPHEMERAL").as_deref() == Ok("1") {
        pipe_deck_lib::daemon::run_ephemeral()
    } else {
        pipe_deck_lib::daemon::run()
    };
    std::process::exit(exit_code);
}
