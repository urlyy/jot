//! Chinese system prompt construction for transcription polishing.

pub const POLISH_STYLE_MINIMAL: &str = "minimal";
pub const POLISH_STYLE_CLEAN: &str = "clean";
pub const POLISH_STYLE_STRUCTURED: &str = "structured";
pub const POLISH_STYLE_CUSTOM: &str = "custom";

/// Maximum number of dictionary entries embedded in the prompt.
const MAX_PROMPT_DICTIONARY_ENTRIES: usize = 100;
/// Characters of the user's custom requirement that are forwarded.
const MAX_CUSTOM_PROMPT_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, Default)]
pub struct PolishPromptOptions<'a> {
    pub dictionary: &'a [String],
    pub polish_style: &'a str,
    pub custom_prompt: &'a str,
}

const BASE_RULES: &str = "\
你是一个中文语音听写的文字润色助手。请严格遵守以下规则：
1. 修正错别字、同音字与错误的分词、标点。
2. 保持原意、语气与原文语言；原文是中文就输出中文，原文是英文就输出英文，不做翻译。
3. 不要增加原文没有的信息，不要编造事实、数字、人名或结论。
4. 不要解释、不要总结、不要描述你做了什么，只输出改写后的正文。
5. 不要添加「润色结果：」之类的前缀，也不要用引号包裹整段输出。";

fn style_section(style: &str) -> &'static str {
    match style {
        POLISH_STYLE_MINIMAL => {
            "【风格：极简】\n只修正明显的错别字和标点错误，尽量保留原来的句子结构、语序与口语表达，不要重写句子。"
        }
        POLISH_STYLE_STRUCTURED => {
            "【风格：结构化】\n在不增加任何信息的前提下，按语义整理出清晰的段落；若原文包含多个并列要点，可以用换行或简单的条目呈现，不要编造小标题。"
        }
        POLISH_STYLE_CUSTOM => {
            "【风格：自定义】\n按下面的「用户自定义要求」处理；如果该要求为空，则按「通顺整理」处理。"
        }
        _ => {
            "【风格：通顺整理】\n在保留原意的前提下让句子更通顺、标点更规范，去掉「嗯」「那个」「就是」之类的口头填充词，也去掉句首的「然后」「就是说」「也就是说」这类口头连接词，但不要改变说话人的表达方式。"
        }
    }
}

fn dictionary_section(dictionary: &[String]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for entry in crate::dictionary::entries(dictionary) {
        if seen.iter().any(|known| known == entry.term) {
            continue;
        }
        seen.push(entry.term.to_string());
        lines.push(match entry.note {
            Some(note) => format!("{}（{}）", entry.term, note),
            None => entry.term.to_string(),
        });
        if lines.len() >= MAX_PROMPT_DICTIONARY_ENTRIES {
            break;
        }
    }
    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "【用户词典】\n下面这些是用户的专有名词或术语，括号里是它们的含义；若原文出现相近的发音或写法，请按术语给出的写法输出，并保证语义与说明一致：\n{}",
        lines.join("、")
    ))
}

fn custom_section(custom_prompt: &str) -> Option<String> {
    let trimmed: String = custom_prompt
        .replace('\0', "")
        .trim()
        .chars()
        .take(MAX_CUSTOM_PROMPT_CHARS)
        .collect();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("【用户自定义要求】\n{trimmed}"))
    }
}

/// Build the complete system prompt: base rules, style, dictionary and the
/// optional user requirement.
pub fn build_system_prompt(options: PolishPromptOptions<'_>) -> String {
    let mut sections: Vec<String> = vec![BASE_RULES.to_string(), style_section(options.polish_style).to_string()];

    if let Some(section) = dictionary_section(options.dictionary) {
        sections.push(section);
    }
    if let Some(section) = custom_section(options.custom_prompt) {
        sections.push(section);
    }

    sections.join("\n\n")
}

/// The user message that carries the raw transcript.
pub fn build_user_message(raw_text: &str) -> String {
    format!("以下是语音听写的原始文字，请按规则润色后只输出正文：\n\n{}", raw_text.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt_for(style: &str, custom: &str) -> String {
        build_system_prompt(PolishPromptOptions {
            dictionary: &["Jot".to_string(), "Whisper".to_string()],
            polish_style: style,
            custom_prompt: custom,
        })
    }

    #[test]
    fn prompt_contains_fidelity_rules() {
        let prompt = prompt_for(POLISH_STYLE_CLEAN, "");
        assert!(prompt.contains("修正错别字"));
        assert!(prompt.contains("保持原意"));
        assert!(prompt.contains("不要增加原文没有的信息"));
        assert!(prompt.contains("只输出改写后的正文"));
    }

    #[test]
    fn styles_change_guidance() {
        assert!(prompt_for(POLISH_STYLE_MINIMAL, "").contains("极简"));
        assert!(prompt_for(POLISH_STYLE_CLEAN, "").contains("通顺整理"));
        assert!(prompt_for(POLISH_STYLE_STRUCTURED, "").contains("结构化"));
        assert!(prompt_for("unknown-style", "").contains("通顺整理"));
    }

    #[test]
    fn dictionary_and_custom_sections_are_optional() {
        let with_dictionary = prompt_for(POLISH_STYLE_CLEAN, "保持正式语气");
        assert!(with_dictionary.contains("用户词典"));
        assert!(with_dictionary.contains("Jot、Whisper"));
        assert!(with_dictionary.contains("保持正式语气"));

        let without = build_system_prompt(PolishPromptOptions {
            dictionary: &[],
            polish_style: POLISH_STYLE_CLEAN,
            custom_prompt: "  ",
        });
        assert!(!without.contains("用户词典"));
        assert!(!without.contains("用户自定义要求"));
    }

    #[test]
    fn user_message_wraps_the_transcript() {
        assert!(build_user_message("  你好世界 ").contains("你好世界"));
    }
}
