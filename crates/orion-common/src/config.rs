#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigEnvironment {
    Development,
    Test,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretClass {
    Public,
    Sensitive,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigValidation {
    NonEmpty,
    Boolean,
    PositiveInteger,
    SocketAddress,
    Url,
    PostgresUrl,
    RedisUrl,
    CsvOrigins,
    LogFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKeySpec {
    pub key: &'static str,
    pub owner: &'static str,
    pub environments: &'static [ConfigEnvironment],
    pub secret_class: SecretClass,
    pub validation: ConfigValidation,
}

const ALL: &[ConfigEnvironment] = &[
    ConfigEnvironment::Development,
    ConfigEnvironment::Test,
    ConfigEnvironment::Production,
];
const RUNTIME: &[ConfigEnvironment] = &[
    ConfigEnvironment::Development,
    ConfigEnvironment::Production,
];
const PRODUCTION: &[ConfigEnvironment] = &[ConfigEnvironment::Production];

macro_rules! key {
    ($name:literal, $owner:literal, $envs:expr, $secret:ident, $validation:ident) => {
        ConfigKeySpec {
            key: $name,
            owner: $owner,
            environments: $envs,
            secret_class: SecretClass::$secret,
            validation: ConfigValidation::$validation,
        }
    };
}

pub const CONFIG_KEYS: &[ConfigKeySpec] = &[
    key!("APP_ENV", "divi912", ALL, Public, NonEmpty),
    key!(
        "API_BIND_ADDRESS",
        "divi912",
        RUNTIME,
        Public,
        SocketAddress
    ),
    key!("DATABASE_URL", "divi912", ALL, Secret, PostgresUrl),
    key!(
        "DATABASE_MAX_CONNECTIONS",
        "divi912",
        RUNTIME,
        Public,
        PositiveInteger
    ),
    key!("REDIS_URL", "divi912", ALL, Secret, RedisUrl),
    key!(
        "SESSION_TTL_SECONDS",
        "divi912",
        RUNTIME,
        Public,
        PositiveInteger
    ),
    key!("SESSION_COOKIE_SECURE", "divi912", RUNTIME, Public, Boolean),
    key!(
        "CORS_ALLOWED_ORIGINS",
        "divi912",
        RUNTIME,
        Sensitive,
        CsvOrigins
    ),
    key!(
        "REQUEST_TIMEOUT_SECONDS",
        "divi912",
        RUNTIME,
        Public,
        PositiveInteger
    ),
    key!("RUST_LOG", "divi912", ALL, Public, LogFilter),
    key!("LOG_FORMAT", "divi912", ALL, Public, NonEmpty),
    key!(
        "DISCORD_INVITE_URL",
        "sudhanshu001122",
        PRODUCTION,
        Sensitive,
        Url
    ),
    key!("NEWS_API_BASE_URL", "sudhanshu001122", RUNTIME, Public, Url),
    key!("NEWS_API_KEY", "sudhanshu001122", RUNTIME, Secret, NonEmpty),
];

#[must_use]
pub fn config_key(name: &str) -> Option<&'static ConfigKeySpec> {
    CONFIG_KEYS.iter().find(|entry| entry.key == name)
}
