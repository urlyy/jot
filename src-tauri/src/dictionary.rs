//! 用户词典条目的解析。
//!
//! 每行一个条目，支持两种写法：
//!
//! ```text
//! 联调
//! 联调 = 接口联合调试，不是“连调”
//! ```
//!
//! - 转写接口只使用「术语」本身做偏置（说明对声学模型没有意义）；
//! - 润色提示词里会带上「说明」，帮助模型选对同音字、保持术语一致。

/// 一个词典条目。`term` 一定非空；`note` 是可选说明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryEntry<'a> {
    pub term: &'a str,
    pub note: Option<&'a str>,
}

/// 说明与术语之间的分隔符。只认这两种，避免把 URL、时间（`10:30`）等误切。
const NOTE_SEPARATORS: [&str; 2] = ["=", "："];

/// 解析一行词典文本；空行或只有说明没有术语时返回 `None`。
pub fn parse_entry(raw: &str) -> Option<DictionaryEntry<'_>> {
    let line = raw.trim();
    if line.is_empty() {
        return None;
    }

    let split_at = NOTE_SEPARATORS
        .iter()
        .filter_map(|separator| line.find(separator).map(|index| (index, separator.len())))
        .min_by_key(|(index, _)| *index);

    match split_at {
        Some((index, separator_len)) => {
            let term = line[..index].trim();
            let note = line[index + separator_len..].trim();
            if term.is_empty() {
                return None;
            }
            Some(DictionaryEntry {
                term,
                note: (!note.is_empty()).then_some(note),
            })
        }
        None => Some(DictionaryEntry {
            term: line,
            note: None,
        }),
    }
}

/// 所有条目里的术语，按原顺序去重后返回。
pub fn terms(dictionary: &[String]) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for raw in dictionary {
        let Some(entry) = parse_entry(raw) else {
            continue;
        };
        if !seen.contains(&entry.term) {
            seen.push(entry.term);
        }
    }
    seen
}

/// 解析全部条目（跳过空行与无效行）。
pub fn entries(dictionary: &[String]) -> Vec<DictionaryEntry<'_>> {
    dictionary.iter().filter_map(|raw| parse_entry(raw)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_term_has_no_note() {
        let entry = parse_entry("  Kubernetes  ").unwrap();
        assert_eq!(entry.term, "Kubernetes");
        assert_eq!(entry.note, None);
    }

    #[test]
    fn equals_sign_splits_term_and_note() {
        let entry = parse_entry("联调 = 接口联合调试，不是连调").unwrap();
        assert_eq!(entry.term, "联调");
        assert_eq!(entry.note, Some("接口联合调试，不是连调"));
    }

    #[test]
    fn fullwidth_colon_splits_term_and_note() {
        let entry = parse_entry("SeedASR：火山引擎流式识别").unwrap();
        assert_eq!(entry.term, "SeedASR");
        assert_eq!(entry.note, Some("火山引擎流式识别"));
    }

    #[test]
    fn ascii_colon_is_kept_so_urls_and_times_stay_intact() {
        let entry = parse_entry("http://localhost:8000/v1").unwrap();
        assert_eq!(entry.term, "http://localhost:8000/v1");
        assert_eq!(entry.note, None);
    }

    #[test]
    fn blank_lines_and_note_only_lines_are_ignored() {
        assert!(parse_entry("   ").is_none());
        assert!(parse_entry("= 只有说明").is_none());
        let dictionary = vec!["".to_string(), "  ".to_string(), "= x".to_string()];
        assert!(terms(&dictionary).is_empty());
    }

    #[test]
    fn terms_are_deduplicated_and_keep_the_first_spelling() {
        let dictionary = vec![
            "联调 = 接口联合调试".to_string(),
            "联调".to_string(),
            "Whisper".to_string(),
        ];
        assert_eq!(terms(&dictionary), vec!["联调", "Whisper"]);
    }
}
