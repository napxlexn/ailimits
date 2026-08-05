// Regression: the background saver must persist the LAST config after a rapid
// burst, and its shutdown handshake must leave the newest config on disk.
// This guards against independently spawned saves completing out of order.
use ailimits::config::{schema::Config, storage};

fn config_with_opacity(opacity: f32) -> Config {
    let mut c = Config::default();
    c.window.opacity = opacity;
    c
}

#[test]
fn saver_persists_the_last_config_under_a_burst() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let path = std::env::temp_dir().join(format!("ailimits-saver-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let saver = storage::spawn_saver(&tokio::runtime::Handle::current(), path.clone());
        let handle = saver.handle();

        // A rapid burst of distinct configs; the newest (0.100) must win.
        for i in 1..=100u32 {
            handle.save(config_with_opacity(i as f32 / 1000.0));
        }
        // The shutdown handshake drains the channel and waits for the writer.
        saver.shutdown(
            config_with_opacity(0.100),
            &tokio::runtime::Handle::current(),
        );

        let saved: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            (saved.window.opacity - 0.100).abs() < 1e-6,
            "the saver did not persist the last config: opacity = {}",
            saved.window.opacity
        );
    });
}

#[test]
fn shutdown_waits_for_the_write_so_no_later_write_can_clobber_it() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let path =
            std::env::temp_dir().join(format!("ailimits-shutdown-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let saver = storage::spawn_saver(&tokio::runtime::Handle::current(), path.clone());
        let handle = saver.handle();

        // Queue an older value, then shut down with the newest one. When
        // shutdown returns, the newest value must ALREADY be on disk — no
        // background write may still be in flight behind it.
        handle.save(config_with_opacity(0.010));
        saver.shutdown(
            config_with_opacity(0.900),
            &tokio::runtime::Handle::current(),
        );

        let saved: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // Sending on a closed channel must not panic or resurrect the writer.
        handle.save(config_with_opacity(0.010));
        let after: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);

        assert!(
            (saved.window.opacity - 0.900).abs() < 1e-6,
            "shutdown returned before the final config was on disk: opacity = {}",
            saved.window.opacity
        );
        assert!(
            (after.window.opacity - 0.900).abs() < 1e-6,
            "a post-shutdown save reached the file: opacity = {}",
            after.window.opacity
        );
    });
}
