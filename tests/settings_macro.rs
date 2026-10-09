#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use bonfig::{FromSource as _, Secret};

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
#[builder(on(PathBuf, into))]
struct Settings {
    /// Explicit default.
    #[builder(default = 8000)]
    port: u16,
    /// No annotation: the default of the type.
    debug: bool,
    domains: Vec<String>,
    db: Db,
    /// `None` by default.
    nickname: Option<String>,
    #[builder(default = Secret::new("dev-secret".to_owned()))]
    token: Secret<String>,
    #[builder(default = "/tmp")]
    data_dir: PathBuf,
    /// Not in the builder, but still has a value.
    #[builder(skip = 7)]
    internal: u32,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
struct Db {
    #[builder(default = "postgres://localhost/app")]
    url: String,
    #[builder(default = 10)]
    max_pool: u32,
}

#[test]
fn defaults_from_annotations_and_types() {
    let s = Settings::default();
    assert_eq!(s.port, 8000);
    assert!(!s.debug);
    assert!(s.domains.is_empty());
    assert_eq!(s.db, Db::default());
    assert_eq!(s.db.url, "postgres://localhost/app");
    assert_eq!(s.nickname, None);
    assert_eq!(s.token.expose(), "dev-secret");
    assert_eq!(s.data_dir, PathBuf::from("/tmp"));
    assert_eq!(s.internal, 7);
}

#[test]
fn builder_setters_take_str_and_path_like_values() {
    let s = Settings::builder()
        .nickname("bob")
        .data_dir("/var/app")
        .db(Db::builder().url("postgres://db/app").build())
        .build();
    assert_eq!(s.nickname.as_deref(), Some("bob"));
    assert_eq!(s.data_dir, PathBuf::from("/var/app"));
    assert_eq!(
        (s.db.url.as_str(), s.db.max_pool),
        ("postgres://db/app", 10)
    );
}

#[test]
fn partial_toml_with_extra_serde_attributes() {
    let s = Settings::from_toml(
        r#"
        debug = true
        data-dir = "/srv"
        [db]
        max-pool = 50
        "#,
    )
    .unwrap();
    assert!(s.debug);
    assert_eq!(s.data_dir, PathBuf::from("/srv"));
    // rename_all applies to Settings only, so Db keeps snake_case keys.
    assert_eq!(s.db.max_pool, 10);

    let s = Settings::from_toml("[db]\nmax_pool = 50").unwrap();
    assert_eq!(s.db.max_pool, 50);
    assert_eq!(s.port, 8000);
}
