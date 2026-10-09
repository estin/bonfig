# bonfig design notes

bonfig is a small Rust crate for application configuration. figment does the
merge work, bon gives the builders, and a few helpers load configuration from
defaults, files, and environment variables. This file records the patterns
from four existing projects and the design that comes from them.

## Projects analyzed

| Project | Loader | Defaults | Environment variables | Files |
|---|---|---|---|---|
| invitor-services: `core-type/src/settings.rs`, `im-types/src/settings.rs` | figment, bon function builder `settings::new()` | `SmartDefault`, `#[serde(default)]` | `IM__A__B` | fixed path if it exists, then the path in `IM_CONFIG_FILE` |
| worker-rust: `wr-type/src/settings.rs` | figment, `load()` function | `SmartDefault`, hand-written bon builders | `WR__A__B` | the path in `WR_CONFIG_FILE` |
| barduck: `src/config/` | `toml`, hand-written overrides | `#[serde(default = "fn")]`, manual `impl Default` | `BARDUCK_FIELD`, top level only | `--config`, `BARDUCK_CONFIG`, XDG, `./config.toml`, first found wins |
| simple-completion-language-server | `toml`, `etcetera`, `std::env::var` | manual `impl Default` | no prefix (`SNIPPETS_PATH`) | XDG directory through `etcetera` |

## Your configuration taste

These patterns repeat across the projects:

1. Every configuration struct has a full default. The app starts with no file
   and no environment variables. Defaults point to a local development setup
   (`localhost`, `"secret"`).
2. Defaults live next to the field: `#[derive(SmartDefault)]` with
   `#[default(...)]`, and `#[serde(default)]` on the struct. A partial file
   sets only the keys it names.
3. Sources are layers. From low to high priority: struct defaults, TOML file,
   environment variables.
4. Environment variables use an app prefix and a double underscore for
   nesting: `WR__ENDPOINT__PORT=8081`. Single underscores stay inside field
   names. Values use TOML syntax, so `APP__TOPICS=["a","b"]` is a list.
5. One environment variable gives the file path. Its name is outside the value
   namespace: `WR_CONFIG_FILE`, not `WR__CONFIG_FILE`.
6. Maps merge key by key. A value for one map key keeps the other default
   keys. The `dict_update` test in `core-type` covers this.
7. Nested sections inherit from shared sections (invitor only). `cache.redis`
   starts from `[redis]`. `broker.incoming.dialog` starts from `[broker]`,
   then `[broker.incoming]`, then its own keys. This file calls this the
   focus cascade.
8. bon builders give short default values for nested structs:
   `KafkaTopicSettings::builder().name("execute-run").build()`.
9. Durations are human-readable strings (`"5s"`, `"1 day"`) through
   `humantime_serde`. Older code uses `_ms: u64` fields.
10. The app logs the final configuration at startup and hides secrets in that
    log.
11. `FromStr` or `TryFrom<&str>` builds a configuration from inline TOML in
    tests.
12. The settings crate re-exports figment (`pub use figment;`), so other
    crates do not add it.

## Problems in the current code

Each problem is a reason for a rule in the design below.

1. A list from a file or an environment variable cannot replace a default
   list. Both figment loaders use `admerge` or `adjoin`, and these join
   arrays. A test with figment 0.10.19, the default `["apifornia.com"]`, and
   `APP__DOMAINS=["x.com"]` gives these results:
   - worker-rust: `["apifornia.com", "x.com"]`
   - invitor: `["x.com", "apifornia.com"]`
   - plain `merge`: `["x.com"]`

   `KafkaTopicSettings.config: Vec<(String, String)>` has the same problem.
   An override adds a second `retention.ms` pair.
2. A wrong file path does not cause an error. worker-rust uses `Toml::file`.
   That function ignores a missing file and searches parent directories for a
   relative path. invitor skips a missing file after `std::fs::exists`
   (`core-type/src/settings.rs:432`). A typo in `IM_CONFIG_FILE` starts the app
   with defaults. barduck does this correctly: a named path must exist.
3. Secrets go to the log. invitor logs the full `ServerSettings` at `info`
   (`core-type/src/settings.rs:453`). The log includes `secret`,
   `internal_shared_secret`, `otlp_tracer_auth_header`, and database URLs.
   worker-rust hides secrets with a hand-written `HideSecrets` trait and
   `Debug` impls. It does not hide
   `StorageClientSettings.persistent_storage_jwt_key`. Hand-written masking is
   easy to forget.
4. Two sources of defaults disagree. worker-rust `MachineConfig` has both
   `#[builder(default)]` and `#[default(1)]`. `MachineConfig::default()` gives
   `vcpu_count: 1, mem_size_mib: 256`. `MachineConfig::builder().build()`
   gives `0, 0`. In the same file, `HttpEndpointSettings` and
   `KafkaTopicSettings` have hand-written bon builders. Each one copies every
   `Option` argument onto `Self::default()`.
5. Hand-written partial structs have bugs. simple-completion-language-server
   reads `max_path_chars` into `max_chars_prefix_len` and ignores the partial
   `max_chars_prefix_len` (`src/lib.rs:98`). worker-rust has
   `PythonResourceLimitPartial::merge` with the same pattern. figment can
   merge a partial value onto a full struct with no extra type.
6. The focus cascade runs inside `Default`. Each
   `#[default(settings().focus("redis").call().unwrap())]` reads the
   environment and the files again, and panics on a bad value. One
   `ServerSettings::default()` in `im-types` does this more than 12 times.
   `T::default()` also reads the process environment, so tests are not
   isolated.
7. `set_default_envs` gives app defaults through
   `unsafe { std::env::set_var }` (`core-type/src/settings.rs:367`). A figment
   layer gives the same result without `unsafe`.
8. `build_from_str` in worker-rust also reads `WR__*` environment variables. A
   test with inline TOML can get values from the shell of the developer.
9. The `SettingBuilder` trait exists only to return the bon typestate builder
   from a preset function. A plain struct as the preset removes the trait.
10. barduck has one `if let Some(v) = lookup("BARDUCK_...")` block per field
    and one parse function per type. figment `Env` does this for all fields,
    nested fields too. Its errors name the environment variable.

## Decisions

The numbers in brackets point to the problems above. `README.md` shows the
API.

1. Lists replace. All layers use figment `merge`. Maps still merge key by
   key. (1)
2. Defaults live only in bon `#[builder(default = ...)]`. `Default` comes
   from the builder, so the two cannot disagree. No `SmartDefault`. (4)
3. The focus cascade is an explicit `Loader::load_focused` call after `load`.
   `Default` reads no files and no environment variables. (6)
4. A named file (`config_path` or `{PREFIX}_CONFIG_FILE`) must exist.
   All files use `file_exact`, so bonfig does not search parent
   directories. (2)
5. `Secret<T>` prints `***` in `Debug`. With the `tracing` feature, `load`
   logs the result with `Debug` at `info`. (3)
6. The `FromSource` trait builds a configuration from one source over
   `T::default()`: `from_provider`, `from_serialize`, and `from_toml`,
   `from_json`, `from_yaml`. It reads no environment variables unless the
   source does (8). `patch` replaces the partial structs (5). A plain `Loader`
   struct replaces `SettingBuilder` (9).
7. `bonfig::Error` is `Box<figment::Error>`. A `figment::Error` is large, and
   clippy warns about `result_large_err` without the box.
8. The file extension picks the format: `toml`, `json`, `yaml` or `yml`,
   each behind a Cargo feature. `toml` and `tracing` are default features.
   `Loader::figment(&base)` returns the `Figment` for other providers, such
   as CLI flags.
9. The license is MIT.
10. bonfig re-exports `bon`, `figment` and `serde`. An app derives
    `bonfig::bon::Builder` with `#[builder(crate = ::bonfig::bon)]` and the
    serde derives with `#[serde(crate = "bonfig::serde")]`. It then needs no
    `bon` or `serde` dependency of its own. bon requires an absolute path
    there.
11. The second crate, `bonfig-macros`, gives `#[bonfig::settings]`. It adds
    the serde and bon derives with the `crate` paths, `#[serde(default)]`,
    `on(String, into)`, and `Default` through the builder. A field without
    `#[builder(default ...)]` gets the default of its type, like a field
    without `#[default]` in `SmartDefault`. The macro does not derive `Debug`
    or `Clone`, so the app controls them. `default_from_builder!` stays for
    structs written by hand, where a field without a default does not
    compile.

## Known limits

These come from figment. The tests in `tests/loader.rs` and
`tests/lists_and_nested.rs` show each one.

1. A string environment variable that looks like a number needs quotes:
   `APP__NAME='"123"'`. Without quotes, the error names the key and the
   `APP__` source.
2. In `patch`, a JSON `null` sets an `Option` field to `None`. A `null` for
   any other field is an error.
3. An environment variable cannot change one list item
   (`APP__UPSTREAMS__0__NAME`). Use a map of objects for that.
4. figment makes environment variable names lowercase, so a map key from an
   environment variable is always lowercase.

## Not in scope

- Domain types (`HttpSettings`, `PostgresSettings`, `KafkaSettings`,
  `RedisSettings`) stay in the app crates. Note that `SocketAddr` parses from
  `"0.0.0.0:8080"`. One `SocketAddr` field can replace a `host`, a `port`,
  and an `as_addr()` method.
- A `validate()` hook. The app calls its own `validate()` after `load`.
- Detection of unknown keys. `#[serde(deny_unknown_fields)]` works for files,
  but it fails on any stray `{PREFIX}__*` environment variable. invitor uses
  the `INVITOR_SERVICE` prefix for many services, so it fails there.
- A re-export of `humantime_serde`. Apps add it directly.
