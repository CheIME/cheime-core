/// Ordered config layers. Higher priority = later in the enum, overrides
/// earlier layers during merge.
///
/// Resolution order (from DRAFT.md §config):
///   Session > App > Profile > Schema > Platform > System
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum ConfigLayer {
    /// Engine built-in defaults (lowest priority).
    System = 0,
    /// Platform-specific defaults (Windows/macOS/Linux conventions).
    Platform = 1,
    /// Schema definition — input logic, dictionaries, spelling rules.
    Schema = 2,
    /// User preference profile — appearance, shortcuts, page size.
    Profile = 3,
    /// Per-application overrides.
    App = 4,
    /// Session-level temporary overrides (incognito, single-switch).
    Session = 5,
}

impl ConfigLayer {
    /// All layers in ascending priority order.
    pub const ALL: [ConfigLayer; 6] = [
        ConfigLayer::System,
        ConfigLayer::Platform,
        ConfigLayer::Schema,
        ConfigLayer::Profile,
        ConfigLayer::App,
        ConfigLayer::Session,
    ];

    /// User-facing name for diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            ConfigLayer::System => "system",
            ConfigLayer::Platform => "platform",
            ConfigLayer::Schema => "schema",
            ConfigLayer::Profile => "profile",
            ConfigLayer::App => "app",
            ConfigLayer::Session => "session",
        }
    }
}
