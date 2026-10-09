#![allow(clippy::unwrap_used, clippy::expect_used, clippy::result_large_err)] // Jail closures return figment::Error

use bonfig::serde::{Deserialize, Serialize};
use bonfig::{FromSource as _, Loader, bon::Builder};
use figment::Jail;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Builder)]
#[serde(default, crate = "bonfig::serde")]
#[builder(crate = ::bonfig::bon, on(String, into))]
struct Settings {
    #[builder(default = "dev".to_owned())]
    name: String,
    #[builder(default = 8000)]
    port: u16,
}

bonfig::default_from_builder!(Settings);

#[test]
fn from_provider_and_from_serialize() {
    let s = Settings::from_provider(figment::providers::Serialized::default("port", 1)).unwrap();
    assert_eq!(s, Settings::builder().port(1).build());

    let s = Settings::from_serialize(serde_json::json!({"name": "x"})).unwrap();
    assert_eq!(s, Settings::builder().name("x").build());
}

#[test]
fn unsupported_extension_is_an_error() {
    Jail::expect_with(|jail| {
        jail.create_file("app.ini", "port = 1")?;
        let err = Loader::builder()
            .file("app.ini")
            .build()
            .load::<Settings>()
            .unwrap_err();
        assert!(err.to_string().contains("app.ini"), "{err}");
        Ok(())
    });
}

#[cfg(feature = "json")]
#[test]
fn json_string_and_file() {
    assert_eq!(Settings::from_json(r#"{"port": 1}"#).unwrap().port, 1);
    Jail::expect_with(|jail| {
        jail.create_file("app.json", r#"{"port": 2}"#)?;
        let s: Settings = Loader::builder().file("app.json").build().load().unwrap();
        assert_eq!(s, Settings::builder().port(2).build());
        Ok(())
    });
}

#[cfg(feature = "yaml")]
#[test]
fn yaml_string_and_file() {
    assert_eq!(Settings::from_yaml("port: 1").unwrap().port, 1);
    Jail::expect_with(|jail| {
        jail.create_file("app.yml", "port: 2")?;
        jail.create_file("named.YAML", "name: named")?;
        let s: Settings = Loader::builder()
            .file("app.yml")
            .config_path("named.YAML")
            .build()
            .load()
            .unwrap();
        assert_eq!(s, Settings::builder().port(2).name("named").build());
        Ok(())
    });
}
