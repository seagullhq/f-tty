//! Search over the font families reported by the platform text system.

/// Case-insensitive substring match, in the order supplied by the caller.
pub fn filter<'a>(families: &'a [String], query: &str) -> Vec<&'a str> {
    let query = query.trim().to_lowercase();
    families
        .iter()
        .map(String::as_str)
        .filter(|name| name.to_lowercase().contains(&query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter;

    #[::core::prelude::v1::test]
    fn filter_is_case_insensitive_and_substring_based() {
        let fonts = vec![
            "Consolas".to_string(),
            "Cascadia Code".to_string(),
            "Menlo".to_string(),
        ];
        assert_eq!(
            filter(&fonts, ""),
            vec!["Consolas", "Cascadia Code", "Menlo"]
        );
        assert_eq!(filter(&fonts, "CAS"), vec!["Cascadia Code"]);
        assert_eq!(filter(&fonts, "nsol"), vec!["Consolas"]);
        assert!(filter(&fonts, "missing").is_empty());
    }
}
