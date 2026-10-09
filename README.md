# bonfig

bonfig loads application configuration in layers.
[figment](https://crates.io/crates/figment) merges the layers, and
[bon](https://crates.io/crates/bon) builders hold the defaults. The crate
keeps one way to do configuration for all my Rust apps.

## Layers

From low to high priority:

1. Struct defaults from `T::default()`.
2. Optional files, in the order that you add them. bonfig skips a missing
   optional file.
3. The named file. The path comes from `config_path`, for example from a
   `--config` flag, or from the `{PREFIX}_CONFIG_FILE` environment variable
   (env var). If this file does not exist, `load` returns an error.
4. Env vars `{PREFIX}__A__B` for the key `a.b`. A double underscore separates
   the levels, so field names can contain single underscores. Values use TOML
   syntax: `APP__TAGS=["a","b"]` is a list. A value that looks like a number
   or a bool is parsed as one. For a string field, quote it:
   `APP__NAME='"123"'`. Without quotes, `load` returns an error that names the
   key.

All layers merge in the same way. Maps merge key by key. Lists and scalars
replace the lower value.

The file extension picks the format: `.toml`, `.json`, `.yaml` or `.yml`.
Each format needs its Cargo feature. For another extension, `load` returns an
error.

## Example

```rust
use bonfig::{Loader, Secret};

#[bonfig::settings]
#[derive(Debug, Clone)]
pub struct Settings {
    #[builder(default = "127.0.0.1:8000")]
    pub listen: String,
    #[builder(default = Secret::new("dev-secret".to_owned()))]
    pub token: Secret<String>,
    pub redis: Redis,
    #[builder(default = Redis::builder().db(1).build())]
    pub cache: Redis,
}

#[bonfig::settings]
#[derive(Debug, Clone)]
pub struct Redis {
    #[builder(default = "redis://127.0.0.1")]
    pub url: String,
    pub db: u8,
}

/// One preset per app.
pub fn config() -> Loader {
    Loader::builder()
        .env_prefix("APP") // APP__REDIS__URL and APP_CONFIG_FILE
        .file("/etc/app/defaults.toml")
        .files(bonfig::xdg_config_file("app"))
        .build()
}

fn main() -> bonfig::Result<()> {
    let mut settings: Settings = config().load()?;
    // `cache` gets the `[redis]` keys first, then its own `[cache]` keys win.
    settings.cache = config().load_focused(&["redis", "cache"], settings.cache)?;
    Ok(())
}
```

## Configuration structs

Put `#[bonfig::settings]` on each configuration struct, above its
`#[derive(...)]`. The attribute does these things:

1. It derives `Serialize`, `Deserialize` and `bon::Builder` through the
   crates that bonfig re-exports. The app needs no `serde` or `bon`
   dependency of its own.
2. It adds `#[serde(default)]`, so a file or an env var sets only the keys
   that it names.
3. It adds `#[builder(on(String, into))]`, so a `String` setter takes a
   `&str`.
4. It gives each field without `#[builder(default ...)]` the default of its
   type. `Option` fields are `None`.
5. It implements `Default` as `Self::builder().build()`, so the builder and
   `Default` cannot disagree.

Derive `Debug` yourself, because `load` logs the result with it. You can add
more `#[serde(...)]` and `#[builder(...)]` attributes to the struct.

Rules for fields:

1. Set a default with `#[builder(default = ...)]`. The setter conversion
   applies to the value too, so `#[builder(default = "dev")]` works for a
   `String` field.
2. Wrap secrets in `bonfig::Secret`. `Debug` prints them as `***`, so the
   startup log of `load` does not show them.
3. Use `std::net::SocketAddr` for listen addresses, and `humantime_serde` for
   durations such as `"5s"`.

### Without the macro

This struct is the same as `Redis` above, written by hand. Here, each field
must have `#[builder(default ...)]`, or the code does not compile. If the app
depends on `bon` or `serde` directly, you can omit its `crate` part.

```rust
use bonfig::bon::Builder;
use bonfig::serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[serde(default, crate = "bonfig::serde")]
#[builder(crate = ::bonfig::bon, on(String, into))]
pub struct Redis {
    #[builder(default = "redis://127.0.0.1")]
    pub url: String,
    #[builder(default)]
    pub db: u8,
}

bonfig::default_from_builder!(Redis);
```

## Build from one source

The `FromSource` trait builds a configuration from one source over
`T::default()`. It reads no env vars and no files, unless the source does.
Every type with `Default`, `Serialize` and `Deserialize` has it. Use it in
tests and for configuration that comes from somewhere other than the
`Loader` layers.

```rust
use bonfig::FromSource as _;

#[derive(Debug, Default, bonfig::serde::Serialize, bonfig::serde::Deserialize)]
#[serde(default, crate = "bonfig::serde")]
struct Settings {
    port: u16,
}

// A string in a format (feature `toml`, `json` or `yaml`).
let s = Settings::from_toml("port = 1")?;
// Any `Serialize` value: a `serde_json::Value`, a map, or a struct.
let s = Settings::from_serialize(serde_json::json!({"port": 2}))?;
// Any figment provider.
let s = Settings::from_provider(bonfig::figment::providers::Env::prefixed("APP__"))?;
# Ok::<(), bonfig::Error>(())
```

`from_json` and `from_yaml` work like `from_toml`.

## Lists and nested objects

The examples in this section use the configuration below.
`tests/lists_and_nested.rs` runs each example, the env var forms too.

```rust
use std::collections::BTreeMap;

use bonfig::FromSource as _;

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
pub struct App {
    // A list of scalars.
    #[builder(default = vec!["a.com".to_owned()])]
    pub domains: Vec<String>,
    // A list of objects. Its default has one item.
    #[builder(default = vec![Upstream::builder().name("main").build()])]
    pub upstreams: Vec<Upstream>,
    // A nested object with its own defaults.
    pub db: Db,
    // A map of scalars.
    #[builder(default = BTreeMap::from([("acks".to_owned(), "1".to_owned())]))]
    pub kafka: BTreeMap<String, String>,
    // A map of objects: empty by default.
    pub services: BTreeMap<String, Upstream>,
    // An optional object: `None` by default.
    pub tls: Option<Tls>,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
pub struct Upstream {
    #[builder(default = "upstream")]
    pub name: String,
    #[builder(default = "http://127.0.0.1")]
    pub url: String,
    #[builder(default = 1)]
    pub weight: u32,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
pub struct Db {
    #[builder(default = "postgres://localhost/app")]
    pub url: String,
    pub pool: Pool,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    #[builder(default = 10)]
    pub max: u32,
    #[builder(default = 1)]
    pub min: u32,
}

#[bonfig::settings]
#[derive(Debug, Clone, PartialEq)]
pub struct Tls {
    #[builder(default = "cert.pem")]
    pub cert: String,
    #[builder(default = "key.pem")]
    pub key: String,
}

let app = App::from_toml(
    r#"
    domains = ["b.com"]

    [[upstreams]]
    name = "a"

    [db.pool]
    max = 50

    [kafka]
    retries = "5"
    "#,
)?;
assert_eq!(app.domains, ["b.com"]);
assert_eq!(app.upstreams, [Upstream::builder().name("a").build()]);
assert_eq!((app.db.url.as_str(), app.db.pool.max, app.db.pool.min), ("postgres://localhost/app", 50, 1));
assert_eq!((app.kafka["acks"].as_str(), app.kafka["retries"].as_str()), ("1", "5"));
# Ok::<(), bonfig::Error>(())
```

Put `#[bonfig::settings]` on every struct, the nested ones too. A new list
item or a new map value then gets its missing fields from the default of its
type.

### Nested objects

```toml
[db.pool]
max = 50
```

The env var is `APP__DB__POOL__MAX=50`, or an inline table:
`APP__DB='{pool={max=50}}'`. Nested objects merge, so `db.url` and
`db.pool.min` keep their defaults.

### Lists

```toml
domains = ["b.com", "c.com"]
```

The env var is `APP__DOMAINS='["b.com", "c.com"]'`. `APP__DOMAINS='[]'` sets
an empty list. A list replaces the default list. To keep the default items,
write them again: `domains = ["a.com", "b.com"]`.

### Lists of objects

```toml
[[upstreams]]
name = "a"

[[upstreams]]
name = "b"
weight = 5
```

The env var is `APP__UPSTREAMS='[{name="a"}, {name="b", weight=5}]'`. The new
list replaces the default list. An item gets its missing fields from
`Upstream::default()`, not from the items of the default list. You cannot
change one item: `APP__UPSTREAMS__0__NAME=a` is an error for the key
`upstreams`. If you must change items one by one, use a map of objects.

### Maps

```toml
[kafka]
retries = "5"
"bootstrap.servers" = "b:9092"
```

The env var is `APP__KAFKA__RETRIES='"5"'`. Maps merge key by key, so `acks`
keeps its default `"1"`. Two rules apply to map keys in env vars:

1. bonfig makes env var names lowercase. `APP__KAFKA__MAX_POLL` sets the key
   `max_poll`. For a key with uppercase letters, use a file.
2. For a key with a dot, use an inline table:
   `APP__KAFKA='{"bootstrap.servers"="b:9092"}'`. The other keys stay.

### Maps of objects

```toml
[services.auth]
url = "http://auth"
```

The env var is `APP__SERVICES__AUTH__URL=http://auth`. A new key gets its
missing fields from `Upstream::default()`. An existing key merges, so a map of
objects lets you change one item by its key.

### Optional objects

`tls` is `None` by default. Any key in `[tls]` makes it `Some`, and the other
fields come from `Tls::default()`. TOML and env vars cannot set `None`. If an
optional object is `Some` by default, add an `enable: bool` field to turn the
feature off.

## Cargo features

| Feature | Default | What it adds |
|---|---|---|
| `toml` | yes | `.toml` files and `from_toml` |
| `tracing` | yes | logs each loaded file and the loaded configuration at `info` |
| `json` | no | `.json` files and `from_json` |
| `yaml` | no | `.yaml` and `.yml` files and `from_yaml` |
| `test` | no | `bonfig::figment::Jail`, which sets env vars and files for one test without `unsafe` |

## Other helpers

- `Loader::load_focused(&["redis", "cache"], base)` loads one value from a
  cascade of sections, general first. Call it after `load`.
- `bonfig::patch(&current, changes)` merges a partial value onto the current
  configuration. An example is JSON from an LSP client. A JSON `null` sets an
  `Option` field to `None`. A `null` for any other field is an error.
- `Loader::figment(&base)` returns all layers as a `Figment`. Use it to merge
  more providers, such as CLI flags.

## AI assistance

The author developed this crate with AI assistance. Claude, an AI model by
Anthropic, wrote much of the code, the tests, and the documentation. The
author directed the design and the decisions.

## Related crates

- [figment](https://crates.io/crates/figment) merges the layers.
- [bon](https://crates.io/crates/bon) builds the settings structs.

## License

MIT
