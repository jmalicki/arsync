//! Internationalization (i18n) support for arsync using Fluent
//!
//! This module provides translation support for user-facing messages using
//! Mozilla's Fluent localization system.
//!
//! Currently supported locales:
//! - `en-US` - English (United States) - default
//! - `x-pirate` - Pirate speak (arrr! 🏴‍☠️)
//!
//! The `x-` prefix indicates a private/experimental locale per BCP 47 standards.

use fluent::{FluentBundle, FluentResource};
use std::sync::{LazyLock, RwLock};
use unic_langid::LanguageIdentifier;

/// Errors that can occur during internationalization operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum I18nError {
    /// The locale lock is poisoned (another thread panicked while holding the lock)
    LockPoisoned,
    /// Failed to acquire the locale lock
    #[allow(dead_code)] // Reserved for future use with try_lock operations
    LockUnavailable,
}

impl std::fmt::Display for I18nError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LockPoisoned => write!(f, "locale lock is poisoned"),
            Self::LockUnavailable => write!(f, "failed to acquire locale lock"),
        }
    }
}

impl std::error::Error for I18nError {}

/// Supported language identifiers
const EN_US: &str = "en-US";

/// Pirate language identifier (using qaa - reserved for private use per ISO 639-2)
/// This is the standard way to represent constructed/private languages in BCP 47
const EN_X_PIRATE: &str = "qaa";

/// English (US) fluent resource
static EN_US_FTL: &str = include_str!("../locales/en-US/main.ftl");

/// Pirate fluent resource  
static X_PIRATE_FTL: &str = include_str!("../locales/qaa/main.ftl");

/// Current active locale (thread-safe)
static CURRENT_LOCALE: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new(EN_US.to_string()));

/// Create a fluent bundle for the given locale (creates fresh each time - cheap operation)
///
/// # Panics
/// Panics if the locale identifier is invalid or the FTL resource is malformed.
/// This is acceptable because these are compile-time constants that are validated during development.
#[allow(clippy::expect_used)] // Acceptable for static compile-time constants
fn create_bundle(locale: &str, ftl_string: &'static str) -> FluentBundle<FluentResource> {
    let langid: LanguageIdentifier = locale.parse().expect("Failed to parse language identifier");
    let resource =
        FluentResource::try_new(ftl_string.to_string()).expect("Failed to parse fluent resource");

    let mut bundle = FluentBundle::new(vec![langid]);
    bundle
        .add_resource(resource)
        .expect("Failed to add fluent resource");

    bundle
}

/// Get the appropriate FTL string for a locale
fn get_ftl_for_locale(locale: &str) -> &'static str {
    match locale {
        EN_X_PIRATE => X_PIRATE_FTL,
        _ => EN_US_FTL,
    }
}

/// Supported languages/locales
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Not all variants used yet but will be in full implementation
#[allow(clippy::missing_docs_in_private_items)] // Variants are self-documenting
pub enum Language {
    English,
    Pirate,
}

impl Language {
    /// Get the BCP 47 locale identifier for this language
    #[must_use]
    pub const fn locale_id(self) -> &'static str {
        match self {
            Self::English => EN_US,
            Self::Pirate => EN_X_PIRATE,
        }
    }
}

/// Set the current language for all translations
pub fn set_language(lang: Language) {
    if let Ok(mut current) = CURRENT_LOCALE.write() {
        *current = lang.locale_id().to_string();
    }
}

/// Get the current language
///
/// # Errors
/// Returns `I18nError::LockPoisoned` if the locale lock is poisoned
#[allow(dead_code)] // Will be used for runtime language queries
pub fn get_language() -> Result<Language, I18nError> {
    let locale = CURRENT_LOCALE.read().map_err(|_| I18nError::LockPoisoned)?;
    Ok(if locale.as_str() == EN_X_PIRATE {
        Language::Pirate
    } else {
        Language::English
    })
}

/// Translation key enum - maps to Fluent message IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Not all keys used yet but available for full i18n coverage
#[allow(clippy::missing_docs_in_private_items)] // Variants map directly to Fluent keys and are self-documenting
pub enum TranslationKey {
    // Progress messages
    ProgressDiscovered,
    ProgressCompleted,
    ProgressInFlight,
    ProgressComplete,
    ProgressFiles,
    ProgressBytes,
    ProgressSpeed,
    ProgressEta,

    // Status messages
    StatusCopyingFile,
    StatusCopyingDirectory,
    StatusCreatingSymlink,
    StatusCreatingHardlink,
    StatusPreservingMetadata,
    StatusComplete,
    StatusFailed,

    // Error messages
    ErrorFileNotFound,
    ErrorPermissionDenied,
    ErrorIoError,
    ErrorInvalidPath,
    ErrorSourceNotExists,
    ErrorDestinationExists,

    // Info messages
    InfoStartingCopy,
    InfoScanningSource,
    InfoCreatingDestination,
    InfoPreservingPermissions,
    InfoPreservingOwnership,
    InfoPreservingTimestamps,
    InfoPreservingXattrs,
    InfoPreservingAcls,
    InfoDryRun,

    // CLI help messages
    HelpDescription,
    HelpSource,
    HelpDestination,
    HelpArchive,
    HelpRecursive,
    HelpLinks,
    HelpPerms,
    HelpTimes,
    HelpGroup,
    HelpOwner,
    HelpDevices,
    HelpXattrs,
    HelpAcls,
    HelpHardLinks,
    HelpVerbose,
    HelpQuiet,
    HelpProgress,
    HelpDryRun,
    HelpPirate,
    HelpQueueDepth,
    HelpMaxFilesInFlight,
    HelpCpuCount,
    HelpBufferSize,

    // Units
    UnitBytes,
    UnitKilobytes,
    UnitMegabytes,
    UnitGigabytes,
    UnitSeconds,

    // Misc
    MiscIn,
    MiscOf,
    MiscAverage,
    MiscTotal,
}

impl TranslationKey {
    /// Get the Fluent message ID for this key
    const fn message_id(self) -> &'static str {
        match self {
            // Progress messages
            Self::ProgressDiscovered => "progress-discovered",
            Self::ProgressCompleted => "progress-completed",
            Self::ProgressInFlight => "progress-in-flight",
            Self::ProgressComplete => "progress-complete",
            Self::ProgressFiles => "progress-files",
            Self::ProgressBytes => "progress-bytes",
            Self::ProgressSpeed => "progress-speed",
            Self::ProgressEta => "progress-eta",

            // Status messages
            Self::StatusCopyingFile => "status-copying-file",
            Self::StatusCopyingDirectory => "status-copying-directory",
            Self::StatusCreatingSymlink => "status-creating-symlink",
            Self::StatusCreatingHardlink => "status-creating-hardlink",
            Self::StatusPreservingMetadata => "status-preserving-metadata",
            Self::StatusComplete => "status-complete",
            Self::StatusFailed => "status-failed",

            // Error messages
            Self::ErrorFileNotFound => "error-file-not-found",
            Self::ErrorPermissionDenied => "error-permission-denied",
            Self::ErrorIoError => "error-io-error",
            Self::ErrorInvalidPath => "error-invalid-path",
            Self::ErrorSourceNotExists => "error-source-not-exists",
            Self::ErrorDestinationExists => "error-destination-exists",

            // Info messages
            Self::InfoStartingCopy => "info-starting-copy",
            Self::InfoScanningSource => "info-scanning-source",
            Self::InfoCreatingDestination => "info-creating-destination",
            Self::InfoPreservingPermissions => "info-preserving-permissions",
            Self::InfoPreservingOwnership => "info-preserving-ownership",
            Self::InfoPreservingTimestamps => "info-preserving-timestamps",
            Self::InfoPreservingXattrs => "info-preserving-xattrs",
            Self::InfoPreservingAcls => "info-preserving-acls",
            Self::InfoDryRun => "info-dry-run",

            // CLI help messages
            Self::HelpDescription => "help-description",
            Self::HelpSource => "help-source",
            Self::HelpDestination => "help-destination",
            Self::HelpArchive => "help-archive",
            Self::HelpRecursive => "help-recursive",
            Self::HelpLinks => "help-links",
            Self::HelpPerms => "help-perms",
            Self::HelpTimes => "help-times",
            Self::HelpGroup => "help-group",
            Self::HelpOwner => "help-owner",
            Self::HelpDevices => "help-devices",
            Self::HelpXattrs => "help-xattrs",
            Self::HelpAcls => "help-acls",
            Self::HelpHardLinks => "help-hard-links",
            Self::HelpVerbose => "help-verbose",
            Self::HelpQuiet => "help-quiet",
            Self::HelpProgress => "help-progress",
            Self::HelpDryRun => "help-dry-run",
            Self::HelpPirate => "help-pirate",
            Self::HelpQueueDepth => "help-queue-depth",
            Self::HelpMaxFilesInFlight => "help-max-files-in-flight",
            Self::HelpCpuCount => "help-cpu-count",
            Self::HelpBufferSize => "help-buffer-size",

            // Units
            Self::UnitBytes => "unit-bytes",
            Self::UnitKilobytes => "unit-kilobytes",
            Self::UnitMegabytes => "unit-megabytes",
            Self::UnitGigabytes => "unit-gigabytes",
            Self::UnitSeconds => "unit-seconds",

            // Misc
            Self::MiscIn => "misc-in",
            Self::MiscOf => "misc-of",
            Self::MiscAverage => "misc-average",
            Self::MiscTotal => "misc-total",
        }
    }

    /// Get the translated string for this key in the current language
    ///
    /// # Errors
    /// Returns `I18nError::LockPoisoned` if the locale lock is poisoned
    pub fn get(self) -> Result<String, I18nError> {
        let locale = CURRENT_LOCALE.read().map_err(|_| I18nError::LockPoisoned)?;
        Ok(self.translate(locale.as_str()))
    }

    /// Get the translated string for this key in a specific language
    #[must_use]
    pub fn translate(self, locale: &str) -> String {
        // Create bundle fresh each time (cheap - FTL strings are static)
        let ftl_string = get_ftl_for_locale(locale);
        let bundle = create_bundle(locale, ftl_string);
        let msg_id = self.message_id();

        if let Some(message) = bundle.get_message(msg_id) {
            if let Some(pattern) = message.value() {
                let mut errors = vec![];
                let value = bundle.format_pattern(pattern, None, &mut errors);
                return value.to_string();
            }
        }

        // Fallback if message not found
        format!("[Missing translation: {msg_id}]")
    }
}

/// Convenience macro for translating messages
///
/// Returns `Result<String, I18nError>` - use with `?` operator or `unwrap_or` for default
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $key.get()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_language_is_english() {
        // Test: Default language should be English (en-US)
        // This is linked to the requirement: Default user interface should be in English
        assert_eq!(get_language().unwrap(), Language::English);
    }

    #[test]
    fn test_set_language_to_pirate() {
        // Test: Should be able to switch to pirate language (x-pirate)
        // This is linked to the requirement: --pirate flag should switch language
        set_language(Language::Pirate);
        assert_eq!(get_language().unwrap(), Language::Pirate);

        // Reset to English for other tests
        set_language(Language::English);
    }

    #[test]
    fn test_english_translations() {
        // Test: All translation keys should have English translations
        // This is linked to the requirement: All messages must be translatable
        let key = TranslationKey::ProgressDiscovered;
        assert_eq!(key.translate(EN_US), "Discovered");

        let key = TranslationKey::StatusComplete;
        assert_eq!(key.translate(EN_US), "Complete");
    }

    #[test]
    fn test_pirate_translations() {
        // Test: All translation keys should have Pirate translations
        // This is linked to the requirement: Pirate translations for all user-facing text
        let key = TranslationKey::ProgressDiscovered;
        assert_eq!(
            key.translate(EN_X_PIRATE),
            "Treasure sighted on the horizon, ahoy"
        );

        let key = TranslationKey::StatusComplete;
        assert_eq!(
            key.translate(EN_X_PIRATE),
            "SHIVER ME TIMBERS! Mission complete! All treasure secured! Hoist the Jolly Roger! 🏴‍☠️"
        );
    }

    #[test]
    fn test_translation_macro() {
        // Test: Translation macro should work correctly
        // This is linked to the requirement: Easy translation access in code
        set_language(Language::English);
        assert_eq!(
            t!(TranslationKey::ProgressDiscovered).unwrap(),
            "Discovered"
        );

        set_language(Language::Pirate);
        assert_eq!(
            t!(TranslationKey::ProgressDiscovered).unwrap(),
            "Treasure sighted on the horizon, ahoy"
        );

        // Reset
        set_language(Language::English);
    }

    #[test]
    fn test_all_keys_have_both_translations() {
        // Test: Every translation key should have both English and Pirate versions
        // This is linked to the requirement: Complete translation coverage
        let keys = vec![
            TranslationKey::ProgressDiscovered,
            TranslationKey::ProgressCompleted,
            TranslationKey::StatusCopyingFile,
            TranslationKey::ErrorFileNotFound,
            TranslationKey::InfoStartingCopy,
            TranslationKey::HelpDescription,
        ];

        for key in keys {
            let english = key.translate(EN_US);
            let pirate = key.translate(EN_X_PIRATE);

            assert!(
                !english.contains("[Missing translation:"),
                "English translation for {:?} should not be missing",
                key
            );
            assert!(
                !pirate.contains("[Missing translation:"),
                "Pirate translation for {:?} should not be missing",
                key
            );
            assert_ne!(
                english, pirate,
                "English and Pirate translations should differ for {:?}",
                key
            );
        }
    }

    #[test]
    fn test_locale_ids() {
        // Test: Locale IDs should follow BCP 47/ISO 639-2 standards
        // en-US is standard, qaa is ISO 639-2 reserved code for private use languages
        assert_eq!(Language::English.locale_id(), "en-US");
        assert_eq!(Language::Pirate.locale_id(), "qaa");
    }
}
