// Regression: the background saver must persist the LAST config after a rapid
// burst (регресійна перевірка: фоновий записувач має зберегти ОСТАННЮ конфігурацію).
// This guards against independently spawned saves completing out of order
// (це захищає від завершення незалежних операцій запису в неправильному порядку).
use ailimits::config::{schema::Config, storage};

#[test]
fn saver_persists_the_last_config_under_a_burst() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let path = std::env::temp_dir().join(format!("ailimits-saver-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let tx = storage::spawn_saver(&tokio::runtime::Handle::current(), path.clone());

        // A rapid burst of distinct configs; the newest (0.100) must win.
        for i in 1..=100u32 {
            let mut c = Config::default();
            c.window.opacity = i as f32 / 1000.0;
            tx.send(Some(c)).unwrap();
        }
        // Let the saver drain and write the final value, then stop it.
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        drop(tx);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let saved: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            (saved.window.opacity - 0.100).abs() < 1e-6,
            "the saver did not persist the last config: opacity = {}",
            saved.window.opacity
        );
    });
}
