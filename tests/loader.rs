#![allow(clippy::unwrap_used, clippy::expect_used, clippy::result_large_err)] // Jail closures return figment::Error

use std::collections::BTreeMap;

use bonfig::serde::{Deserialize, Serialize};
use bonfig::{FromSource as _, Loader, Secret, bon::Builder};
use figment::Jail;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(default, crate = "bonfig::serde")]
#[builder(crate = ::bonfig::bon, on(String, into))]
struct Settings {
    #[builder(default = "dev".to_owned())]
    name: String,
    #[builder(default = 8000)]
    port: u16,
    #[builder(default = vec!["a.com".to_owned()])]
    domains: Vec<String>,
    #[builder(default = BTreeMap::from([
        ("acks".to_owned(), "1".to_owned()),
        ("retries".to_owned(), "3".to_owned()),
    ]))]
    kafka: BTreeMap<String, String>,
    #[builder(default = Secret::new("dev-secret".to_owned()))]
    token: Secret<String>,
    #[builder(default)]
    redis: Redis,
    #[builder(default = Redis::builder().db(1).build())]
    cache: Redis,
    nickname: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(default, crate = "bonfig::serde")]
#[builder(crate = ::bonfig::bon, on(String, into))]
struct Redis {
    #[builder(default = "redis://127.0.0.1".to_owned())]
    url: String,
    #[builder(default)]
    db: u8,
}

bonfig::default_from_builder!(Settings, Redis);

fn loader() -> Loader {
    Loader::builder()
        .env_prefix("APP")
        .file("defaults.toml")
        .files(None::<&str>)
        .file("missing.toml")
        .build()
}

#[test]
fn defaults_come_from_builder() {
    let s = Settings::default();
    assert_eq!((s.name.as_str(), s.port, s.cache.db), ("dev", 8000, 1));
    assert_eq!(s.nickname, None);
    Jail::expect_with(|_| {
        assert_eq!(loader().load::<Settings>().unwrap(), Settings::default());
        Ok(())
    });
}

#[test]
fn layers_merge_in_order() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "defaults.toml",
            r#"
            name = "file"
            port = 1
            domains = ["b.com"]
            [kafka]
            acks = "all"
            "#,
        )?;
        jail.create_file("named.toml", "port = 2")?;
        jail.set_env("APP_CONFIG_FILE", "named.toml");
        jail.set_env("APP__NAME", "env");
        jail.set_env("APP__REDIS__URL", "redis://env");
        jail.set_env("APP__KAFKA__RETRIES", r#""5""#); // quoted: a bare 5 is a number

        let s: Settings = loader().load().unwrap();
        assert_eq!(s.name, "env"); // env var over files
        assert_eq!(s.port, 2); // named file over optional file
        assert_eq!(s.domains, ["b.com"]); // lists replace
        assert_eq!(s.kafka["acks"], "all"); // maps merge key by key
        assert_eq!(s.kafka["retries"], "5");
        assert_eq!(s.redis.url, "redis://env");
        assert_eq!(s.cache.db, 1); // struct default stays
        Ok(())
    });
}

#[test]
fn env_string_that_looks_like_a_number_needs_quotes() {
    Jail::expect_with(|jail| {
        jail.set_env("APP__NAME", "123");
        let err = loader().load::<Settings>().unwrap_err();
        assert!(
            err.to_string().contains(r#"key "NAME" in `APP__`"#),
            "{err}"
        );

        jail.set_env("APP__NAME", r#""123""#);
        assert_eq!(loader().load::<Settings>().unwrap().name, "123");
        Ok(())
    });
}

#[test]
fn missing_named_file_is_an_error() {
    Jail::expect_with(|jail| {
        jail.set_env("APP_CONFIG_FILE", "nope.toml");
        let err = loader().load::<Settings>().unwrap_err();
        assert!(err.to_string().contains("nope.toml"), "{err}");

        // An empty value means unset.
        jail.set_env("APP_CONFIG_FILE", "");
        assert_eq!(loader().load::<Settings>().unwrap().port, 8000);

        // config_path wins over APP_CONFIG_FILE
        jail.create_file("cli.toml", "port = 3")?;
        let s: Settings = Loader::builder()
            .env_prefix("APP")
            .config_path("cli.toml")
            .build()
            .load()
            .unwrap();
        assert_eq!(s.port, 3);
        Ok(())
    });
}

#[test]
fn optional_file_is_not_searched_in_parent_directories() {
    Jail::expect_with(|jail| {
        jail.create_file("defaults.toml", "port = 1")?;
        jail.change_dir(jail.create_dir("sub")?)?;
        assert_eq!(loader().load::<Settings>().unwrap().port, 8000);
        Ok(())
    });
}

#[test]
fn load_focused_cascades_general_to_specific() {
    Jail::expect_with(|jail| {
        jail.create_file(
            "defaults.toml",
            r#"
            [redis]
            url = "redis://shared"
            db = 5
            [cache]
            db = 7
            [extra.redis]
            db = 9
            "#,
        )?;
        let mut s: Settings = loader().load().unwrap();
        s.cache = loader().load_focused(&["redis", "cache"], s.cache).unwrap();
        assert_eq!(
            s.cache,
            Redis::builder().url("redis://shared").db(7).build()
        );

        // Dotted section paths and env vars work in the cascade.
        jail.set_env("APP__EXTRA__REDIS__URL", "redis://env");
        let r: Redis = loader()
            .load_focused(&["redis", "extra.redis"], Redis::default())
            .unwrap();
        assert_eq!(r, Redis::builder().url("redis://env").db(9).build());
        Ok(())
    });
}

#[test]
fn from_toml_reads_no_env() {
    Jail::expect_with(|jail| {
        jail.set_env("APP__NAME", "env");
        let s = Settings::from_toml("port = 9").unwrap();
        assert_eq!((s.name.as_str(), s.port), ("dev", 9));
        Ok(())
    });
}

#[test]
fn patch_merges_partial_json() {
    let base = Settings::default();
    let changes = serde_json::json!({"port": 1, "kafka": {"acks": "all"}, "unknown": true});
    let s = bonfig::patch(&base, changes).unwrap();
    assert_eq!(s.port, 1);
    assert_eq!(s.kafka["acks"], "all");
    assert_eq!(s.kafka["retries"], "3");
    assert_eq!(s.name, "dev");
}

#[test]
fn patch_with_json_null_clears_an_option_only() {
    let base = Settings::builder().nickname("bob").build();
    let s = bonfig::patch(&base, serde_json::json!({"nickname": null, "port": 1})).unwrap();
    assert_eq!((s.nickname, s.port), (None, 1));

    let err = bonfig::patch(&base, serde_json::json!({"port": null})).unwrap_err();
    assert_eq!(err.path, ["port"]);
}

#[test]
fn secret_is_hidden_in_debug_and_loaded_from_env() {
    Jail::expect_with(|jail| {
        jail.set_env("APP__TOKEN", "from-env");
        let s: Settings = loader().load().unwrap();
        assert_eq!(s.token.expose(), "from-env");
        let debug = format!("{s:?}");
        assert!(!debug.contains("from-env"), "{debug}");
        assert!(debug.contains("token: ***"), "{debug}");
        Ok(())
    });
}
