//! First-run UI language uses native language preferences, not region formats.

pub fn system_language() -> String {
    let native = native_language();
    let environment: Vec<_> = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"].into_iter()
        .filter_map(|key| std::env::var(key).ok()).collect();
    select_language(native.as_deref(), environment.iter().map(String::as_str)).to_string()
}

#[cfg(target_os = "macos")]
fn native_language() -> Option<String> {
    objc2::rc::autoreleasepool(|_| {
        objc2_foundation::NSLocale::preferredLanguages().firstObject().map(|language| language.to_string())
    })
}

#[cfg(not(target_os = "macos"))]
fn native_language() -> Option<String> { None }

fn select_language<'a>(native: Option<&str>, environment: impl IntoIterator<Item = &'a str>) -> &'static str {
    native.and_then(language_from_locale)
        .or_else(|| environment.into_iter().find_map(language_from_locale)).unwrap_or("en")
}

pub fn language_from_locale(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.split(['-', '.', '@', ':']).next().unwrap_or("") {
        "" | "c" | "posix" => None,
        "zh" => Some("zh-CN"),
        _ => Some("en"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_ui_language_has_priority_over_process_or_region_hints() {
        assert_eq!(select_language(Some("zh-Hans-CN"), ["C.UTF-8", "en_US.UTF-8"]), "zh-CN");
        assert_eq!(select_language(Some("en-US"), ["zh_CN.UTF-8"]), "en");
        assert_eq!(select_language(Some("fr-FR"), ["zh_CN"]), "en");
        assert_eq!(select_language(None, ["POSIX", "C.UTF-8", "zh_Hant_TW"]), "zh-CN");
        assert_eq!(select_language(None, ["C", "POSIX.UTF-8"]), "en");
        for neutral in ["", "C", "C.UTF-8", "POSIX", "POSIX.UTF-8"] {
            assert_eq!(language_from_locale(neutral), None);
        }
    }
}
