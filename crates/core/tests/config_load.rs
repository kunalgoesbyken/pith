use std::path::PathBuf;

use pith_core::Config;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pith-load-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn loading_a_missing_file_writes_the_defaults() {
    let dir = scratch("missing");
    let path = dir.join("nested").join("config.toml");
    assert_eq!(Config::load(&path).unwrap(), Config::default());
    assert_eq!(
        Config::from_toml(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        Config::default()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loading_never_replaces_a_file_it_cannot_read() {
    let dir = scratch("unreadable");
    let path = dir.join("config.toml");
    let utf16: Vec<u8> = "max_words = 3\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    for broken in [utf16, b"max_words = 0".to_vec(), b"= [".to_vec()] {
        std::fs::write(&path, &broken).unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), broken);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_byte_order_mark_and_windows_line_endings_load() {
    let dir = scratch("bom");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "\u{feff}max_words = 3\r\n# note\r\ndebounce_ms = 90\r\n",
    )
    .unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!((config.max_words, config.debounce_ms), (3, 90));
    let _ = std::fs::remove_dir_all(&dir);
}
