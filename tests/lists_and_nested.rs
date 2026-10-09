//! Every example in the "Lists and nested objects" section of README.md.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::result_large_err)] // Jail closures return figment::Error

use std::collections::BTreeMap;

use bonfig::{FromSource as _, Loader};
use figment::Jail;

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
struct App {
    #[builder(default = vec!["a.com".to_owned()])]
    domains: Vec<String>,
    #[builder(default = vec![Upstream::builder().name("main").build()])]
    upstreams: Vec<Upstream>,
    db: Db,
    #[builder(default = BTreeMap::from([("acks".to_owned(), "1".to_owned())]))]
    kafka: BTreeMap<String, String>,
    services: BTreeMap<String, Upstream>,
    tls: Option<Tls>,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
struct Upstream {
    #[builder(default = "upstream")]
    name: String,
    #[builder(default = "http://127.0.0.1")]
    url: String,
    #[builder(default = 1)]
    weight: u32,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
struct Db {
    #[builder(default = "postgres://localhost/app")]
    url: String,
    pool: Pool,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
struct Pool {
    #[builder(default = 10)]
    max: u32,
    #[builder(default = 1)]
    min: u32,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
struct Tls {
    #[builder(default = "cert.pem")]
    cert: String,
    #[builder(default = "key.pem")]
    key: String,
}

fn load_env(vars: &[(&str, &str)]) -> bonfig::Result<App> {
    let mut out = None;
    Jail::expect_with(|jail| {
        for (key, value) in vars {
            jail.set_env(key, value);
        }
        out = Some(Loader::builder().env_prefix("APP").build().load::<App>());
        Ok(())
    });
    out.unwrap()
}

// Nested objects

#[test]
fn nested_object_from_toml_table() {
    let app = App::from_toml("[db.pool]\nmax = 50").unwrap();
    assert_eq!(app.db.url, "postgres://localhost/app");
    assert_eq!(app.db.pool, Pool::builder().max(50).build());
}

#[test]
fn nested_object_from_env_path() {
    let app = load_env(&[("APP__DB__POOL__MAX", "50")]).unwrap();
    assert_eq!(app.db.url, "postgres://localhost/app");
    assert_eq!((app.db.pool.max, app.db.pool.min), (50, 1));
}

#[test]
fn nested_object_from_inline_env_dict_merges() {
    let app = load_env(&[("APP__DB", "{pool={max=50}}")]).unwrap();
    assert_eq!(app.db.url, "postgres://localhost/app");
    assert_eq!((app.db.pool.max, app.db.pool.min), (50, 1));
}

// Lists

#[test]
fn list_from_toml_replaces_default() {
    let app = App::from_toml(r#"domains = ["b.com", "c.com"]"#).unwrap();
    assert_eq!(app.domains, ["b.com", "c.com"]);
}

#[test]
fn list_from_env_replaces_default() {
    let app = load_env(&[("APP__DOMAINS", r#"["b.com", "c.com"]"#)]).unwrap();
    assert_eq!(app.domains, ["b.com", "c.com"]);
    let app = load_env(&[("APP__DOMAINS", "[]")]).unwrap();
    assert!(app.domains.is_empty());
}

#[test]
fn list_of_objects_from_toml_uses_element_defaults() {
    let app = App::from_toml(
        r#"
        [[upstreams]]
        name = "a"
        [[upstreams]]
        name = "b"
        weight = 5
        "#,
    )
    .unwrap();
    assert_eq!(
        app.upstreams,
        [
            Upstream::builder().name("a").build(),
            Upstream::builder().name("b").weight(5).build(),
        ]
    );
}

#[test]
fn list_of_objects_from_env() {
    let app = load_env(&[("APP__UPSTREAMS", r#"[{name="a"}, {name="b", weight=5}]"#)]).unwrap();
    assert_eq!(app.upstreams[0], Upstream::builder().name("a").build());
    assert_eq!(app.upstreams[1].weight, 5);
}

#[test]
fn list_item_cannot_be_set_by_index() {
    let err = load_env(&[("APP__UPSTREAMS__0__NAME", "a")]).unwrap_err();
    assert_eq!(err.path, ["upstreams"]);
}

// Maps

#[test]
fn map_merges_key_by_key() {
    let app = App::from_toml("[kafka]\nretries = \"5\"").unwrap();
    assert_eq!(app.kafka["acks"], "1");
    assert_eq!(app.kafka["retries"], "5");

    let app = load_env(&[("APP__KAFKA__RETRIES", r#""5""#)]).unwrap();
    assert_eq!(
        (app.kafka["acks"].as_str(), app.kafka["retries"].as_str()),
        ("1", "5")
    );
}

#[test]
fn map_key_with_dots_needs_inline_dict_in_env() {
    let app = App::from_toml("[kafka]\n\"bootstrap.servers\" = \"b:9092\"").unwrap();
    assert_eq!(app.kafka["bootstrap.servers"], "b:9092");

    let app = load_env(&[("APP__KAFKA", r#"{"bootstrap.servers"="b:9092"}"#)]).unwrap();
    assert_eq!(app.kafka["bootstrap.servers"], "b:9092");
    assert_eq!(app.kafka["acks"], "1");
}

#[test]
fn env_map_keys_are_lowercase() {
    let app = load_env(&[("APP__KAFKA__MAX_POLL", r#""9""#)]).unwrap();
    assert_eq!(app.kafka["max_poll"], "9");
}

#[test]
fn map_of_objects_uses_element_defaults() {
    let app = App::from_toml("[services.auth]\nurl = \"http://auth\"").unwrap();
    assert_eq!(
        app.services["auth"],
        Upstream::builder().url("http://auth").build()
    );

    let app = load_env(&[("APP__SERVICES__AUTH__URL", "http://auth")]).unwrap();
    assert_eq!(app.services["auth"].weight, 1);
}

// Optional objects

#[test]
fn optional_object_is_set_by_any_key() {
    assert_eq!(App::default().tls, None);
    let app = App::from_toml("[tls]\ncert = \"/etc/cert.pem\"").unwrap();
    assert_eq!(app.tls, Some(Tls::builder().cert("/etc/cert.pem").build()));
}

// Rust values

#[test]
fn from_serialize_takes_nested_json() {
    let app = App::from_serialize(serde_json::json!({
        "db": {"pool": {"max": 50}},
        "upstreams": [{"name": "a"}],
    }))
    .unwrap();
    assert_eq!((app.db.pool.max, app.db.pool.min), (50, 1));
    assert_eq!(app.upstreams, [Upstream::builder().name("a").build()]);
    assert_eq!(app.domains, ["a.com"]);
}
